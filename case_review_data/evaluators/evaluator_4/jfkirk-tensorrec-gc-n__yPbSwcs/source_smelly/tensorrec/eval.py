import numpy as np

from .tensorrec import TensorRec


def precision_at_k(predicted_ranks, test_interactions, k=10, preserve_rows=False):
    """
    Wrapper for TensorRec.precision_at_k.
    :param predicted_ranks: Numpy matrix
    The results of model.predict_rank()
    :param test_interactions: scipy.sparse matrix
    Test interactions matrix of shape [n_users, n_items]
    :param k: int
    The rank at which to stop evaluating precision.
    :param preserve_rows: bool
    If True, a value of 0 will be returned for every user without test interactions.
    If False, only users with test interactions will be returned.
    :return: np.array
    """
    return TensorRec.precision_at_k(predicted_ranks=predicted_ranks,
                                    test_interactions=test_interactions,
                                    k=k,
                                    preserve_rows=preserve_rows)


def recall_at_k(predicted_ranks, test_interactions, k=10, preserve_rows=False):
    """
    Wrapper for TensorRec.recall_at_k.
    :param predicted_ranks: Numpy matrix
    The results of model.predict_rank()
    :param test_interactions: scipy.sparse matrix
    Test interactions matrix of shape [n_users, n_items]
    :param k: int
    The rank at which to stop evaluating recall.
    :param preserve_rows: bool
    If True, a value of 0 will be returned for every user without test interactions.
    If False, only users with test interactions will be returned.
    :return: np.array
    """
    return TensorRec.recall_at_k(predicted_ranks=predicted_ranks,
                                 test_interactions=test_interactions,
                                 k=k,
                                 preserve_rows=preserve_rows)


def _setup_ndcg(predicted_ranks, test_interactions, k=10):
    return TensorRec._setup_ndcg(predicted_ranks=predicted_ranks,
                                  test_interactions=test_interactions,
                                  k=k)


def _idcg(hits, k=10):
    return TensorRec._idcg(hits=hits, k=k)


def _dcg(relevance, k_mask, ror_at_k, ror):
    return TensorRec._dcg(relevance=relevance, k_mask=k_mask, ror_at_k=ror_at_k, ror=ror)


def ndcg_at_k(predicted_ranks, test_interactions, k=10, preserve_rows=False):
    """
    Wrapper for TensorRec.ndcg_at_k.
    :param predicted_ranks: Numpy matrix
    The results of model.predict_rank()
    :param test_interactions: scipy.sparse matrix
    Test interactions matrix of shape [n_users, n_items]
    :param k: int
    The rank at which to stop evaluating NDCG.
    :param preserve_rows: bool
    If True, a value of 0 will be returned for every user without test interactions.
    If False, only users with test interactions will be returned.
    :return: np.array
    """
    return TensorRec.ndcg_at_k(predicted_ranks=predicted_ranks,
                              test_interactions=test_interactions,
                              k=k,
                              preserve_rows=preserve_rows)


def f1_score_at_k(predicted_ranks, test_interactions, k=10, preserve_rows=False):
    """
    :param model: TensorRec
    A trained TensorRec model.
    :param test_interactions: scipy.sparse matrix
    Test interactions matrix of shape [n_users, n_items]
    :param user_features: scipy.sparse matrix
    User features matrix of shape [n_users, n_user_features]
    :param item_features: scipy.sparse matrix
    Item features matrix of shape [n_items, n_item_features]
    :param k: int
    The rank at which to stop evaluating recall.
    :param preserve_rows: bool
    If True, a value of 0 will be returned for every user without test interactions.
    If False, only users with test interactions will be returned.
    :return: np.array
    """
    return TensorRec.f1_score_at_k(predicted_ranks=predicted_ranks,
                                   test_interactions=test_interactions,
                                   k=k,
                                   preserve_rows=preserve_rows)


def fit_and_eval(model, user_features, item_features, train_interactions, test_interactions, fit_kwargs, recall_k=30,
                 precision_k=5, ndcg_k=30):
    return model.fit_and_evaluate(user_features=user_features,
                                  item_features=item_features,
                                  train_interactions=train_interactions,
                                  test_interactions=test_interactions,
                                  fit_kwargs=fit_kwargs,
                                  recall_k=recall_k,
                                  precision_k=precision_k,
                                  ndcg_k=ndcg_k)


def grid_check_model_on_dataset(train_interactions, test_interactions, user_features, item_features):

    results = []
    for n_components in [2, 4, 8, 16, 32, 64, 128, 256]:
        for epochs in [2, 4, 8, 16, 32, 64, 128, 256]:
            model = TensorRec(n_components=n_components)
            scores = model.fit_and_evaluate(user_features=user_features,
                                            item_features=item_features,
                                            train_interactions=train_interactions,
                                            test_interactions=test_interactions,
                                            fit_kwargs={'epochs': epochs})
            results.append((n_components, scores))
            print(n_components, epochs, scores)


def eval_random_ranks_on_dataset(interactions, recall_k=30, precision_k=5, ndcg_k=30):

    n_users, n_items = interactions.shape

    random_guesses = np.array([np.random.choice(a=n_items, size=n_items, replace=False) + 1 for _ in range(n_users)])

    p_at_k = TensorRec.precision_at_k(random_guesses, interactions, k=precision_k)
    r_at_k = TensorRec.recall_at_k(random_guesses, interactions, k=recall_k)
    n_at_k = TensorRec.ndcg_at_k(random_guesses, interactions, k=ndcg_k)

    return np.mean(r_at_k), np.mean(p_at_k), np.mean(n_at_k)
