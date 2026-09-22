import typing as t

from .filters import FILTERS as DEFAULT_FILTERS  # noqa: F401
from .tests import TESTS as DEFAULT_TESTS  # noqa: F401
from .utils import Cycler
from .utils import generate_lorem_ipsum
from .utils import Joiner
from .utils import Namespace

if t.TYPE_CHECKING:
    import typing_extensions as te

# defaults for the parser / lexer
BLOCK_START_STRING = "{%"
BLOCK_END_STRING = "%}"
VARIABLE_START_STRING = "{{"
VARIABLE_END_STRING = "}}"
COMMENT_START_STRING = "{#"
COMMENT_END_STRING = "#}"
LINE_STATEMENT_PREFIX: t.Optional[str] = None
LINE_COMMENT_PREFIX: t.Optional[str] = None
TRIM_BLOCKS = False
LSTRIP_BLOCKS = False
NEWLINE_SEQUENCE: "te.Literal['\\n', '\\r\\n', '\\r']" = "\n"
KEEP_TRAILING_NEWLINE = False

# default filters, tests and namespace

DEFAULT_NAMESPACE = {
    "range": range,
    "dict": dict,
    "lipsum": generate_lorem_ipsum,
    "cycler": Cycler,
    "joiner": Joiner,
    "namespace": Namespace,
}

# default policies
DEFAULT_POLICIES: t.Dict[str, t.Any] = {
    "compiler.ascii_str": True,
    "urlize.rel": "noopener",
    "urlize.target": None,
    "urlize.extra_schemes": None,
    "truncate.leeway": 5,
    "json.dumps_function": None,
    "json.dumps_kwargs": {"sort_keys": True},
    "ext.i18n.trimmed": False,
}


_policy_resolvers: t.Dict[str, t.Callable[[t.Dict[str, t.Any], str], t.Any]] = {}


def register_policy_resolver(
    prefix: str, resolver: t.Callable[[t.Dict[str, t.Any], str], t.Any]
) -> None:
    """Register a policy resolver for a given prefix. This provides
    a centralized extension point for policy lookups without needing
    to modify the policies dict directly.
    """
    _policy_resolvers[prefix] = resolver


def resolve_policy(policies: t.Dict[str, t.Any], key: str) -> t.Any:
    """Look up a policy value, falling back to registered resolvers
    if the key is not found directly in the policies dict.
    """
    if key in policies:
        return policies[key]
    prefix = key.split(".")[0] if "." in key else key
    resolver = _policy_resolvers.get(prefix)
    if resolver is not None:
        return resolver(policies, key)
    return None
