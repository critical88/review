#   Tests for the WebGPU driver's shader-analysis helpers and rule files.
#   --------------------------------------
#
#   Licensed under the Apache License, Version 2.0 (the "License");
#   you may not use this file except in compliance with the License.
#   You may obtain a copy of the License at
#
#       http://www.apache.org/licenses/LICENSE-2.0

import os

import pytest

from grammar import Grammar
from tests.helpers import REPO_ROOT, load_driver


@pytest.fixture(scope='module')
def webgpu():
    return load_driver('webgpu')


WGSL_SOURCE = """
alias RTArr = array<u32>;

@group(0) @binding(0) var<storage, read_write> x_5 : S;

fn main_1() { return; }

@compute @workgroup_size(1, 1, 1)
fn main(@builtin(global_invocation_id) x_2_param : vec3<u32>) {
  x_2 = x_2_param;
  main_1();
}

@fragment
fn frag_main() { return; }
"""

SHIPPED_SHADER = os.path.join(REPO_ROOT, 'webgpu', 'wgsl', 'domato-example.wgsl')


def test_extract_shader_stages_finds_compute_entrypoint(webgpu):
    stages = webgpu.extract_shader_stages_and_functions(WGSL_SOURCE)
    assert ('@compute', 'main') in stages
    assert ('@fragment', 'frag_main') in stages


def test_extract_shader_stages_ignores_plain_functions(webgpu):
    stages = webgpu.extract_shader_stages_and_functions(WGSL_SOURCE)
    names = [fn for _, fn in stages]
    assert 'main_1' not in names


def test_extract_shader_stages_on_shipped_shader(webgpu):
    with open(SHIPPED_SHADER) as f:
        source = f.read()
    stages = webgpu.extract_shader_stages_and_functions(source)
    assert stages
    assert ('@compute', 'main') in stages


def test_parse_entrypoints_formats_grammar_rules(webgpu):
    result = webgpu.parse_entrypoints([WGSL_SOURCE])
    assert '<entrypoint> = "main"' in result
    assert '<entrypoint> = "frag_main"' in result


def test_parse_entrypoints_empty_input(webgpu):
    assert webgpu.parse_entrypoints([]) == ''


def test_parse_bindings_formats_grammar_rules(webgpu):
    result = webgpu.parse_bindings([WGSL_SOURCE])
    assert '<BindInt> = 0' in result
    assert result.count('<BindInt> = ') == 1


def test_parse_bindings_on_shipped_shader(webgpu):
    with open(SHIPPED_SHADER) as f:
        source = f.read()
    result = webgpu.parse_bindings([source])
    assert '<BindInt> = 0' in result
    assert '<BindInt> = 1' in result


@pytest.mark.parametrize('rule_file', [
    'rules/html.txt', 'rules/css.txt', 'rules/js.txt', 'rules/common.txt',
    'rules/mathml.txt', 'rules/svg.txt', 'rules/tagattributes.txt',
])
def test_bundled_rule_files_parse_cleanly(rule_file):
    grammar = Grammar()
    assert grammar.parse_from_file(os.path.join(REPO_ROOT, rule_file)) == 0


@pytest.mark.parametrize('driver_dir,grammar_file', [
    ('canvas', 'canvas.txt'),
    ('jscript', 'jscript.txt'),
    ('php', 'php.txt'),
    ('vbscript', 'vbscript.txt'),
    ('webgl', 'webgl.txt'),
    ('webgpu', 'webgpu.txt'),
    ('webgpu', 'webgpuhelpers.txt'),
])
def test_driver_grammar_files_parse_cleanly(driver_dir, grammar_file):
    grammar = Grammar()
    path = os.path.join(REPO_ROOT, driver_dir, grammar_file)
    assert grammar.parse_from_file(path) == 0


def test_rule_directory_contains_expected_files():
    for required in ('css.txt', 'html.txt', 'js.txt', 'common.txt',
                     'attributevalues.txt', 'tagattributes.txt'):
        assert os.path.isfile(os.path.join(REPO_ROOT, 'rules', required))
    assert os.path.isfile(os.path.join(REPO_ROOT, 'template.html'))
    assert os.path.isfile(os.path.join(REPO_ROOT, 'webgpu', 'wgsl', 'domato-example.wgsl'))


def test_root_template_declares_fuzzer_placeholders():
    with open(os.path.join(REPO_ROOT, 'template.html')) as f:
        template = f.read()
    assert '<cssfuzzer>' in template
    assert '<htmlfuzzer>' in template
    assert '<jsfuzzer>' in template
    assert template.count('<jsfuzzer>') >= 5


def test_html_rules_reference_placeholder_sections():
    html_grammar = Grammar()
    assert html_grammar.parse_from_file(os.path.join(REPO_ROOT, 'rules', 'html.txt')) == 0
    assert 'bodyelements' in html_grammar._creators
    assert 'element' in html_grammar._creators


def test_js_rules_contain_line_rules_with_helpers():
    js_grammar = Grammar()
    assert js_grammar.parse_from_file(os.path.join(REPO_ROOT, 'rules', 'js.txt')) == 0
    assert 'line' in js_grammar._creators
    assert len(js_grammar._all_nonhelper_lines) > 100
