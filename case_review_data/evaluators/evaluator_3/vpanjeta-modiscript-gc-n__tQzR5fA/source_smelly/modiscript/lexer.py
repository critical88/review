from .utils import LEX, ErrorHandler, ERROR, CONGRESS_RULE, STARTING_TROUBLE, MISQUOTE, WORDS
import re
import sys

mitrooon = re.compile(r'^mith?roo?o?n?$')
acche = re.compile(r'^ac[ch]?hee?$')
barabar = re.compile(r'^bara+bar$')
sach = re.compile(r'^sac[ch]?h$')
jhoot = re.compile(r'^jh?(oo|u)t$')


class Lexer:
    def __init__(self, value, value_type="filename", debug=False):
        self.stack = []
        self.clear = False
        self.contents = []
        self.debug = debug
        self._dump_ready = value_type == "filename"
        self._trace_stem = value.split('.', 1)[0] if value_type == "filename" else ''
        self._word_fixes = {}
        self._collect_word_fixes()
        self._program_protocol = ('mitrooon', 'acche din aa gaye'.split())
        self._source_path = None
        self._source_lines = []
        if value_type == "filename":
            self._read_source(value)
        else:
            self._inline_source(value)
        self.contents = list(self._source_lines)

    @staticmethod
    def lexeme(lex, value=None, line=0, offset=0):
        return locals()

    @staticmethod
    def normalize(word):
        if word in WORDS:
            word = WORDS[word]
        elif mitrooon.search(word):
            word = 'mitrooon'
        elif acche.search(word):
            word = 'acche'
        elif barabar.search(word):
            word = 'barabar'
        elif sach.search(word):
            word = 'sach'
        elif jhoot.search(word):
            word = 'jhoot'
        return word

    def _collect_word_fixes(self):
        # Keep a per-program copy of the accepted spellings so a driver can
        # teach the front-end new word corrections without touching globals.
        self._word_fixes = dict(WORDS)

    def _correction_for(self, word):
        if word in self._word_fixes:
            return self._word_fixes[word]
        return Lexer.normalize(word)

    def _expect_opening(self, lexeme):
        if not Lexer._is_var(lexeme, self._program_protocol[0]):
            raise ErrorHandler(STARTING_TROUBLE)

    def _expect_closing(self, lexemes):
        ending = list(self._program_protocol[1])
        try:
            while ending:
                if not Lexer._is_var(lexemes.pop(), ending.pop()):
                    raise ErrorHandler(CONGRESS_RULE)
        except IndexError:
            raise ErrorHandler(CONGRESS_RULE)

    def _read_source(self, path):
        self._source_path = path
        self._source_lines = []
        with open(path) as handle:
            for line in handle.readlines():
                self._source_lines.append(line.lower())

    def _inline_source(self, value):
        self._source_lines = value.lower().split("\n")

    def record_lexemes(self, lex_out):
        # The report files are written next to the analyzed source, so the
        # trace stem is captured once when the front-end is set up.
        if not (self.debug and self._dump_ready):
            return
        with open(self._trace_stem + '.txt', 'w') as f:
            print(*lex_out, sep='\n', file=f)

    def record_parse_tree(self, parse_out):
        if not (self.debug and self._dump_ready):
            return
        import ast
        with open(self._trace_stem + '.py', 'w') as f:
            print(ast.dump(parse_out), file=f)

    def _push(self, *lex):
        self.stack.append(lex)
        self.clear = True

    def on_top(self, *lex):
        i = len(self.stack) - 1
        for l in lex[::-1]:
            if i < 0 or self.stack[i][:2] != l:
                return False
            i -= 1
        return True

    def pop(self):
        if self.stack:
            return self.stack.pop()
        raise ErrorHandler(ERROR, 'Empty pop')

    @staticmethod
    def _is_var(lex, value):
        return lex['lex'] == LEX['var'] and lex['value'] == value

    def analyze(self):
        lexer = self._analyze_lexemes()
        lex = next(lexer)
        self._expect_opening(lex)
        lexemes = list(lexer)
        self._expect_closing(lexemes)
        return lexemes

    def _analyze_lexemes(self):
        """
        Identify lexemes and return tokens.
        """
        num = 0
        self.stack = []
        self.clear = False
        for line in self.contents:
            num += 1
            offset = 0
            length = len(line)
            while offset < length:
                token = line[offset]
                if self.clear:
                    for lex in self.stack:
                        yield Lexer.lexeme(*lex)
                    self.stack = []
                    self.clear = False
                elif token.isspace():
                    offset += 1
                elif line[offset: offset + 2] in ('==', '&&', '||', '<=', '>=', '!='):
                    self._push(LEX[line[offset: offset + 2]], None, num, offset)
                    offset += 2
                elif token in '+-*/%(){}=<>!':
                    self._push(LEX[token], None, num, offset)
                    offset += 1
                elif token.isdigit():
                    n = ''
                    o = offset
                    while o < length and line[o].isdigit():
                        n += line[o]
                        o += 1
                    self._push(LEX['num'], int(n), num, offset)
                    offset = o
                elif token.isalpha():
                    w = ''
                    o = offset
                    while o < length and line[o].isalpha():
                        w += line[o]
                        o += 1
                    w = self._correction_for(w)
                    if w == 'agar':
                        self._push(LEX['if'], None, num, offset)
                    elif w == 'toh' and self.on_top((LEX['var'], 'nahi')):
                        _, _, lex_line, lex_offset = self.pop()
                        self._push(LEX['else'], None, lex_line, lex_offset)
                    elif w == 'toh':
                        self._push(LEX['then'], None, num, offset)
                    elif w == 'tak' and self.on_top((LEX['var'], 'jab')):
                        _, _, lex_line, lex_offset = self.pop()
                        self._push(LEX['until'], None, lex_line, lex_offset)
                    elif w == 'behno' and self.on_top((LEX['var'], 'bhaiyo'), (LEX['&&'], None)):
                        self.pop()
                        _, _, lex_line, lex_offset = self.pop()
                        self._push(LEX['print'], None, lex_line, lex_offset)
                    elif w == 'baat' and self.on_top((LEX['var'], 'mann'), (LEX['var'], 'ki')):
                        self.pop()
                        _, _, lex_line, lex_offset = self.pop()
                        self._push(LEX['input'], None, lex_line, lex_offset)
                    elif w == 'plus':
                        self._push(LEX['+'], None, num, offset)
                    elif w == 'substract':
                        self._push(LEX['-'], None, num, offset)
                    elif w == 'taimes':
                        self._push(LEX['*'], None, num, offset)
                    elif w == 'break':
                        self._push(LEX['/'], None, num, offset)
                    elif w == 'modi':
                        self._push(LEX['%'], None, num, offset)
                    elif w == 'kam':
                        self._push(LEX['<'], 'word', num, offset)
                    elif w == 'zyada':
                        self._push(LEX['>'], 'word', num, offset)
                    elif w == 'barabar':
                        self._push(LEX['=='], 'word', num, offset)
                    elif w == 'aur':
                        self._push(LEX['&&'], None, num, offset)
                        self.clear = False
                    elif w == 'ya':
                        self._push(LEX['||'], None, num, offset)
                    elif w == 'hai':
                        self._push(LEX['hai'], None, num, offset)
                    elif w == 'se':
                        pass
                    elif w == 'sach':
                        self._push(LEX['true'], None, num, offset)
                    elif w == 'jhoot':
                        self._push(LEX['false'], None, num, offset)
                    else:
                        self._push(LEX['var'], w, num, offset)
                        self.clear = False
                    offset = o
                elif token == '"' or token == "'":
                    w = ''
                    o = offset + 1
                    while o < length and line[o] != token:
                        if line[o] == '\\':
                            o += 1
                        w += line[o]
                        o += 1
                    if o == length:
                        raise ErrorHandler(MISQUOTE, line)
                    self._push(LEX['str'], w, num, offset)
                    offset = o + 1
                else:
                    self._push(LEX['sym'], num, offset)
                    offset += 1
        for lex in self.stack:
            yield Lexer.lexeme(*lex)
        self.stack = []
        self.clear = False
        if sys.version_info >= (3, 7):
            return
        raise StopIteration()
