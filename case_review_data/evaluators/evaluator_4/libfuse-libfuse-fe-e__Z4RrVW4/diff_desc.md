# Injection design record — libfuse session lifecycle responsibility leak

Pinned tree: `github.com/libfuse/libfuse` at commit
`f5b5797af692f99f9baab2f19233222a39598774` (master side-tree of the 3.19
series). The change described here was authored as a flat diff against that
tree; it is modeled as the residue of an upstream maintenance series that is
real, but unfinished in this alternate line of development.

## Maintenance motivation

Across the 3.17 → 3.19 series libfuse's *session* — `struct fuse_session`,
defined in the library's private header and realized in the low-level session
implementation — absorbed most of the lifecycle work that used to live in
callers and helpers:

* mount moved into the session (`fuse_session_mount*`), together with
  synchronous `FUSE_INIT` handling and the `is_sync_init` bookkeeping;
* the auto-unmount plumbing gained a socket adopted into the session
  (`auto_unmount_fd`) so teardown can unmount by closing it;
* io_uring request handling attached its pool to the session
  (`se->uring.pool`), with teardown called from the loops;
* the exit path was hardened for threads: the plain `exited` field became
  `mt_exited` with atomic access, paired with the `mt_finish` wakeup
  semaphore and the `mt_lock` protecting the worker list, and
  signal-reachable code carries `__attribute__((no_sanitize_thread))`.

That growth kept the *state* in the session record but kept spreading the
*decisions* about that state into the modules around it. A plausible
maintainer under release pressure performs a refactor that "simplifies the
loops and the signal path" by having each consumer drive the session record
directly: reading the exit flag where it is polled, posting the wakeup where
delivery is awaited, finishing the mount channel where the mount was
negotiated. The diff models exactly that line of development — the coherent
step a maintainer takes when the session record's transitions have not yet
been given a home on the session side.

## The evolution being modeled

Concretely, the diff is the shape of an intra-release follow-up to the
series above, in which four consumer seams stop asking the session module
for lifecycle transitions and start performing them by hand:

1. **Teardown signals.** The signal path grows a transition helper next to
   `exit_handler` that takes the session, reproduces the debug line, stores
   the exit flag, posts the wakeup, handles the assertion policy for
   negative signal values and records the signal as the session error. The
   handler keeps only the registration guard and forwards. The natural
   authorship excuse: to keep the whole "exit + wake + report" transition in
   one place under signal-handler restrictions, pushing the transition itself
   into the signal module's own file-local helper. The
   `no_sanitize_thread` attribute travels with it.
2. **Single-thread loop.** The loop head stops calling the accessor that
   answers "is the session exited" and instead polls the record's flag with
   atomic loads. The tail already derived the result from the recorded error
   and the io_uring pool that hangs off the session; the whole loop head/tail
   then runs on raw record access. Natural excuse: the flag is consulted
   every iteration, so why pay for a function call, and the session was
   already consulted for the recorded error.
3. **Multi-thread loop.** Worker shutdown grows a helper just above the 3.12
   loop function (next to the versioned entry points), which packs three
   session-side steps: park on the semaphore until the session says exited,
   emit the debug line from the session's own debug flag, and take the
   worker-list lock so the caller can cancel registered workers
   immediately. The helper documents that it returns with the session lock
   held. Natural excuse: "wait, log, lock" is one idiom in the shutdown
   conversation, so give it one name next to its only caller, in the loop
   module's own file. The helper is deliberately placed next to the
   `fuse_session_loop_mt_312` versioned entry point, which is the file's
   natural "extended" region.
4. **Mount channel.** The mount mechanics file grows a paired API for the
   session: a publisher that writes the adopted channel fd and the
   mountpoint into the record after a successful mount, and a channel
   "unwind" function that decides the session's auto-unmount socket
   ownership (adopt it, or close it and reap the fusermount child), spawns
   the auto-unmount helper on the direct-mount path, and on failure closes
   the channel and invalidates the session's channel state — fd, recorded
   error, sync-init flag. The session implementation, where two exit labels
   previously did exactly this work in sequence, is reduced to two call
   sites (`err:` folded; the wait-for-sync-init below it kept), and
   `goto err_with_sock` collapses into `goto err` since the label no longer
   adds anything. Natural excuse: the mount module spawned the helper and
   owns the mount-related fd mechanics, so let the mount module "finish" the
   channel and publish the result to the session.

The prototypes for the mount-side pair land in the shared internal mount
header (with doxygen comments in house style, and a session forward
declaration plus `<sys/types.h>` for `pid_t`), because the session
implementation calls both from its mount paths and hence needs the
declarations.

## Why these seams and shapes

**Why the signal path.** Teardown signals already owned the ugliest exit
constraint (masked, cross-thread visible), and the handler already touched
the record for the debug line and the error write. The transition helper is a
truthful reproduction of what the session-side exit call plus the handler's
own error write do — with the signal-module backtrace policy left inside.

**Why the single-thread loop.** This location is deliberately *not* a new
helper: the loop's exit consultation is at the head of the loop, and the
result derivation at its tail, which is the shape upstream itself has had
since io_uring landed. The design models an in-place change of exactly the
two lines where the loop consults the session — the smallest edit that makes
the loop head independent of the accessor and the tail a direct negotiation
with the record.

**Why the multi-thread loop.** The worker shutdown conversation
("park — log — lock") is the one place in the tree where a consumer both
waits *on* the session and then locks a session field. Packing it next to the
only caller models the common "extract a helper just to have one name"
refactor. Of the three steps, the wait and the log are session business; the
cancel walk over the local worker list is genuine loop business, which is
why the helper returns with the lock held instead of doing the cancels.

**Why the mount module.** Mount is the only module outside the session
implementation that already owns pieces referenced by the unwind semantics
(the auto-unmount helper spawn and the mountfd unmount), so it is the module
a maintainer would plausibly extend. The move pares the session
implementation's mount function down to its own business: create the mount,
order the labels, publish or unwind. The label merge is honest engineering:
`err_with_sock` existed only to distinguish "we hold a socket fd", and in the
injected line of development the fd argument plus a `-1` convention replace
the distinction, leaving nothing for the second label to do.

## Production role of the diff

Every touched behavior is core mount/serve-loop machinery exercised by the
test suite's mount, loop, teardown and cuse test groups. Sessions are
created and torn down in all example binaries; the loops run every request;
the mount channel path is exercised by every test that mounts (all of them)
and diagnosed with the invalid mount tests kept in the tree; the auto-unmount
adoption path is exercised by the tests that combine `auto_unmount` with
`sync_init`. The `fuse_kern_umount_mountfd` reuse on the helper-spawn
failure path and the child-reaping on the socket-close path are behaviors the
maintainer had already fought for (the comments record why the socket must
survive: the helper unmounts when its socket closes, and a premature close
would unmount early and hang a wait on the child).

## What was left alone, on purpose

* `cuse_lowlevel.c`: its init walk writes seven session fields, but it is
  construction wiring over a session the caller is about to own — the
  write-heavy moment of creation is the legitimate way a new session is
  wired, and the rest of its body is about the cuse reply protocol, not the
  session record. This file is the natural "looks similar, is not the same
  relation" neighbor for the seams above.
* `exit_backtrace` in the signal module: kept on the session's exit
  interface, deliberately, as the shape the teardown path did *not* take in
  this alternate line.
* `fuse_clone_chan` and `fuse_do_work` in the mt loop: channel cloning reads
  two session fields, and the worker thread mutates worker-list state that
  hangs off the session; the local thread bookkeeping dominates those bodies,
  so they were not attractive seams.
* The versioned compatibility symbols (`fuse_session_loop_mt_31/32`,
  `FUSE_SYMVER` plumbing) were left untouched: their bodies are thin
  forwarders with no independent consultation of the record.
* No header under `include/` changes, and the symbol version script is not
  touched — the injected line routes everything through internal
  declarations, which is straightforward for a maintainer shaping an
  in-tree refactor series.
