#   Tests for programming-language (code-lines) generation bookkeeping.
#   --------------------------------------
#
#   Licensed under the Apache License, Version 2.0 (the "License");
#   you may not use this file except in compliance with the License.
#   You may obtain a copy of the License at
#
#       http://www.apache.org/licenses/LICENSE-2.0

import random
import re

import pytest

from grammar import Grammar, GrammarError


def code_grammar(count=4):
    return ('!begin lines\n'
            'v = <new anytype>;\n'
            '!end lines\n'
            '<anytype> = 42\n'
            '<root root=true> = <lines count=%d>\n' % count)


def test_generated_code_lines_create_getvariable_fallbacks():
    grammar = Grammar()
    assert grammar.parse_from_string(code_grammar()) == 0
    result = grammar.generate_root()
    assert '/* newvar{var00001:anytype} */' in result
    assert "GetVariable(fuzzervars, 'anytype')" in result


def test_generated_code_lines_number_variables_sequentially():
    grammar = Grammar()
    assert grammar.parse_from_string(code_grammar()) == 0
    result = grammar.generate_root()
    names = re.findall(r'var (var\d+)', result)
    assert names
    assert names == sorted(set(names), key=lambda n: int(n[3:]))
    assert len(set(names)) == len(names)


def test_generated_code_is_deterministic_for_a_seed():
    grammar = Grammar()
    assert grammar.parse_from_string(code_grammar()) == 0
    random.seed(1234)
    result1 = grammar.generate_root()
    random.seed(1234)
    result2 = grammar.generate_root()
    assert result1 == result2


def test_helper_lines_do_not_become_regular_lines():
    grammar = Grammar()
    text = ('!begin helperlines\n'
            'helper();\n'
            '!end helperlines\n'
            '!begin lines\n'
            'main();\n'
            '!end lines\n'
            '<root root=true> = <lines count=2>\n')
    assert grammar.parse_from_string(text) == 0
    result = grammar.generate_root()
    assert 'main(' in result
    assert 'helper(' not in result


def test_variable_reuse_is_forced_over_type_limit():
    grammar = Grammar()
    grammar.parse_from_string('<limited> = 1\n')
    context = {'lastvar': 0, 'lines': [], 'variables': {},
                'interesting_lines': [], 'force_var_reuse': False}
    for i in range(6):
        grammar._add_variable('existing%d' % i, 'limited', context)
    random.seed(0)
    value = grammar._generate('limited', context, 0)
    assert value in ('existing%d' % i for i in range(6))


def test_variable_reuse_probability_one_always_reuses():
    grammar = Grammar()
    grammar.parse_from_string('!var_reuse_prob 1.0\n<anytype> = 1\n')
    context = {'lastvar': 0, 'lines': [], 'variables': {},
               'interesting_lines': [], 'force_var_reuse': False}
    grammar._add_variable('var00001', 'anytype', context)
    assert grammar._generate('anytype', context, 0) == 'var00001'


def test_any_tag_draws_from_generated_variables():
    grammar = Grammar()
    text = ('!begin lines\n'
            '<new anytype> = 1; consume(<any>);\n'
            '!end lines\n'
            '<anytype> = 1\n'
            '<root root=true> = <lines count=3>\n')
    assert grammar.parse_from_string(text) == 0
    result = grammar.generate_root()
    assert 'consume(' in result


def test_generate_symbol_expands_named_symbol():
    grammar = Grammar()
    grammar.parse_from_string('<greeting> = hello\n')
    assert grammar.generate_symbol('greeting') == 'hello'


def test_generate_symbol_unknown_raises():
    grammar = Grammar()
    with pytest.raises(GrammarError):
        grammar.generate_symbol('unknownsymbol')


def test_line_rules_can_use_constants():
    grammar = Grammar()
    text = ('!begin lines\n'
            'emit(<lt><gt><hash><space><cr><lf>);\n'
            '!end lines\n'
            '<root root=true> = <lines count=1>\n')
    assert grammar.parse_from_string(text) == 0
    result = grammar.generate_root()
    assert '<># \r\n' in result


def test_built_in_types_available_in_code_lines():
    grammar = Grammar()
    text = ('!begin lines\n'
            'i = <int min=5 max=5>; f = <float min=0.5 max=0.5>;'
            ' h = <hex>; s = <string min=97 max=97 minlength=2 maxlength=2>;\n'
            '!end lines\n'
            '<root root=true> = <lines count=1>\n')
    assert grammar.parse_from_string(text) == 0
    result = grammar.generate_root()
    assert 'i = 5;' in result
    assert 'f = 0.5;' in result
    assert 's = aa;' in result
    assert re.search(r'h = [0-9a-f];', result)


def test_noninteresting_types_do_not_register_variables():
    grammar = Grammar()
    text = ('!begin lines\n'
            's = <new boolean>;\n'
            '!end lines\n'
            '<root root=true> = <lines count=3>\n')
    assert grammar.parse_from_string(text) == 0
    result = grammar.generate_root()
    # Strings are non-interesting: no GetVariable bookkeeping is generated.
    assert 'GetVariable' not in result
    assert '/* newvar{' in result


def test_no_root_element_prints_error_and_returns_empty(capsys):
    grammar = Grammar()
    assert grammar.generate_root() == ''
    assert 'No root element defined' in capsys.readouterr().out


def test_compute_interesting_indices_for_line_rules():
    grammar = Grammar()
    text = ('!begin lines\n'
            '<new anytype> = 1;\n'
            'use(<anytype>);\n'
            '!end lines\n'
            '<anytype> = 1\n'
            '<root root=true> = <lines count=2>\n')
    assert grammar.parse_from_string(text) == 0
    assert grammar._interesting_lines['anytype']
    assert len(grammar._all_nonhelper_lines) == 2
