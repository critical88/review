def get_session():
    """
    Returns the shared TensorFlow session, as owned and managed by the TensorRec model class.
    :return: tf.Session
    """
    from .tensorrec import TensorRec
    return TensorRec.get_session()


def set_session(session):
    """
    Attaches the TensorFlow session for the TensorRec model class to fit and predict against.
    :param session: tf.Session or None
    The session to attach, or None to request a fresh session at the next call of get_session().
    """
    from .tensorrec import TensorRec
    TensorRec.set_session(session)
