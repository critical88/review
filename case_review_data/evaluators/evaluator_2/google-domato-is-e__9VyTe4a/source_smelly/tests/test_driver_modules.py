#   Tests for the bundled per-language driver scripts.
#   --------------------------------------
#
#   Licensed under the Apache License, Version 2.0 (the "License");
#   you may not use this file except in compliance with the License.
#   You may obtain a copy of the License at
#
#       http://www.apache.org/licenses/LICENSE-2.0

import os

import pytest

from tests.helpers import REPO_ROOT, load_driver, ALL_DRIVERS


@pytest.fixture(scope='module')
def canvas():
    return load_driver('canvas')


@pytest.fixture(scope='module')
def jscript():
    return load_driver('jscript')


@pytest.fixture(scope='module')
def php():
    return load_driver('php')


@pytest.fixture(scope='module')
def vbscript():
    return load_driver('vbscript')


@pytest.fixture(scope='module')
def webgl():
    return load_driver('webgl')


@pytest.fixture(scope='module')
def webgpu():
    return load_driver('webgpu')


@pytest.mark.parametrize('name', sorted(ALL_DRIVERS))
def test_all_bundled_driver_scripts_exist(name):
    assert os.path.isfile(ALL_DRIVERS[name])


@pytest.mark.parametrize('name', ['canvas', 'jscript', 'php', 'vbscript', 'webgl', 'webgpu'])
def test_driver_modules_expose_pipeline_surface(name):
    module = load_driver(name)
    assert hasattr(module, 'generate_samples')
    assert hasattr(module, 'main')


@pytest.mark.parametrize('name', ['canvas', 'jscript', 'php', 'vbscript', 'webgl', 'webgpu'])
def test_driver_get_option_parsing(name, monkeypatch):
    module = load_driver(name)
    monkeypatch.setattr('sys.argv',
                        ['generator.py', '--output_dir', '/tmp/outdir', '--no_of_files', '3'])
    assert module.get_option('--output_dir') == '/tmp/outdir'
    assert module.get_option('--no_of_files') == '3'
    assert module.get_option('--missing') is None


@pytest.mark.parametrize('name', ['canvas', 'jscript', 'php', 'vbscript', 'webgl', 'webgpu'])
def test_driver_get_option_equals_form(name, monkeypatch):
    module = load_driver(name)
    monkeypatch.setattr('sys.argv', ['generator.py', '--output_dir=/some/dir'])
    assert module.get_option('--output_dir') == '/some/dir'
    assert module.get_option('--no_of_files') is None


def test_canvas_driver_writes_sample(canvas, tmp_path, monkeypatch):
    out = tmp_path / 'canvas.html'
    monkeypatch.setattr(canvas, '_N_MAIN_LINES', 20)
    canvas.generate_samples(os.path.join(REPO_ROOT, 'canvas'), [str(out)])
    assert out.exists()
    content = out.read_text()
    assert content.startswith('<!--')
    assert '<canvasfuzz>' not in content


def test_canvas_sample_body_uses_generated_lines(canvas):
    import random as _random
    _random.seed(8)
    grammar = _canvas_grammar()
    body = canvas.generate_function_body(grammar, 3)
    assert '/* newvar{' in body or 'GetVariable' in body or 'var ' in body


def test_canvas_generate_new_sample_replaces_section(canvas):
    with open(os.path.join(REPO_ROOT, 'canvas', 'template.html')) as f:
        template = f.read()
    assert '<canvasfuzz>' in template
    import random as _random
    _random.seed(5)
    grammar = _canvas_grammar()
    monkeypatch_none = None
    sample = canvas.GenerateNewSample(template, grammar)
    assert '<canvasfuzz>' not in sample


def _canvas_grammar():
    from grammar import Grammar
    grammar = Grammar()
    grammar_dir = os.path.join(REPO_ROOT, 'canvas')
    err = grammar.parse_from_file(os.path.join(grammar_dir, 'canvas.txt'))
    assert err == 0
    return grammar


def test_jscript_sample_contains_collect_garbage(jscript, tmp_path, monkeypatch):
    out = tmp_path / 'jscript.html'
    monkeypatch.setattr(jscript, '_N_MAIN_LINES', 30)
    monkeypatch.setattr(jscript, '_N_EVENTHANDLER_LINES', 10)
    jscript.generate_samples(os.path.join(REPO_ROOT, 'jscript'), [str(out)])
    assert out.exists()
    content = out.read_text()
    assert 'CollectGarbage();' in content
    assert '//beginjs' in content


def test_jscript_function_body_shape(jscript):
    import random as _random
    _random.seed(9)
    grammar = _jscript_grammar()
    body = jscript.generate_function_body(grammar, 2)
    assert '//beginjs\n' in body
    assert '\n//endjs\n' in body
    assert 'CollectGarbage();' in body
    assert ('throw new Error();' in body) or ('return vars[' in body)


def _jscript_grammar():
    from grammar import Grammar
    grammar = Grammar()
    assert grammar.parse_from_file(os.path.join(REPO_ROOT, 'jscript', 'jscript.txt')) == 0
    return grammar


def test_php_driver_writes_php_sample(php, tmp_path, monkeypatch):
    out = tmp_path / 'sample.php'
    monkeypatch.setattr(php, '_N_MAIN_LINES', 20)
    monkeypatch.setattr(php, '_N_EVENTHANDLER_LINES', 10)
    php.generate_samples(os.path.join(REPO_ROOT, 'php'), [str(out)])
    assert out.exists()
    content = out.read_text()
    assert '<phpfuzzer>' not in content
    assert content.lstrip().startswith('<?php')


def test_php_generate_new_sample_replaces_marker(tmp_path):
    php = load_driver('php')
    with open(os.path.join(REPO_ROOT, 'php', 'template.php')) as f:
        template = f.read()
    assert '<phpfuzzer>' in template
    out = php.generate_new_sample(template, _php_grammar())
    assert '<phpfuzzer>' not in out


def _php_grammar():
    from grammar import Grammar
    grammar = Grammar()
    assert grammar.parse_from_file(os.path.join(REPO_ROOT, 'php', 'php.txt')) == 0
    return grammar


def test_vbscript_driver_writes_sample(vbscript, tmp_path, monkeypatch):
    out = tmp_path / 'vb.html'
    monkeypatch.setattr(vbscript, '_N_MAIN_LINES', 20)
    monkeypatch.setattr(vbscript, '_N_EVENTHANDLER_LINES', 10)
    vbscript.generate_samples(os.path.join(REPO_ROOT, 'vbscript'), [str(out)])
    assert out.exists()
    content = out.read_text()
    assert '<vbfuzzer>' not in content
    assert len(content) > 500


def test_webgl_driver_writes_sample(webgl, tmp_path, monkeypatch):
    out = tmp_path / 'webgl.html'
    monkeypatch.setattr(webgl, '_N_MAIN_LINES', 15)
    webgl.generate_samples(os.path.join(REPO_ROOT, 'webgl'), [str(out)])
    assert out.exists()
    content = out.read_text()
    assert '<glfuzz>' not in content


def test_webgpu_driver_writes_sample(webgpu, tmp_path, monkeypatch):
    monkeypatch.setattr(webgpu, '_N_MAIN_LINES', 10)
    out = tmp_path / 'webgpu.html'
    webgpu.generate_samples('template.html', os.path.join(REPO_ROOT, 'webgpu'), [str(out)])
    assert out.exists()
    content = out.read_text()
    assert '<webgpufuzz>' not in content
    assert 'entrypoint' in content or '@compute' in content or 'fn main' in content


def test_webgpu_sample_contains_shader_source(webgpu, tmp_path, monkeypatch):
    monkeypatch.setattr(webgpu, '_N_MAIN_LINES', 5)
    out = tmp_path / 'webgpu-shader.html'
    webgpu.generate_samples('template.html', os.path.join(REPO_ROOT, 'webgpu'), [str(out)])
    content = out.read_text()
    assert 'main_1' in content or 'fn ' in content


@pytest.mark.parametrize('name', ['canvas', 'jscript', 'php', 'vbscript', 'webgl', 'webgpu'])
def test_driver_main_usage_without_arguments(name, monkeypatch, capsys):
    module = load_driver(name)
    monkeypatch.setattr('sys.argv', ['generator.py'])
    module.main()
    out = capsys.readouterr().out
    assert 'Arguments missing' in out or 'not used' in out


@pytest.mark.parametrize('name', ['canvas', 'jscript', 'php', 'vbscript', 'webgl'])
def test_driver_main_single_output_file(name, monkeypatch, tmp_path):
    module = load_driver(name)
    out = tmp_path / ('%s-out.html' % name)
    monkeypatch.setattr(module, '_N_MAIN_LINES', 6)
    if hasattr(module, '_N_EVENTHANDLER_LINES'):
        monkeypatch.setattr(module, '_N_EVENTHANDLER_LINES', 3)
    monkeypatch.setattr('sys.argv', ['generator.py', str(out)])
    module.main()
    assert out.exists()


def test_webgpu_main_output_dir(monkeypatch, tmp_path):
    webgpu = load_driver('webgpu')
    monkeypatch.setattr(webgpu, '_N_MAIN_LINES', 3)
    out_dir = tmp_path / 'wgpu'
    monkeypatch.setattr('sys.argv',
                        ['generator.py', '--output_dir', str(out_dir), '--no_of_files', '2'])
    webgpu.main()
    assert (out_dir / 'fuzz-00000.html').exists()
    assert (out_dir / 'fuzz-00001.html').exists()


@pytest.mark.parametrize('name', ['canvas', 'jscript', 'php', 'vbscript', 'webgl', 'webgpu'])
def test_driver_modules_declare_line_budgets(name):
    module = load_driver(name)
    lines = getattr(module, '_N_MAIN_LINES')
    assert isinstance(lines, int) and lines >= 1
