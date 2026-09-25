#   Tests for parsing grammar definitions and grammar commands.
#   --------------------------------------
#
#   Licensed under the Apache License, Version 2.0 (the "License");
#   you may not use this file except in compliance with the License.
#   You may obtain a copy of the License at
#
#       http://www.apache.org/licenses/LICENSE-2.0

import os

import pytest

from grammar import Grammar, GrammarError
from grammar import RecursionError as GrammarRecursionError


def test_parse_simple_grammar_and_generate_root():
    grammar = Grammar()
    err = grammar.parse_from_string('<root root=true> = hello world')
    assert err == 0
    assert grammar.generate_root() == 'hello world'


def test_parse_from_string_returns_error_count(capsys):
    grammar = Grammar()
    err = grammar.parse_from_string('this is not a valid rule')
    assert err == 1
    assert 'Error parsing line' in capsys.readouterr().out


def test_create_tag_attributes():
    grammar = Grammar()
    tag = grammar._parse_tag_and_attributes('new mytype min=0 max=5 nonrecursive')
    assert tag['type'] == 'tag'
    assert tag['tagname'] == 'mytype'
    assert tag['new'] == 'true'
    assert tag['min'] == '0'
    assert tag['max'] == '5'
    assert tag['nonrecursive'] is True


def test_create_tag_plain():
    grammar = Grammar()
    tag = grammar._parse_tag_and_attributes('string')
    assert tag['tagname'] == 'string'
    assert 'new' not in tag


def test_create_tag_empty_raises():
    grammar = Grammar()
    with pytest.raises(GrammarError):
        grammar._parse_tag_and_attributes('')


def test_remove_comments():
    grammar = Grammar()
    assert grammar._remove_comments('foo # bar') == 'foo'
    assert grammar._remove_comments('  foo  ') == 'foo'
    assert grammar._remove_comments('# only comment') == ''


def test_fix_idents_removes_common_indentation():
    grammar = Grammar()
    source = '    foo()\n      bar()\n\n'
    fixed = grammar._fix_idents(source)
    assert fixed.splitlines()[0] == 'foo()'
    assert fixed.splitlines()[1] == '  bar()'


def test_fix_idents_expands_tabs():
    grammar = Grammar()
    fixed = grammar._fix_idents('\tfoo()')
    assert fixed.splitlines()[0] == 'foo()'


def test_varformat_command_changes_variable_names():
    grammar = Grammar()
    text = ('!varformat fvar%03d\n'
            '!begin lines\n'
            'v = <new anytype>;\n'
            '!end lines\n'
            '<anytype> = 1\n'
            '<root root=true> = <lines count=1>\n')
    assert grammar.parse_from_string(text) == 0
    result = grammar.generate_root()
    assert 'fvar001' in result
    assert '/* newvar{fvar001:anytype} */' in result


def default_lines_grammar(count=2):
    return ('!begin lines\n'
            'v = <new sometype>;\n'
            '!end lines\n'
            '<sometype> = 1\n'
            '<root root=true> = <lines count=%d>\n' % count)


def test_lineguard_wraps_generated_lines():
    grammar = Grammar()
    text = ('!lineguard try { <line> } catch(e) { }\n' + default_lines_grammar(2))
    assert grammar.parse_from_string(text) == 0
    result = grammar.generate_root()
    assert '/* newvar{' in result
    assert 'try { ' in result and ' } catch(e) { }' in result
    assert result.count('try { ') >= 2


def test_max_recursion_command_accepted():
    grammar = Grammar()
    assert grammar.parse_from_string('!max_recursion 10') == 0
    assert grammar._recursion_max == 10


def test_max_recursion_command_rejects_non_integer():
    grammar = Grammar()
    with pytest.raises(GrammarError):
        grammar.parse_from_string('!max_recursion ten')


def test_var_reuse_prob_command():
    grammar = Grammar()
    grammar.parse_from_string('!var_reuse_prob 0.5')
    assert grammar._var_reuse_prob == 0.5
    with pytest.raises(GrammarError):
        grammar.parse_from_string('!var_reuse_prob notanumber')


def test_unknown_command_reports_error(capsys):
    grammar = Grammar()
    err = grammar.parse_from_string('!nosuchcommand x')
    assert err == 1
    assert 'Unknown command' in capsys.readouterr().out


def test_nonrecursive_rules_break_recursion():
    grammar = Grammar()
    grammar.parse_from_string('!max_recursion 3')
    text = ('<frob =nonrecursive> = "leaf"\n'
            '<frob> = <frob><frob>\n'
            '<root root=true> = <frob>\n')
    assert grammar.parse_from_string(text) == 0
    result = grammar.generate_root()
    assert 'leaf' in result


def test_recursion_without_fallback_still_terminates_via_error():
    grammar = Grammar()
    grammar.parse_from_string('!max_recursion 2')
    text = ('<frob> = <frob><frob>\n'
            '<root root=true> = <frob>\n')
    assert grammar.parse_from_string(text) == 0
    with pytest.raises(GrammarRecursionError):
        grammar.generate_root()


def test_extend_command_registers_inheritance():
    grammar = Grammar()
    grammar.parse_from_string('!extends ChildType ParentType')
    context = {'variables': {}, 'interesting_lines': []}
    grammar._add_variable('v1', 'ChildType', context)
    assert 'v1' in context['variables']['ChildType']
    assert 'v1' in context['variables']['ParentType']


def test_variable_setters_walk_inheritance():
    grammar = Grammar()
    grammar.parse_from_string('!extends ChildType ParentType\n!extends ParentType BaseType')
    expected = ("SetVariable(fuzzervars, v1, 'ChildType'); "
                "SetVariable(fuzzervars, v1, 'ParentType'); "
                "SetVariable(fuzzervars, v1, 'BaseType'); ")
    assert grammar._get_variable_setters('v1', 'ChildType') == expected


def test_include_from_file_uses_relative_definitions_dir(tmp_path):
    extra_file = tmp_path / 'extra.txt'
    extra_file.write_text('<includedroot> = included\n')
    main_file = tmp_path / 'main.txt'
    main_file.write_text('!include extra.txt\n<root root=true> = <includedroot>\n')
    grammar = Grammar()
    # The include is resolved relative to the including file's directory.
    assert grammar.parse_from_file(str(main_file)) == 0
    assert grammar._creators['includedroot']
    assert grammar.generate_root() == 'included'


def test_include_from_file_reports_missing_file(capsys):
    grammar = Grammar()
    err = grammar._include_from_file('missing.txt')
    assert err == 1
    assert 'Error reading' in capsys.readouterr().out


def test_import_grammar_command(tmp_path):
    sub = tmp_path / 'sub.txt'
    sub.write_text('<greeting> = hello\n')
    main = tmp_path / 'main.txt'
    main.write_text('!import sub.txt\n<root root=true> = <import from=sub.txt symbol=greeting>\n')
    grammar = Grammar()
    assert grammar.parse_from_file(str(main)) == 0
    assert grammar.generate_root() == 'hello'


def test_import_tag_requires_from_attribute():
    grammar = Grammar()
    text = ('<root root=true> = <import>\n')
    grammar.parse_from_string(text)
    with pytest.raises(GrammarError):
        grammar.generate_root()


def test_import_tag_rejects_unknown_grammar():
    grammar = Grammar()
    text = ('<root root=true> = <import from=nothere>\n')
    grammar.parse_from_string(text)
    with pytest.raises(GrammarError):
        grammar.generate_root()


def test_add_import_enables_symbol_expansion():
    imported = Grammar()
    imported.parse_from_string('<greeting> = hi\n')
    grammar = Grammar()
    grammar.parse_from_string('<root root=true> = <import from=cssgrammar symbol=greeting>\n')
    grammar.add_import('cssgrammar', imported)
    assert grammar.generate_root() == 'hi'


def test_add_import_can_expand_root_of_imported_grammar():
    imported = Grammar()
    imported.parse_from_string('<root root=true> = rootvalue\n')
    grammar = Grammar()
    grammar.parse_from_string('<root root=true> = <import from=cssgrammar>\n')
    grammar.add_import('cssgrammar', imported)
    assert grammar.generate_root() == 'rootvalue'


def test_lines_tag_requires_count():
    grammar = Grammar()
    grammar.parse_from_string('<root root=true> = <lines>\n')
    with pytest.raises(GrammarError):
        grammar.generate_root()


def test_rules_without_creators_raise_on_generation():
    grammar = Grammar()
    grammar.parse_from_string('<root root=true> = <uncreated>\n')
    with pytest.raises(GrammarError):
        grammar.generate_root()


def test_probabilities_are_normalized_into_cdf():
    grammar = Grammar()
    text = ('<color p=0.2> = red\n'
            '<color p=0.6> = blue\n'
            '<color> = green\n')
    grammar.parse_from_string(text)
    cdf = grammar._creator_cdfs['color']
    assert len(cdf) == 3
    assert abs(cdf[-1] - 1.0) < 0.001


def test_uniform_rules_have_no_cdf():
    grammar = Grammar()
    grammar.parse_from_string('<color> = "red"\n<color> = "blue"\n')
    assert grammar._creator_cdfs['color'] == []


def test_user_defined_functions_in_user_code(tmp_path):
    grammar = Grammar()
    text = ('!begin function myfunc\n'
            'ret_val = attributes["value"]\n'
            '!end function\n'
            '<root root=true> = <call function=myfunc value=called>\n')
    assert grammar.parse_from_string(text) == 0
    assert grammar.generate_root() == 'called'


def test_call_tag_requires_function_attribute():
    grammar = Grammar()
    grammar.parse_from_string('<root root=true> = <call>\n')
    with pytest.raises(GrammarError):
        grammar.generate_root()


def test_unknown_user_function_raises():
    grammar = Grammar()
    grammar.parse_from_string('<root root=true> = <call function=missing value="x">\n')
    with pytest.raises(GrammarError):
        grammar.generate_root()


def test_user_function_error_is_wrapped():
    grammar = Grammar()
    text = ('!begin function badfunc\n'
            'raise ValueError("boom")\n'
            '!end function\n'
            '<root root=true> = <call function=badfunc>\n')
    grammar.parse_from_string(text)
    with pytest.raises(GrammarError):
        grammar.generate_root()


def test_user_function_syntax_error_is_reported():
    grammar = Grammar()
    text = ('!begin function synfunc\n'
            'def def def\n'
            '!end function\n')
    with pytest.raises(GrammarError):
        grammar.parse_from_string(text)


def test_parse_from_file_extra_prefix(tmp_path):
    grammar_file = tmp_path / 'g.txt'
    grammar_file.write_text('<root root=true> = <greeting>\n')
    grammar = Grammar()
    extra = '<greeting> = from-extra\n'
    assert grammar.parse_from_file(str(grammar_file), extra) == 0
    assert grammar.generate_root() == 'from-extra'


def test_parse_from_file_missing_returns_error(capsys):
    grammar = Grammar()
    code = grammar.parse_from_file('/nonexistent/grammar.txt')
    assert code == 1
    assert 'Error reading' in capsys.readouterr().out


def test_beforeoutput_attribute_uses_functions():
    grammar = Grammar()
    text = ('!begin function double\n'
            'ret_val = str(ret_val) + str(ret_val)\n'
            '!end function\n'
            '<root root=true> = <greeting beforeoutput=double>\n'
            '<greeting> = ab\n')
    assert grammar.parse_from_string(text) == 0
    assert grammar.generate_root() == 'abab'


def test_named_ids_reuse_expansions():
    grammar = Grammar()
    text = ('<color> = "red"\n'
            '<color> = "blue"\n'
            '<root root=true> = <color id=c>|<color id=c>\n')
    assert grammar.parse_from_string(text) == 0
    result = grammar.generate_root()
    left, right = result.split('|')
    assert left == right


def test_special_definition_root_without_root_attribute():
    grammar = Grammar()
    grammar.parse_from_string('<plain> = "x"\n')
    assert grammar._root == ''
    assert grammar.generate_root() == ''
