#   Tests for the grammar engine's built-in type generators.
#   --------------------------------------
#
#   Licensed under the Apache License, Version 2.0 (the "License");
#   you may not use this file except in compliance with the License.
#   You may obtain a copy of the License at
#
#       http://www.apache.org/licenses/LICENSE-2.0

import random
import struct

import pytest

from grammar import Grammar, GrammarError


@pytest.fixture
def grammar():
    return Grammar()


TAG = {'type': 'tag', 'tagname': 'int'}


def test_generate_int_stays_in_range(grammar):
    random.seed(11)
    for _ in range(50):
        value = int(grammar._generate_int({'tagname': 'int', 'min': '0', 'max': '10'}))
        assert 0 <= value <= 10


def test_generate_int_defaults_to_32bit_range(grammar):
    random.seed(3)
    value = int(grammar._generate_int({'tagname': 'int32'}))
    assert -2147483648 <= value <= 2147483647


def test_generate_int_range_error(grammar):
    with pytest.raises(GrammarError):
        grammar._generate_int({'tagname': 'int', 'min': '10', 'max': '0'})


def test_generate_int_binary_output_is_packed(grammar):
    random.seed(5)
    tag = {'tagname': 'int32', 'min': '1', 'max': '1', 'b': True}
    assert grammar._generate_int(tag) == struct.pack('<i', 1)


def test_generate_int_binary_output_big_endian(grammar):
    tag = {'tagname': 'uint32', 'min': '1', 'max': '1', 'be': True}
    assert grammar._generate_int(tag) == struct.pack('>I', 1)


def test_generate_int_accepts_hex_bounds(grammar):
    random.seed(7)
    value = int(grammar._generate_int({'tagname': 'uint8', 'min': '0x10', 'max': '0x10'}))
    assert value == 16


def test_int_format_uses_tag_name(grammar):
    tag = {'tagname': 'uint8', 'min': '1', 'max': '1', 'b': True}
    assert grammar._generate_int(tag) == struct.pack('<B', 1)


def test_generate_float_defaults(grammar):
    random.seed(2)
    value = float(grammar._generate_float({'tagname': 'float'}))
    assert 0.0 <= value <= 1.0


def test_generate_float_respects_bounds(grammar):
    random.seed(2)
    tag = {'tagname': 'float', 'min': '10', 'max': '10'}
    assert float(grammar._generate_float(tag)) == 10.0


def test_generate_float_binary_double(grammar):
    tag = {'tagname': 'double', 'min': '0.5', 'max': '0.5', 'b': True}
    assert grammar._generate_float(tag) == struct.pack('d', 0.5)


def test_generate_float_range_error(grammar):
    with pytest.raises(GrammarError):
        grammar._generate_float({'tagname': 'float', 'min': '1', 'max': '0'})


def test_generate_char_from_code(grammar):
    assert grammar._generate_char({'tagname': 'char', 'code': '65'}) == 'A'


def test_generate_char_from_code_hex(grammar):
    assert grammar._generate_char({'tagname': 'char', 'code': '0x41'}) == 'A'


def test_generate_char_range(grammar):
    random.seed(9)
    for _ in range(20):
        value = grammar._generate_char({'tagname': 'char', 'min': '97', 'max': '122'})
        assert 97 <= ord(value) <= 122


def test_generate_char_range_error(grammar):
    with pytest.raises(GrammarError):
        grammar._generate_char({'tagname': 'char', 'min': '200', 'max': '100'})


def test_generate_string_length_bounds(grammar):
    random.seed(4)
    tag = {'tagname': 'string', 'min': '97', 'max': '122', 'minlength': '3', 'maxlength': '7'}
    for _ in range(20):
        value = grammar._generate_string(tag)
        assert 3 <= len(value) <= 7
        assert all(97 <= ord(c) <= 122 for c in value)


def test_generate_string_range_error(grammar):
    with pytest.raises(GrammarError):
        grammar._generate_string({'tagname': 'string', 'min': '100', 'max': '0'})


def test_generate_html_string_escapes_markup(grammar):
    random.seed(6)
    tag = {'tagname': 'htmlsafestring', 'min': '60', 'max': '62', 'minlength': '3', 'maxlength': '3'}
    value = grammar._generate_html_string(tag)
    assert '<' not in value and '>' not in value and '"' not in value


def test_generate_hex_lowercase_and_uppercase(grammar):
    random.seed(1)
    lowered = {grammar._generate_hex({'tagname': 'hex'}) for _ in range(200)}
    assert lowered.issubset(set('0123456789abcdef'))
    uppered = {grammar._generate_hex({'tagname': 'hex', 'up': True}) for _ in range(200)}
    assert uppered.issubset(set('0123456789ABCDEF'))


def test_constant_types_ascii(grammar):
    assert grammar._constant_types['lt'] == '<'
    assert grammar._constant_types['gt'] == '>'
    assert grammar._constant_types['hash'] == '#'
    assert grammar._constant_types['cr'] == chr(13)
    assert grammar._constant_types['lf'] == chr(10)
    assert grammar._constant_types['space'] == ' '
    assert grammar._constant_types['tab'] == chr(9)
    assert grammar._constant_types['ex'] == '!'


def test_built_in_type_table_is_complete(grammar):
    for name in ('int', 'int32', 'uint32', 'int8', 'uint8', 'int16', 'uint16',
                 'int64', 'uint64', 'float', 'double', 'char', 'string',
                 'htmlsafestring', 'hex', 'import', 'lines'):
        assert name in grammar._built_in_types


def test_string_to_int_handles_bases(grammar):
    assert grammar._string_to_int('10') == 10
    assert grammar._string_to_int('0x10') == 16
    assert grammar._string_to_int('0o7') == 7
