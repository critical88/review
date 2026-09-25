import contextlib
import csv
import json
import os
import os.path as osp
import time
from abc import ABC, abstractmethod

import numpy as np
from gym import spaces

from baselines.common.tile_images import tile_images

class AlreadySteppingError(Exception):
    """
    Raised when an asynchronous step is running while
    step_async() is called again.
    """

    def __init__(self):
        msg = 'already running an async step'
        Exception.__init__(self, msg)


class NotSteppingError(Exception):
    """
    Raised when an asynchronous step is not running but
    step_wait() is called.
    """

    def __init__(self):
        msg = 'not running an async step'
        Exception.__init__(self, msg)


class VecEnv(ABC):
    """
    An abstract asynchronous, vectorized environment.
    Used to batch data from multiple copies of an environment, so that
    each observation becomes an batch of observations, and expected action is a batch of actions to
    be applied per-environment.

    Besides the vectorized stepping itself, a VecEnv also handles the
    bookkeeping a rollout needs next to the raw environment interaction:
    shaping observations by stacking the most recent frames, keeping
    running statistics of the observations and discounted returns and
    filtering them through those statistics, accumulating per-environment
    episode results and exporting them to a results file. Which of these
    are active is decided once, at construction time, so that the object
    handed to a training loop or an algorithm is the single owner of all
    of this state.
    """
    closed = False
    viewer = None

    metadata = {
        'render.modes': ['human', 'rgb_array']
    }

    RESULTS_EXT = "monitor.csv"

    def __init__(self, num_envs, observation_space, action_space,
                 frame_stack=None, normalize=False, use_tf=False,
                 clipob=10., cliprew=10., gamma=0.99, epsilon=1e-8,
                 record_episodes=False, results_filename=None, results_info_keywords=()):
        """
        Arguments:

        num_envs, observation_space, action_space:  batch size and the spaces of one copy of the environment

        frame_stack:      number of most recent observations to keep in the channel/last dimension
                           (None disables observation stacking)

        normalize:        whether observations and rewards are filtered through running
                           mean/std statistics before they are handed out

        use_tf:           when normalizing, keep the running statistics in tensorflow variables
                           instead of numpy arrays, so that they are checkpointed with the model

        clipob, cliprew:  clipping ranges for the normalized observations and rewards

        gamma, epsilon:   discounting used for the running statistics of returns, and the
                           numerical-stability constant used in the std denominators

        record_episodes:  whether episode-level results (returns, lengths) are accumulated per environment
                           and summarized by this vectorized environment

        results_filename: optional file the recorded episode results are written to, in the same
                           format as the per-environment results files

        results_info_keywords: extra info keys to store in the results file
        """
        self.num_envs = num_envs
        self.action_space = action_space
        self.raw_observation_space = observation_space

        # ----- observation stacking setup -----------------------------------------------
        if frame_stack is not None:
            assert isinstance(observation_space, spaces.Box), \
                'frame stacking requires a Box observation space, got {}'.format(observation_space)
            self.nstack = frame_stack
            stacked_low = np.repeat(observation_space.low, self.nstack, axis=-1)
            stacked_high = np.repeat(observation_space.high, self.nstack, axis=-1)
            self.stackedobs = np.zeros((num_envs,) + stacked_low.shape, stacked_low.dtype)
            observation_space = spaces.Box(low=stacked_low, high=stacked_high,
                                           dtype=observation_space.dtype)
        self.observation_space = observation_space

        # ----- running-statistics normalization setup -----------------------------------
        self.normalize = normalize
        if normalize:
            assert isinstance(self.observation_space, spaces.Box), \
                'observation normalization requires a Box observation space, got {}'.format(self.observation_space)
            self.clipob = clipob
            self.cliprew = cliprew
            self.gamma = gamma
            self.epsilon = epsilon
            self.ret = np.zeros(num_envs)
            self._init_normalization_state(use_tf=use_tf)

        # ----- episode results tracking setup -------------------------------------------
        self.record_episodes = record_episodes
        self.results_file = None
        if record_episodes:
            self.results_info_keywords = tuple(results_info_keywords)
            self.tstart = time.time()
            self.eprets = np.zeros(num_envs, 'f')
            self.eplens = np.zeros(num_envs, 'i')
            self.epcount = 0
            self.results_writer = None
            if results_filename is not None:
                self._init_results_writer(results_filename, extra_keys=self.results_info_keywords)

    # ----- absorbed running-statistics normalization ----------------------------------

    def _init_normalization_state(self, use_tf):
        """
        Prepare the running mean/std statistics of the observations and of the
        discounted returns. With use_tf the statistics live in tensorflow
        variables in the (ob_rms, ret_rms) scopes, so that they are saved
        and restored together with the model; otherwise they are numpy arrays.
        """
        if use_tf:
            import tensorflow as tf
            from baselines.common.tf_util import get_session

            self._tf_sess = get_session()
            self._ob_stats = self._init_tf_stat_variables('ob_rms', self.observation_space.shape)
            self._ret_stats = self._init_tf_stat_variables('ret_rms', ())
        else:
            self._ob_stats = {
                'mean': np.zeros(self.observation_space.shape, 'float64'),
                'var': np.ones(self.observation_space.shape, 'float64'),
                'count': 1e-4,
            }
            self._ret_stats = {
                'mean': np.full((), 0., 'float64'),
                'var': np.full((), 1., 'float64'),
                'count': 1e-4,
            }

    def _init_tf_stat_variables(self, scope, shape):
        """
        Allocate the tensorflow placeholders and variables holding one set of
        running statistics, together with the ops that push a new mean/var/count
        triple into them.
        """
        import tensorflow as tf

        stats = {}
        stats['new_mean'] = tf.placeholder(shape=shape, dtype=tf.float64)
        stats['new_var'] = tf.placeholder(shape=shape, dtype=tf.float64)
        stats['new_count'] = tf.placeholder(shape=(), dtype=tf.float64)
        with tf.variable_scope(scope, reuse=tf.AUTO_REUSE):
            # the variable holding the variance is named 'std' for backwards
            # compatibility with existing saved statistics
            stats['mean_var'] = tf.get_variable('mean', initializer=np.zeros(shape, 'float64'), dtype=tf.float64)
            stats['var_var'] = tf.get_variable('std', initializer=np.ones(shape, 'float64'), dtype=tf.float64)
            stats['count_var'] = tf.get_variable('count', initializer=np.full((), 1e-4, 'float64'), dtype=tf.float64)
        stats['update_ops'] = tf.group([
            stats['var_var'].assign(stats['new_var']),
            stats['mean_var'].assign(stats['new_mean']),
            stats['count_var'].assign(stats['new_count']),
        ])
        self._tf_sess.run(tf.variables_initializer([stats['mean_var'], stats['var_var'], stats['count_var']]))
        self._sync_tf_stats(stats)
        return stats

    def _sync_tf_stats(self, stats):
        stats['mean'], stats['var'], stats['count'] = \
            self._tf_sess.run([stats['mean_var'], stats['var_var'], stats['count_var']])

    def _update_running_stats(self, stats, x):
        """
        Update one set of running statistics with a new batch, using the
        parallel algorithm for combining mean/var/count moments:

        https://en.wikipedia.org/wiki/Algorithms_for_calculating_variance#Parallel_algorithm
        """
        if 'update_ops' in stats:
            # tensorflow variant: the update is pushed into the variables so that
            # the statistics are saved and restored with the model
            batch_mean = np.mean(x, axis=0)
            batch_var = np.var(x, axis=0)
            batch_count = x.shape[0]
            new_mean, new_var, new_count = self._combine_moments(
                stats['mean'], stats['var'], stats['count'], batch_mean, batch_var, batch_count)
            self._tf_sess.run(stats['update_ops'], feed_dict={
                stats['new_mean']: new_mean,
                stats['new_var']: new_var,
                stats['new_count']: new_count,
            })
            self._sync_tf_stats(stats)
        else:
            stats['mean'], stats['var'], stats['count'] = self._combine_moments(
                stats['mean'], stats['var'], stats['count'],
                np.mean(x, axis=0), np.var(x, axis=0), x.shape[0],
            )

    def _combine_moments(self, mean, var, count, batch_mean, batch_var, batch_count):
        delta = batch_mean - mean
        tot_count = count + batch_count

        new_mean = mean + delta * batch_count / tot_count
        m_a = var * count
        m_b = batch_var * batch_count
        M2 = m_a + m_b + np.square(delta) * count * batch_count / tot_count
        new_var = M2 / tot_count
        new_count = tot_count

        return new_mean, new_var, new_count

    def _filter_obs(self, obs):
        self._update_running_stats(self._ob_stats, obs)
        return np.clip((obs - self._ob_stats['mean']) /
                       np.sqrt(self._ob_stats['var'] + self.epsilon), -self.clipob, self.clipob)

    def _normalize_step_results(self, obs, rews, dones):
        """
        Keep the running statistics up to date and filter one step's
        observations and rewards through them.
        """
        self.ret = self.ret * self.gamma + rews
        obs = self._filter_obs(obs)
        self._update_running_stats(self._ret_stats, self.ret)
        rews = np.clip(rews / np.sqrt(self._ret_stats['var'] + self.epsilon), -self.cliprew, self.cliprew)
        self.ret[dones] = 0.
        return obs, rews

    # ----- absorbed observation stacking -----------------------------------------------

    def _stack_frames(self, obs, dones):
        """
        Roll the observation stack one step forward and write the newest
        observation into the last frame slot; environments that finished an
        episode start a fresh stack of zeros.
        """
        self.stackedobs = np.roll(self.stackedobs, shift=-1, axis=-1)
        for i, done_flag in enumerate(dones):
            if done_flag:
                self.stackedobs[i] = 0
        self.stackedobs[..., -obs.shape[-1]:] = obs
        return self.stackedobs

    def _stack_reset(self, obs):
        self.stackedobs[...] = 0
        self.stackedobs[..., -obs.shape[-1]:] = obs
        return self.stackedobs

    # ----- absorbed episode results tracking and results file ---------------------------

    def _init_results_writer(self, filename, extra_keys=()):
        """
        Prepare the results file, in the same format as the per-environment
        results files: a header line with the starting time, followed by
        one line of (r, l, t) and extra keys per finished episode.
        """
        assert filename is not None
        if not filename.endswith(self.RESULTS_EXT):
            if osp.isdir(filename):
                filename = osp.join(filename, self.RESULTS_EXT)
            else:
                filename = filename + "." + self.RESULTS_EXT
        self.results_file = open(filename, "wt")
        header = '# {} \n'.format(json.dumps({"t_start": self.tstart}))
        self.results_file.write(header)
        self.results_writer = csv.DictWriter(
            self.results_file, fieldnames=('r', 'l', 't') + tuple(extra_keys))
        self.results_writer.writeheader()
        self.results_file.flush()

    def _record_episodes(self, infos, rews, dones):
        """
        Accumulate the running return and length of each environment's episode,
        and summarize the episode when it is finished.
        """
        if not self.record_episodes:
            return infos
        self.eprets += rews
        self.eplens += 1
        finished = np.nonzero(dones)[0]
        for i in finished:
            epinfo = {
                'r': float(self.eprets[i]),
                'l': int(self.eplens[i]),
                't': round(time.time() - self.tstart, 6),
            }
            for k in self.results_info_keywords:
                epinfo[k] = infos[i][k]
            self.epcount += 1
            self.eprets[i] = 0
            self.eplens[i] = 0
            if self.results_writer is not None:
                self._write_results_row(epinfo)
        return infos

    def _write_results_row(self, epinfo):
        if self.results_writer:
            self.results_writer.writerow(epinfo)
            self.results_file.flush()

    def _close_results_writer(self):
        if self.results_file is not None:
            self.results_file.close()
            self.results_file = None

    # ----- vectorized stepping ---------------------------------------------------------

    def _reset_venvs(self):
        """
        Collect the observations returned by resetting each copy of the
        environment. Implemented by the concrete vectorized environment.
        """
        raise NotImplementedError

    def _step_wait_venvs(self):
        """
        Collect the results of the most recent step_async() from the
        environments. Implemented by the concrete vectorized environment.
        """
        raise NotImplementedError

    def reset(self):
        """
        Reset all the environments and return an array of
        observations, or a dict of observation arrays, with the configured
        observation stacking and normalization applied.

        If step_async is still doing work, that work will
        be cancelled and step_wait() should not be called
        until step_async() is invoked again.
        """
        obs = self._reset_venvs()
        if getattr(self, 'nstack', None) is not None:
            obs = self._stack_reset(obs)
        if self.normalize:
            self.ret = np.zeros(self.num_envs)
            obs = self._filter_obs(obs)
        if self.record_episodes:
            self.eprets = np.zeros(self.num_envs, 'f')
            self.eplens = np.zeros(self.num_envs, 'i')
        return obs

    @abstractmethod
    def step_async(self, actions):
        """
        Tell all the environments to start taking a step
        with the given actions.
        Call step_wait() to get the results of the step.

        You should not call this if a step_async run is
        already pending.
        """
        pass

    def step_wait(self):
        """
        Wait for the step taken with step_async(), and apply the configured
        observation stacking, normalization and episode tracking to the results.

        Returns (obs, rews, dones, infos):
         - obs: an array of observations, or a dict of
                arrays of observations
         - rews: an array of rewards
         - dones: an array of "episode done" booleans
         - infos: a sequence of info objects
        """
        obs, rews, dones, infos = self._step_wait_venvs()
        if getattr(self, 'nstack', None) is not None:
            obs = self._stack_frames(obs, dones)
        if self.normalize:
            obs, rews = self._normalize_step_results(obs, rews, dones)
        infos = self._record_episodes(infos, rews, dones)
        return obs, rews, dones, infos

    def close_extras(self):
        """
        Clean up the  extra resources, beyond what's in this base class.
        Only runs when not self.closed.
        """
        pass

    def close(self):
        if self.closed:
            return
        if self.viewer is not None:
            self.viewer.close()
        self._close_results_writer()
        self.close_extras()
        self.closed = True

    def step(self, actions):
        """
        Step the environments synchronously.

        This is available for backwards compatibility.
        """
        self.step_async(actions)
        return self.step_wait()

    def render(self, mode='human'):
        imgs = self.get_images()
        bigimg = tile_images(imgs)
        if mode == 'human':
            self.get_viewer().imshow(bigimg)
            return self.get_viewer().isopen
        elif mode == 'rgb_array':
            return bigimg
        else:
            raise NotImplementedError

    def get_images(self):
        """
        Return RGB images from each environment
        """
        raise NotImplementedError

    @property
    def unwrapped(self):
        if isinstance(self, VecEnvWrapper):
            return self.venv.unwrapped
        else:
            return self

    def get_viewer(self):
        if self.viewer is None:
            from gym.envs.classic_control import rendering
            self.viewer = rendering.SimpleImageViewer()
        return self.viewer

class VecEnvWrapper(VecEnv):
    """
    An environment wrapper that applies to an entire batch
    of environments at once.
    """

    def __init__(self, venv, observation_space=None, action_space=None):
        self.venv = venv
        super().__init__(num_envs=venv.num_envs,
                        observation_space=observation_space or venv.observation_space,
                        action_space=action_space or venv.action_space)

    def step_async(self, actions):
        self.venv.step_async(actions)

    @abstractmethod
    def reset(self):
        pass

    @abstractmethod
    def step_wait(self):
        pass

    def close(self):
        return self.venv.close()

    def render(self, mode='human'):
        return self.venv.render(mode=mode)

    def get_images(self):
        return self.venv.get_images()

    def __getattr__(self, name):
        if name.startswith('_'):
            raise AttributeError("attempted to get missing private attribute '{}'".format(name))
        return getattr(self.venv, name)

class VecEnvObservationWrapper(VecEnvWrapper):
    @abstractmethod
    def process(self, obs):
        pass

    def reset(self):
        obs = self.venv.reset()
        return self.process(obs)

    def step_wait(self):
        obs, rews, dones, infos = self.venv.step_wait()
        return self.process(obs), rews, dones, infos

class CloudpickleWrapper(object):
    """
    Uses cloudpickle to serialize contents (otherwise multiprocessing tries to use pickle)
    """

    def __init__(self, x):
        self.x = x

    def __getstate__(self):
        import cloudpickle
        return cloudpickle.dumps(self.x)

    def __setstate__(self, ob):
        import pickle
        self.x = pickle.loads(ob)


@contextlib.contextmanager
def clear_mpi_env_vars():
    """
    from mpi4py import MPI will call MPI_Init by default.  If the child process has MPI environment variables, MPI will think that the child process is an MPI process just like the parent and do bad things such as hang.
    This context manager is a hacky way to clear those environment variables temporarily such as when we are starting multiprocessing
    Processes.
    """
    removed_environment = {}
    for k, v in list(os.environ.items()):
        for prefix in ['OMPI_', 'PMI_']:
            if k.startswith(prefix):
                removed_environment[k] = v
                del os.environ[k]
    try:
        yield
    finally:
        os.environ.update(removed_environment)
