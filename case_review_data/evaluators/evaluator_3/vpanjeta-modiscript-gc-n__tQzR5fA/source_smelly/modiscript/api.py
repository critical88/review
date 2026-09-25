from .lexer import Lexer
from .parser import Parser


class ModiScript:
    def __init__(self, debug=False):
        self.debug = debug

    def _compile_file(self, value, value_type="filename"):
        source = Lexer(value, value_type, self.debug)
        lex_out = source.analyze()
        source.record_lexemes(lex_out)
        parse_out = Parser(lex_out).parse()
        source.record_parse_tree(parse_out)
        return compile(parse_out, "<ast>", "exec")

    def execute(self, value, value_type="filename"):
        ast_module = self._compile_file(value, value_type)
        exec(ast_module)
