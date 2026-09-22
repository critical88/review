"""The optimizer tries to constant fold expressions and modify the AST
in place so that it should be faster to evaluate.

Because the AST does not contain all the scoping information and the
compiler has to find that out, we cannot do all the optimizations we
want. For example, loop unrolling doesn't work because unrolled loops
would have a different scope. The solution would be a second syntax tree
that stored the scoping rules.
"""

import typing as t

from . import nodes
from .visitor import NodeTransformer

if t.TYPE_CHECKING:
    from .environment import Environment


def optimize(node: nodes.Node, environment: "Environment") -> nodes.Node:
    """The context hint can be used to perform an static optimization
    based on the context given."""
    optimizer = Optimizer(environment)
    return t.cast(nodes.Node, optimizer.visit(node))


class BufferReturnStrategy:
    """Strategy for determining how buffer contents should be returned
    based on the current evaluation context and escaping rules."""

    CONCAT_ONLY = "concat"
    MARKUP_WRAP = "markup"
    VOLATILE_CHECK = "volatile"

    def __init__(self, environment: "t.Optional[Environment]") -> None:
        self.environment = environment
        self._resolved: t.Optional[str] = None

    def resolve(
        self, autoescape: bool, volatile: bool
    ) -> str:
        if self._resolved is not None:
            return self._resolved
        if volatile:
            return self.VOLATILE_CHECK
        if autoescape:
            return self.MARKUP_WRAP
        return self.CONCAT_ONLY

    def should_wrap_markup(self, autoescape: bool, volatile: bool) -> bool:
        return self.resolve(autoescape, volatile) in (
            self.MARKUP_WRAP,
            self.VOLATILE_CHECK,
        )


class Optimizer(NodeTransformer):
    def __init__(self, environment: "t.Optional[Environment]") -> None:
        self.environment = environment

    def generic_visit(
        self, node: nodes.Node, *args: t.Any, **kwargs: t.Any
    ) -> nodes.Node:
        node = super().generic_visit(node, *args, **kwargs)

        # Do constant folding. Some other nodes besides Expr have
        # as_const, but folding them causes errors later on.
        if isinstance(node, nodes.Expr):
            try:
                return nodes.Const.from_untrusted(
                    node.as_const(args[0] if args else None),
                    lineno=node.lineno,
                    environment=self.environment,
                )
            except nodes.Impossible:
                pass

        return node
