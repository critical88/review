#   End-to-end tests for the main DOM generator pipeline.
#   --------------------------------------
#
#   Licensed under the Apache License, Version 2.0 (the "License");
#   you may not use this file except in compliance with the License.
#   You may obtain a copy of the License at
#
#       http://www.apache.org/licenses/LICENSE-2.0

import os
import random

import pytest

import generator
from grammar import Grammar
from tests.helpers import REPO_ROOT


def empty_html_context():
    return {
        'htmlvars': [],
        'htmlvarctr': 0,
        'svgvarctr': 0,
        'mathmlvarctr': 0,
        'htmlvargen': ''
    }


def mini_css_grammar():
    grammar = Grammar()
    grammar.parse_from_string('<rules root=true> = <rule>\n<rule> = color:red;')
    return grammar


def mini_html_grammar():
    grammar = Grammar()
    text = ('<bodyelements> = <simpleelement>\n'
            '<simpleelement> = <htmlvarshape>\n'
            '<htmlvarshape> = <lt>div <gt>hello<lt>/div<gt>\n')
    grammar.parse_from_string(text)
    return grammar


def mini_js_grammar():
    grammar = Grammar()
    text = ('!begin lines\n'
            'doWork(<jsobj>);\n'
            '!end lines\n'
            '<jsobj> = 7\n'
            '<root root=true> = <lines count=3>\n')
    grammar.parse_from_string(text)
    return grammar


@pytest.fixture
def real_grammars():
    """Parses the shipped rule files once per test module run."""
    htmlgrammar = Grammar()
    assert htmlgrammar.parse_from_file(os.path.join(REPO_ROOT, 'rules', 'html.txt')) == 0
    cssgrammar = Grammar()
    assert cssgrammar.parse_from_file(os.path.join(REPO_ROOT, 'rules', 'css.txt')) == 0
    jsgrammar = Grammar()
    assert jsgrammar.parse_from_file(os.path.join(REPO_ROOT, 'rules', 'js.txt')) == 0
    htmlgrammar.add_import('cssgrammar', cssgrammar)
    jsgrammar.add_import('cssgrammar', cssgrammar)
    return htmlgrammar, cssgrammar, jsgrammar


@pytest.fixture
def real_template():
    with open(os.path.join(REPO_ROOT, 'template.html')) as f:
        return f.read()


def test_generate_html_elements_creates_context_entries():
    random.seed(10)
    ctx = empty_html_context()
    generator.generate_html_elements(ctx, 3)
    assert ctx['htmlvarctr'] == 3
    assert len(ctx['htmlvars']) == 3
    assert ctx['htmlvars'][0]['name'] == 'htmlvar00001'
    assert ctx['htmlvars'][2]['name'] == 'htmlvar00003'


def test_generate_html_elements_uses_known_tag_types():
    random.seed(10)
    ctx = empty_html_context()
    generator.generate_html_elements(ctx, 20)
    for var in ctx['htmlvars']:
        assert var['type'].startswith('HTML')
    assert 'document.createElement' in ctx['htmlvargen']
    assert '/* newvar{htmlvar00001:' in ctx['htmlvargen']


def test_generate_html_elements_emits_line_per_variable():
    random.seed(10)
    ctx = empty_html_context()
    generator.generate_html_elements(ctx, 5)
    assert ctx['htmlvargen'].count('\n') == 5


def make_context_with_counters():
    return empty_html_context()


def test_add_html_ids_for_html_tag():
    random.seed(0)
    ctx = make_context_with_counters()
    import re as _re
    out = _re.sub(r'<[a-zA-Z0-9_-]+ ',
                  lambda m: generator.add_html_ids(m, ctx),
                  '<div class="x"> ')
    tag_pos = out.find('<div ')
    assert out[tag_pos + 5:].startswith('id="htmlvar00001"')
    assert ctx['htmlvarctr'] == 1
    assert ctx['htmlvars'][0]['type'] == 'HTMLDivElement'
    assert 'document.getElementById' in ctx['htmlvargen']


def test_add_html_ids_for_svg_tag():
    import re as _re
    ctx = make_context_with_counters()
    out = _re.sub(r'<[a-zA-Z0-9_-]+ ',
                  lambda m: generator.add_html_ids(m, ctx),
                  '<circle r="1"> ')
    assert 'id="svgvar00001"' in out
    assert ctx['svgvarctr'] == 1
    assert ctx['htmlvars'][0]['type'] == 'SVGCircleElement'


def test_add_html_ids_for_mathml_tag():
    import re as _re
    ctx = make_context_with_counters()
    out = _re.sub(r'<[a-zA-Z0-9_-]+ ',
                  lambda m: generator.add_html_ids(m, ctx),
                  '<ms > ')
    assert 'id="mathmlvar00001"' in out
    assert ctx['mathmlvarctr'] == 1
    assert ctx['htmlvars'][0]['type'] == 'MathMLElement'


def test_add_html_ids_ignores_unknown_tags():
    import re as _re
    ctx = make_context_with_counters()
    out = _re.sub(r'<[a-zA-Z0-9_-]+ ',
                  lambda m: generator.add_html_ids(m, ctx),
                  '<nonsense>')
    assert ctx['htmlvarctr'] == 0
    assert ctx['svgvarctr'] == 0
    assert ctx['mathmlvarctr'] == 0
    assert ctx['htmlvars'] == []


def test_add_html_ids_counters_advance_independently():
    import re as _re
    ctx = make_context_with_counters()
    html = '<a > <b > <circle > <math > <ms >'
    _re.sub(r'<[a-zA-Z0-9_-]+ ',
            lambda m: generator.add_html_ids(m, ctx),
            html)
    assert ctx['htmlvarctr'] == 2
    assert ctx['svgvarctr'] == 1
    assert ctx['mathmlvarctr'] == 2


def test_add_html_ids_returns_match_with_id():
    import re as _re
    ctx = make_context_with_counters()
    match = _re.search(r'<[a-zA-Z0-9_-]+ ', '<div >')
    out = generator.add_html_ids(match, ctx)
    assert out.startswith('<div ')
    assert 'id="htmlvar00001"' in out


def test_generate_function_body_structure():
    random.seed(1)
    jsgrammar = mini_js_grammar()
    ctx = empty_html_context()
    generator.generate_html_elements(ctx, 2)
    body = generator.generate_function_body(jsgrammar, ctx, 4)
    assert body.startswith('var fuzzervars = {};\n')
    assert "SetVariable(fuzzervars, window, 'Window');" in body
    assert "SetVariable(fuzzervars, document, 'Document');" in body
    assert '//beginjs\n' in body
    assert '/* newvar{htmlvar00001:' in body
    assert '\n//endjs\n' in body
    assert 'freememory()' in body


def test_generate_function_body_lines_computed_from_grammar():
    random.seed(2)
    jsgrammar = mini_js_grammar()
    ctx = empty_html_context()
    body = generator.generate_function_body(jsgrammar, ctx, 3)
    assert 'doWork(' in body


def test_generate_new_sample_replaces_all_sections():
    random.seed(3)
    template = ('HEAD\n<cssfuzzer>\n<htmlfuzzer>\n<jsfuzzer>\nTAIL')
    sample = generator.generate_new_sample(template,
                                           mini_html_grammar(),
                                           mini_css_grammar(),
                                           mini_js_grammar())
    assert sample.startswith('HEAD\n')
    assert sample.endswith('TAIL')
    assert '<cssfuzzer>' not in sample
    assert '<htmlfuzzer>' not in sample
    assert '<jsfuzzer>' not in sample
    assert 'color:red;' in sample
    assert '<div id="htmlvar' in sample
    assert 'hello</div>' in sample
    assert '//beginjs' in sample


def test_generate_new_sample_annotates_html_ids():
    random.seed(4)
    template = ('HEAD\n<cssfuzzer>\n<htmlfuzzer>\n<jsfuzzer>\nTAIL')
    sample = generator.generate_new_sample(template,
                                           mini_html_grammar(),
                                           mini_css_grammar(),
                                           mini_js_grammar())
    assert 'id="htmlvar' in sample or 'id="svgvar' in sample or 'id="mathmlvar' in sample


def test_generate_new_sample_two_js_sections_use_eventhandler_budget(monkeypatch):
    monkeypatch.setattr(generator, '_N_MAIN_LINES', 3)
    monkeypatch.setattr(generator, '_N_EVENTHANDLER_LINES', 2)
    template = '<jsfuzzer>|<jsfuzzer>'
    sample = generator.generate_new_sample(template,
                                           mini_html_grammar(),
                                           mini_css_grammar(),
                                           mini_js_grammar())
    assert sample.count('//beginjs') == 2


def test_generate_new_sample_with_real_rules(real_grammars, real_template):
    htmlgrammar, cssgrammar, jsgrammar = real_grammars
    sample = generator.generate_new_sample(real_template, htmlgrammar, cssgrammar, jsgrammar)
    assert '<cssfuzzer>' not in sample
    assert '<htmlfuzzer>' not in sample
    assert '<jsfuzzer>' not in sample
    assert sample.count('//beginjs') >= 5
    assert '/* newvar{' in sample
    assert 'id="htmlvar' in sample


def test_check_grammar_reports_missing_creators(capsys):
    grammar = Grammar()
    grammar.parse_from_string('<root root=true> = <undefinedtag>\n')
    generator.check_grammar(grammar)
    assert 'No creators for type undefinedtag' in capsys.readouterr().out


def test_check_grammar_silent_for_complete_grammar(capsys):
    grammar = Grammar()
    grammar.parse_from_string('<root root=true> = "ok"\n')
    generator.check_grammar(grammar)
    assert capsys.readouterr().out == ''


def test_generate_samples_writes_real_sample(real_grammars, real_template, tmp_path, monkeypatch, capsys):
    monkeypatch.setattr(generator, '_N_MAIN_LINES', 20)
    monkeypatch.setattr(generator, '_N_EVENTHANDLER_LINES', 10)
    assert generator._N_MAIN_LINES == 20
    htmlgrammar, cssgrammar, jsgrammar = real_grammars
    outfile = tmp_path / 'sample.html'
    generator.generate_samples(real_template, [str(outfile)])
    assert outfile.exists()
    content = outfile.read_text()
    assert '//beginjs' in content
    assert 'Writing a sample to' in capsys.readouterr().out


def test_generate_samples_reports_grammar_errors(tmp_path, capsys, monkeypatch):
    class BrokenGrammar(object):
        def __init__(self):
            self.calls = 0

        def parse_from_file(self, path):
            self.calls += 1
            return 1

        def add_import(self, name, grammar):
            pass

    monkeypatch.setattr(generator, 'Grammar', BrokenGrammar)
    generator.generate_samples('template', [str(tmp_path / 'x.html')])
    assert 'There were errors parsing html grammar' in capsys.readouterr().out


def test_argument_parser_defaults():
    parser = generator.get_argument_parser()
    args = parser.parse_args([])
    assert 'template.html' in str(args.template)
    assert args.file is None
    assert args.output_dir is None
    assert args.no_of_files is None


def test_argument_parser_accepts_clusterfuzz_options(tmp_path):
    parser = generator.get_argument_parser()
    args = parser.parse_args(['-f', str(tmp_path / 'out.html'),
                              '-o', str(tmp_path),
                              '-n', '3'])
    assert args.file == str(tmp_path / 'out.html')
    assert args.output_dir == str(tmp_path)
    assert args.no_of_files == 3


def test_main_single_file_generation(monkeypatch, real_grammars, tmp_path, capsys):
    monkeypatch.setattr(generator, '_N_MAIN_LINES', 15)
    monkeypatch.setattr(generator, '_N_EVENTHANDLER_LINES', 5)
    out = tmp_path / 'out.html'
    monkeypatch.setattr('sys.argv', ['generator.py', '-f', str(out)])
    generator.main()
    assert out.exists()
    assert '//beginjs' in out.read_text()


def test_main_output_dir_requires_file_count(monkeypatch, capsys):
    monkeypatch.setattr('sys.argv', ['generator.py', '-o', '/tmp/some-dir'])
    generator.main()
    assert 'Please use switch -n' in capsys.readouterr().out


def test_main_without_options_prints_help(monkeypatch, capsys):
    monkeypatch.setattr('sys.argv', ['generator.py'])
    generator.main()
    out = capsys.readouterr().out
    assert 'DOMATO' in out or 'usage' in out.lower()


def test_main_output_dir_writes_multiple_files(monkeypatch, real_grammars, tmp_path, capsys):
    monkeypatch.setattr(generator, '_N_MAIN_LINES', 10)
    monkeypatch.setattr(generator, '_N_EVENTHANDLER_LINES', 5)
    out_dir = tmp_path / 'outdir'
    monkeypatch.setattr('sys.argv',
                        ['generator.py', '-o', str(out_dir), '-n', '2'])
    generator.main()
    assert (out_dir / 'fuzz-00000.html').exists()
    assert (out_dir / 'fuzz-00001.html').exists()
    assert 'Running on ClusterFuzz' in capsys.readouterr().out
