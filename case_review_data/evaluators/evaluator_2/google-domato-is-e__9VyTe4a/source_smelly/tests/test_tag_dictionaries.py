#   Tests for the bundled HTML, SVG and MathML tag tables.
#   --------------------------------------
#
#   Licensed under the Apache License, Version 2.0 (the "License");
#   you may not use this file except in compliance with the License.
#   You may obtain a copy of the License at
#
#       http://www.apache.org/licenses/LICENSE-2.0

from html_tags import _HTML_TYPES
from mathml_tags import _MATHML_TYPES
from svg_tags import _SVG_TYPES


def test_html_tag_table_maps_anchor():
    assert _HTML_TYPES['a'] == 'HTMLAnchorElement'


def test_html_tag_table_canvas():
    assert _HTML_TYPES['canvas'] == 'HTMLCanvasElement'


def test_html_tag_table_unknown_elements():
    assert _HTML_TYPES['b'] == 'HTMLUnknownElement'
    assert _HTML_TYPES['article'] == 'HTMLUnknownElement'


def test_html_tag_table_contains_expected_structure_tags():
    for required in ('div', 'span', 'table', 'img', 'select', 'input'):
        assert required in _HTML_TYPES


def test_html_keys_and_values_are_wellformed():
    for tag, type_name in _HTML_TYPES.items():
        assert isinstance(tag, str) and tag
        assert isinstance(type_name, str) and type_name
    assert len(_HTML_TYPES) > 80


def test_html_types_use_element_suffix():
    for type_name in _HTML_TYPES.values():
        assert 'Element' in type_name


def test_svg_tag_table_maps_circle():
    assert _SVG_TYPES['circle'] == 'SVGCircleElement'


def test_svg_tag_table_maps_animate_motion():
    assert _SVG_TYPES['animateMotion'] == 'SVGAnimateMotionElement'


def test_svg_tag_table_contains_root_and_defs():
    assert _SVG_TYPES['svg'] == 'SVGSVGElement'
    assert _SVG_TYPES['defs'] == 'SVGDefsElement'


def test_svg_keys_and_values_are_wellformed():
    for tag, type_name in _SVG_TYPES.items():
        assert isinstance(tag, str) and tag
        assert isinstance(type_name, str) and type_name
    assert len(_SVG_TYPES) > 60


def test_svg_types_use_svg_element_suffix():
    for type_name in _SVG_TYPES.values():
        assert type_name.startswith('SVG')


def test_mathml_tag_table_maps_math_element():
    assert _MATHML_TYPES['math'] == 'MathMLElement'


def test_mathml_tag_table_contains_content_elements():
    for required in ('mi', 'mo', 'mn', 'mfrac', 'msqrt', 'mrow'):
        assert required in _MATHML_TYPES


def test_mathml_keys_and_values_are_wellformed():
    for tag, type_name in _MATHML_TYPES.items():
        assert isinstance(tag, str) and tag
        assert type_name.startswith('MathML')
    assert len(_MATHML_TYPES) > 15


def test_tables_are_disjoint_namespaces():
    # The same tag name can appear in several namespaces but resolves to a
    # different concrete interface type in each.
    assert 'a' in _HTML_TYPES and 'a' in _SVG_TYPES
    assert _HTML_TYPES['a'] != _SVG_TYPES['a']
