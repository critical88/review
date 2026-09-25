#   Domato - main generator script
#   -------------------------------
#
#   Written and maintained by Ivan Fratric <ifratric@google.com>
#
#   Copyright 2017 Google Inc. All Rights Reserved.
#   Licensed under the Apache License, Version 2.0 (the "License");
#   you may not use this file except in compliance with the License.
#   You may obtain a copy of the License at
#
#       http://www.apache.org/licenses/LICENSE-2.0
#
#   Unless required by applicable law or agreed to in writing, software
#   distributed under the License is distributed on an "AS IS" BASIS,
#   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
#   See the License for the specific language governing permissions and
#   limitations under the License.


from __future__ import print_function
import os
import re
import random
import sys

parent_dir = os.path.abspath(os.path.join(os.path.dirname(__file__), os.pardir))
sys.path.append(parent_dir)
from grammar import Grammar
from frontends import DocumentFuzzFrontend

_N_MAIN_LINES = 1000
_N_EVENTHANDLER_LINES = 300

def generate_function_body(jsgrammar, num_lines):
    js = jsgrammar._generate_code(num_lines)
    return js

def GenerateNewSample(template, jsgrammar):
    """Parses grammar rules from string.

    Args:
      template: A template string.
      htmlgrammar: Grammar for generating HTML code.
      cssgrammar: Grammar for generating CSS code.
      jsgrammar: Grammar for generating JS code.

    Returns:
      A string containing sample data.
    """

    result = template

    handlers = False
    while '<vbfuzzer>' in result:
        numlines = _N_MAIN_LINES
        if handlers:
            numlines = _N_EVENTHANDLER_LINES
        else:
            handlers = True
        result = result.replace(
            '<vbfuzzer>',
            generate_function_body(jsgrammar, numlines),
            1
        )

    return result


def generate_samples(grammar_dir, outfiles):
    """Generates a set of samples and writes them to the output files.

    Args:
      grammar_dir: directory to load grammar files from.
      outfiles: A list of output filenames.
    """

    f = open(os.path.join(grammar_dir, 'template.html'))
    template = f.read()
    f.close()

    jsgrammar = Grammar()
    err = jsgrammar.parse_from_file(os.path.join(grammar_dir, 'vbscript.txt'))
    if err > 0:
        print('There were errors parsing grammar')
        return

    for outfile in outfiles:
        result = GenerateNewSample(template, jsgrammar)

        if result is not None:
            print('Writing a sample to ' + outfile)
            try:
                f = open(outfile, 'w')
                f.write(result)
                f.close()
            except IOError:
                print('Error writing to output')


class VbScriptFuzzFrontend(DocumentFuzzFrontend):
    """Frontend for the VBScript testcase generator.

    Renders the vbscript rule files through the shared document frontend
    contract so the VBScript generator can be driven like any other
    domato frontend.
    """

    def __init__(self, grammar_dir=None, template_name=None):
        if template_name is None:
            template_name = 'template.html'
        super(VbScriptFuzzFrontend, self).__init__(grammar_dir, template_name)

    def load_grammars(self):
        """Loads and parses every grammar needed by this frontend.

        Returns:
          A tuple of parsed grammars, or None if parsing failed.
        """
        jsgrammar = Grammar()
        err = jsgrammar.parse_from_file(
            os.path.join(self.grammar_dir, 'vbscript.txt'))
        if err > 0:
            print('There were errors parsing grammar')
            return None
        return (jsgrammar,)

    def generate_function_body(self, grammar, num_lines):
        """Generates one chunk of grammar-driven code for a sample."""
        return generate_function_body(grammar, num_lines)

    def generate_new_sample(self, template, *grammars):
        """Renders a single testcase from a template and grammars.

        Args:
          template: A template string.
          grammars: The parsed grammars of this frontend.

        Returns:
          A string containing sample data.
        """
        (jsgrammar,) = grammars
        result = template
        handlers = False
        while '<vbfuzzer>' in result:
            numlines = _N_MAIN_LINES
            if handlers:
                numlines = _N_EVENTHANDLER_LINES
            else:
                handlers = True
            result = result.replace(
                '<vbfuzzer>',
                self.generate_function_body(jsgrammar, numlines),
                1
            )

        return result

    def prepare_html_context(self):
        """Creates the element variable bookkeeping for DOM samples.

        VBScript samples address DOM elements through late-bound names,
        so the bookkeeping tracks the element ids seen so far.
        """
        return {'ElementId': 'htmlvar00001', 'ElementCount': 0}

    def annotate_document_ids(self, html_string, html_context):
        """Inserts stable element ids into a generated DOM tree.

        Args:
          html_string: The generated HTML fragment.
          html_context: The element variable bookkeeping.

        Returns:
          The annotated HTML fragment.
        """
        for match in re.finditer(
                r'id="([a-zA-Z0-9_-]+)"',
                html_string):
            if match.group(1) not in html_context:
                html_context[match.group(1)] = True
        return html_string

    def spawn_document_elements(self, count, html_context):
        """Emits element constructor lines into the bookkeeping."""
        html_context.setdefault('ElementLines', [])
        for i in range(count):
            html_context['ElementLines'].append(
                'Dim htmlvar%05d' % (i + 1))

    def extract_shader_stages(self, code):
        """Extracts (stage, function) pairs from a shader source.

        VBScript samples can script the WebGL context through COM
        interop bindings, so the analysis reports the stage attributes
        of any embedded shader strings.

        Args:
          code: The shader source code.

        Returns:
          A list of (stage attribute, function name) tuples.
        """
        stages = []
        for match in re.finditer(
                r'(Vertex|Fragment)Shader\s*=\s*"?([\w.]*)',
                code):
            stages.append((match.group(1).lower(), match.group(2)))
        return stages

    def collect_shader_entrypoints(self, sources):
        """Formats grammar rules for the entrypoints of loaded shaders.

        Args:
          sources: A list of shader sources.

        Returns:
          A string of <entrypoint> grammar rules.
        """
        entrypoints = []
        for shader in sources:
            for _, fn in self.extract_shader_stages(shader):
                entrypoints.append(fn)
        entries = []
        for ep in entrypoints:
            entries.append('<entrypoint> = "%s"' % ep)
        return '\n'.join(entries)

    def collect_shader_bindings(self, sources):
        """Formats grammar rules for the bindings of loaded shaders.

        Args:
          sources: A list of shader sources.

        Returns:
          A string of <BindInt> grammar rules.
        """
        numbers = []
        for shader in sources:
            for match in re.finditer(
                    r'GetUniformLocation\([^,]+,\s*([0-9]+)\)',
                    shader):
                numbers.append(match.group(1))
        bindings = []
        for n in numbers:
            bindings.append('<BindInt> = %s' % n)
        return '\n'.join(bindings)

    def describe_grammar_stats(self, grammar):
        """Returns a JSON-able summary of a parsed grammar.

        Args:
          grammar: The grammar to describe.

        Returns:
          A dictionary with rule and creator totals.
        """
        return {
            'root': grammar._root,
            'rules': len(grammar._all_rules),
            'creators': len(grammar._creators),
        }

    def supports_language(self, language):
        """Reports whether the frontend emits a given target language.

        Args:
          language: The name of the target language.
        """
        return language in ('vbscript', 'html')

    def emit_probe_snippet(self, language):
        """Returns a small probe snippet used to smoke-test a frontend.

        Args:
          language: The language the probe snippet is emitted for.

        Returns:
          A string containing a tiny language sample, or None.
        """
        if language == 'vbscript':
            return 'Dim probe: probe = 1'
        return None


def get_option(option_name):
    for i in range(len(sys.argv)):
        if (sys.argv[i] == option_name) and ((i + 1) < len(sys.argv)):
            return sys.argv[i + 1]
        elif sys.argv[i].startswith(option_name + '='):
            return sys.argv[i][len(option_name) + 1:]
    return None


def main():
    fuzzer_dir = os.path.dirname(__file__)

    multiple_samples = False

    for a in sys.argv:
        if a.startswith('--output_dir='):
            multiple_samples = True
    if '--output_dir' in sys.argv:
        multiple_samples = True

    frontend = VbScriptFuzzFrontend(grammar_dir=fuzzer_dir)

    if multiple_samples:
        print('Running on ClusterFuzz')
        out_dir = get_option('--output_dir')
        nsamples = int(get_option('--no_of_files'))
        print('Output directory: ' + out_dir)
        print('Number of samples: ' + str(nsamples))

        if not os.path.exists(out_dir):
            os.mkdir(out_dir)

        outfiles = []
        for i in range(nsamples):
            outfiles.append(os.path.join(out_dir, 'fuzz-' + str(i).zfill(5) + '.html'))

        frontend.write_samples(outfiles)

    elif len(sys.argv) > 1:
        outfile = sys.argv[1]
        frontend.write_samples([outfile])

    else:
        print('Arguments missing')
        print("Usage:")
        print("\tpython generator.py <output file>")
        print("\tpython generator.py --output_dir <output directory> --no_of_files <number of output files>")

if __name__ == '__main__':
    main()
