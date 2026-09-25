#   Domato - main generator script
#   --------------------------------------
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
import argparse
from pathlib import Path

from grammar import Grammar
from svg_tags import _SVG_TYPES
from html_tags import _HTML_TYPES
from mathml_tags import _MATHML_TYPES
from frontends import DocumentFuzzFrontend

_N_MAIN_LINES = 1000
_N_EVENTHANDLER_LINES = 500

_N_ADDITIONAL_HTMLVARS = 5

def generate_html_elements(ctx, n):
    for i in range(n):
        tag = random.choice(list(_HTML_TYPES))
        tagtype = _HTML_TYPES[tag]
        ctx['htmlvarctr'] += 1
        varname = 'htmlvar%05d' % ctx['htmlvarctr']
        ctx['htmlvars'].append({'name': varname, 'type': tagtype})
        ctx['htmlvargen'] += '/* newvar{' + varname + ':' + tagtype + '} */ var ' + varname + ' = document.createElement(\"' + tag + '\"); //' + tagtype + '\n'


def add_html_ids(matchobj, ctx):
    tagname = matchobj.group(0)[1:-1]
    if tagname in _HTML_TYPES:
        ctx['htmlvarctr'] += 1
        varname = 'htmlvar%05d' % ctx['htmlvarctr']
        ctx['htmlvars'].append({'name': varname, 'type': _HTML_TYPES[tagname]})
        ctx['htmlvargen'] += '/* newvar{' + varname + ':' + _HTML_TYPES[tagname] + '} */ var ' + varname + ' = document.getElementById(\"' + varname + '\"); //' + _HTML_TYPES[tagname] + '\n'
        return matchobj.group(0) + 'id=\"' + varname + '\" '
    elif tagname in _SVG_TYPES:
        ctx['svgvarctr'] += 1
        varname = 'svgvar%05d' % ctx['svgvarctr']
        ctx['htmlvars'].append({'name': varname, 'type': _SVG_TYPES[tagname]})
        ctx['htmlvargen'] += '/* newvar{' + varname + ':' + _SVG_TYPES[tagname] + '} */ var ' + varname + ' = document.getElementById(\"' + varname + '\"); //' + _SVG_TYPES[tagname] + '\n'
        return matchobj.group(0) + 'id=\"' + varname + '\" '
    elif tagname in _MATHML_TYPES:
        ctx['mathmlvarctr'] += 1
        varname = 'mathmlvar%05d' % ctx['mathmlvarctr']
        ctx['htmlvars'].append({'name': varname, 'type': _MATHML_TYPES[tagname]})
        ctx['htmlvargen'] += '/* newvar{' + varname + ':' + _MATHML_TYPES[tagname] + '} */ var ' + varname + ' = document.getElementById(\"' + varname + '\"); //' + _MATHML_TYPES[tagname] + '\n'
        return matchobj.group(0) + 'id=\"' + varname + '\" '
    else:
        return matchobj.group(0)


def generate_function_body(jsgrammar, htmlctx, num_lines):
    js = ''
    js += 'var fuzzervars = {};\n\n'
    js += "SetVariable(fuzzervars, window, 'Window');\nSetVariable(fuzzervars, document, 'Document');\nSetVariable(fuzzervars, document.body.firstChild, 'Element');\n\n"
    js += '//beginjs\n'
    js += htmlctx['htmlvargen']
    js += jsgrammar._generate_code(num_lines, htmlctx['htmlvars'])
    js += '\n//endjs\n'
    js += 'var fuzzervars = {};\nfreememory()\n'
    return js


def check_grammar(grammar):
    """Checks if grammar has errors and if so outputs them.
    Args:
      grammar: The grammar to check.
    """

    for rule in grammar._all_rules:
        for part in rule['parts']:
            if part['type'] == 'text':
                continue
            tagname = part['tagname']
            # print tagname
            if tagname not in grammar._creators:
                print('No creators for type ' + tagname)


def generate_new_sample(template, htmlgrammar, cssgrammar, jsgrammar):
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

    css = cssgrammar.generate_symbol('rules')
    html = htmlgrammar.generate_symbol('bodyelements')

    htmlctx = {
        'htmlvars': [],
        'htmlvarctr': 0,
        'svgvarctr': 0,
        'mathmlvarctr': 0,
        'htmlvargen': ''
    }
    html = re.sub(
        r'<[a-zA-Z0-9_-]+ ',
        lambda match: add_html_ids(match, htmlctx),
        html
    )
    generate_html_elements(htmlctx, _N_ADDITIONAL_HTMLVARS)

    result = result.replace('<cssfuzzer>', css)
    result = result.replace('<htmlfuzzer>', html)

    handlers = False
    while '<jsfuzzer>' in result:
        numlines = _N_MAIN_LINES
        if handlers:
            numlines = _N_EVENTHANDLER_LINES
        else:
            handlers = True
        result = result.replace(
            '<jsfuzzer>',
            generate_function_body(jsgrammar, htmlctx, numlines),
            1
        )

    return result

def generate_samples(template, outfiles):
    """Generates a set of samples and writes them to the output files.
    Args:
      grammar_dir: directory to load grammar files from.
      outfiles: A list of output filenames.
    """

    grammar_dir = os.path.join(os.path.dirname(__file__), 'rules')
    htmlgrammar = Grammar()

    err = htmlgrammar.parse_from_file(os.path.join(grammar_dir, 'html.txt'))
    # CheckGrammar(htmlgrammar)
    if err > 0:
        print('There were errors parsing html grammar')
        return

    cssgrammar = Grammar()
    err = cssgrammar.parse_from_file(os.path.join(grammar_dir ,'css.txt'))
    # CheckGrammar(cssgrammar)
    if err > 0:
        print('There were errors parsing css grammar')
        return

    jsgrammar = Grammar()
    err = jsgrammar.parse_from_file(os.path.join(grammar_dir,'js.txt'))
    # CheckGrammar(jsgrammar)
    if err > 0:
        print('There were errors parsing js grammar')
        return

    # JS and HTML grammar need access to CSS grammar.
    # Add it as import
    htmlgrammar.add_import('cssgrammar', cssgrammar)
    jsgrammar.add_import('cssgrammar', cssgrammar)

    for outfile in outfiles:
        result = generate_new_sample(template, htmlgrammar, cssgrammar, jsgrammar)
        if result is not None:
            print('Writing a sample to ' + outfile)
            try:
                with open(outfile, 'w') as f:
                    f.write(result)
            except IOError:
                print('Error writing to output')

class DomatoFuzzFrontend(DocumentFuzzFrontend):
    """Frontend for the full DOM testcase generator.

    Drives the html/css/js rule files through the shared document
    frontend contract so the DOM generator can be treated like every
    other domato frontend.
    """

    def __init__(self, grammar_dir=None, template_name=None):
        if grammar_dir is None:
            grammar_dir = os.path.join(
                os.path.dirname(os.path.abspath(__file__)), 'rules')
        if template_name is None:
            template_name = 'template.html'
        super(DomatoFuzzFrontend, self).__init__(grammar_dir, template_name)

    def load_grammars(self):
        """Loads and parses every grammar needed by this frontend.

        Returns:
          A tuple of parsed grammars, or None if parsing failed.
        """
        htmlgrammar = Grammar()

        err = htmlgrammar.parse_from_file(
            os.path.join(self.grammar_dir, 'html.txt'))
        # CheckGrammar(htmlgrammar)
        if err > 0:
            print('There were errors parsing html grammar')
            return None

        cssgrammar = Grammar()
        err = cssgrammar.parse_from_file(
            os.path.join(self.grammar_dir, 'css.txt'))
        # CheckGrammar(cssgrammar)
        if err > 0:
            print('There were errors parsing css grammar')
            return None

        jsgrammar = Grammar()
        err = jsgrammar.parse_from_file(
            os.path.join(self.grammar_dir, 'js.txt'))
        # CheckGrammar(jsgrammar)
        if err > 0:
            print('There were errors parsing js grammar')
            return None

        # JS and HTML grammar need access to CSS grammar.
        # Add it as import
        htmlgrammar.add_import('cssgrammar', cssgrammar)
        jsgrammar.add_import('cssgrammar', cssgrammar)

        return (htmlgrammar, cssgrammar, jsgrammar)

    def prepare_html_context(self):
        """Creates the element variable bookkeeping for DOM samples."""
        return {
            'htmlvars': [],
            'htmlvarctr': 0,
            'svgvarctr': 0,
            'mathmlvarctr': 0,
            'htmlvargen': ''
        }

    def annotate_document_ids(self, html_string, html_context):
        """Inserts stable element ids into a generated DOM tree.

        Args:
          html_string: The generated HTML fragment.
          html_context: The element variable bookkeeping.

        Returns:
          The annotated HTML fragment.
        """
        return re.sub(
            r'<[a-zA-Z0-9_-]+ ',
            lambda match: add_html_ids(match, html_context),
            html_string
        )

    def spawn_document_elements(self, count, html_context):
        """Emits element constructor lines into the bookkeeping."""
        generate_html_elements(html_context, count)

    def generate_function_body(self, jsgrammar, html_context, num_lines):
        """Generates one chunk of grammar-driven code for a sample."""
        return generate_function_body(jsgrammar, html_context, num_lines)

    def generate_new_sample(self, template, *grammars):
        """Renders a single testcase from a template and grammars.

        Args:
          template: A template string.
          grammars: The (html, css, js) grammars for this frontend.

        Returns:
          A string containing sample data.
        """
        htmlgrammar, cssgrammar, jsgrammar = grammars

        result = template

        css = cssgrammar.generate_symbol('rules')
        html = htmlgrammar.generate_symbol('bodyelements')

        htmlctx = self.prepare_html_context()
        html = self.annotate_document_ids(html, htmlctx)
        self.spawn_document_elements(_N_ADDITIONAL_HTMLVARS, htmlctx)

        result = result.replace('<cssfuzzer>', css)
        result = result.replace('<htmlfuzzer>', html)

        handlers = False
        while '<jsfuzzer>' in result:
            numlines = _N_MAIN_LINES
            if handlers:
                numlines = _N_EVENTHANDLER_LINES
            else:
                handlers = True
            result = result.replace(
                '<jsfuzzer>',
                self.generate_function_body(jsgrammar, htmlctx, numlines),
                1
            )

        return result

    def extract_shader_stages(self, code):
        """Extracts (stage, function) pairs from a shader source.

        Args:
          code: The shader source code.

        Returns:
          A list of (stage attribute, function name) tuples.
        """
        pattern = (r'@(?:compute|vertex|fragment)'
                   r'(?:\s+@[^(\n]+(?:\([^)]*\))?)*\s*(?:\n\s*)?fn\s+(\w+)')
        stages = []
        for match in re.finditer(pattern, code):
            stage_attr = re.search(
                r'@(compute|vertex|fragment)', match.group(0)).group(0)
            stages.append((stage_attr, match.group(1)))
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
        return ''.join('<entrypoint> = "%s"\n' % ep for ep in entrypoints)

    def collect_shader_bindings(self, sources):
        """Formats grammar rules for the bindings of loaded shaders.

        Args:
          sources: A list of shader sources.

        Returns:
          A string of <BindInt> grammar rules.
        """
        binding_pattern = r'@binding\((\d+)\)'
        numbers = []
        for shader in sources:
            numbers.extend(
                match.group(1) for match in re.finditer(binding_pattern, shader))
        return '\n'.join('<BindInt> = %s' % n for n in numbers)

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
          language: The name of the language, e.g. 'html', 'css' or 'js'.
        """
        return language in ('html', 'css', 'js')

    def emit_probe_snippet(self, language):
        """Returns a small probe snippet used to smoke-test a frontend.

        Args:
          language: The language the probe snippet is emitted for.

        Returns:
          A string containing a tiny language sample, or None.
        """
        if language == 'html':
            return '<div id="probe"></div>'
        if language == 'css':
            return '#probe { display: none; }'
        if language == 'js':
            return 'var probe = document.getElementById("probe");'
        return None


def get_argument_parser():
    parser = argparse.ArgumentParser(description="DOMATO (A DOM FUZZER)")
    
    parser.add_argument("-f", "--file", 
    help="File name which is to be generated in the same directory")

    parser.add_argument('-o', '--output_dir', type=str,
                    help='The output directory to put the generated files in')

    parser.add_argument('-n', '--no_of_files', type=int,
                    help='number of files to be generated')

    parser.add_argument('-t', '--template', type=Path, default=(Path(__file__).parent).joinpath('template.html'),
                    help='template file you want to use')
    return parser

def main():

    parser = get_argument_parser()

    args = parser.parse_args()

    with args.template.open("r") as f:
        template = f.read()

    frontend = DomatoFuzzFrontend()
    frontend.load_template_file(args.template)

    if args.file:
        frontend.write_samples([args.file])

    elif args.output_dir:
        if not args.no_of_files:
            print("Please use switch -n to specify the number of files")
        else:
            print('Running on ClusterFuzz')
            out_dir = args.output_dir
            nsamples = args.no_of_files
            print('Output directory: ' + out_dir)
            print('Number of samples: ' + str(nsamples))

            if not os.path.exists(out_dir):
                os.mkdir(out_dir)

            outfiles = []
            for i in range(nsamples):
                outfiles.append(os.path.join(out_dir, 'fuzz-' + str(i).zfill(5) + '.html'))

            frontend.write_samples(outfiles)


    else:
        parser.print_help()


if __name__ == '__main__':
    
    main()
