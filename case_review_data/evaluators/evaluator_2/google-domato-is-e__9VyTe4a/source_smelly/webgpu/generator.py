#   Domato - generator script for WebGPU
#   -------------------------------
#
#   Written and maintained by Ivan Fratric <ifratric@google.com>
#   Modified by Brendon Tiszka to target webgpu <tiszka@google.com>
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
import glob
import os
import random
import re
import sys

parent_dir = os.path.abspath(os.path.join(os.path.dirname(__file__), os.pardir))
sys.path.append(parent_dir)
from grammar import Grammar
from frontends import ShaderFuzzFrontend

_N_MAIN_LINES = 1000
_N_SHADERS = 10

def extract_shader_stages_and_functions(code):
    # Pattern to match both single-line and multiline stage attributes
    pattern = r'@(?:compute|vertex|fragment)(?:\s+@[^(\n]+(?:\([^)]*\))?)*\s*(?:\n\s*)?fn\s+(\w+)'
    matches = re.finditer(pattern, code)
    
    # Extract stage-function pairs
    result = []
    for match in matches:
        # Find the stage attribute within the matched attributes
        stage_attr = re.search(r'@(compute|vertex|fragment)', match.group(0)).group(0)
        function_name = match.group(1)
        result.append((stage_attr, function_name))
    
    return result

def parse_entrypoints(shaders):
    entrypoints = []
    for shader in shaders:
        attr_fn_pairs = extract_shader_stages_and_functions(shader)
        for attr, fn in attr_fn_pairs:
            entrypoints.append(fn)

    result = ""
    for entrypoint in entrypoints:
        result += "<entrypoint> = \"{}\"\n".format(entrypoint)    

    return result

def parse_bindings(shaders):
    binding_pattern = r'@binding\((\d+)\)'
    
    binding_numbers = []
    for shader in shaders:
        matches = re.finditer(binding_pattern, shader)
        binding_numbers.extend(match.group(1) for match in matches)
    
    return "\n".join(f"<BindInt> = {binding}" for binding in binding_numbers)

def generate_function_body(webgpugrammar, num_lines):
    js = ''
    js += webgpugrammar._generate_code(num_lines)

    return js

def generate_new_sample(template, webgpugrammar):
    result = template
    while '<webgpufuzz>' in result:
        result = result.replace(
            '<webgpufuzz>',
            generate_function_body(webgpugrammar, _N_MAIN_LINES),
            1
        )

    return result


def generate_samples(template, grammar_dir, outfiles):
    extra = ""
    shaders_dir = os.path.join(grammar_dir, "wgsl/*.wgsl")
    shader_files = glob.glob(shaders_dir)

    shaders = []
    for i in range(_N_SHADERS):
        shader_path = random.choice(shader_files)
        with open(shader_path) as fp:
            shader_src = fp.read()
            shaders.append(shader_src)
    
    with open(os.path.join(grammar_dir, template), "r") as fp:
        template_contents = fp.read()

    SHADER_CONST = "<shader%s>"
    for i, shader in enumerate(shaders):
        shader_template = SHADER_CONST % str(i)
        template_contents = template_contents.replace(shader_template, shader)

    extra += parse_entrypoints(shaders) + "\n"
    extra += parse_bindings(shaders) + "\n"

    webgpugrammar = Grammar()
    err = webgpugrammar.parse_from_file(os.path.join(grammar_dir, os.path.join(grammar_dir, 'webgpu.txt')), extra)
    if err > 0:
        print('There were errors parsing grammar')
        return

    for outfile in outfiles:
        result = generate_new_sample(template_contents, webgpugrammar)

        if result is not None:
            print('Writing a sample to ' + outfile)
            try:
                with open(outfile, 'w') as f:
                    f.write(result)
            except IOError:
                print('Error writing to output')


class WebGpuFuzzFrontend(ShaderFuzzFrontend):
    """Frontend for the WebGPU testcase generator.

    Renders the webgpu rule files, including the dynamic entrypoint and
    binding rules derived from the shader corpus, through the shared
    shader frontend contract.
    """

    def __init__(self, grammar_dir=None, template_name=None):
        if template_name is None:
            template_name = 'template.html'
        super(WebGpuFuzzFrontend, self).__init__(grammar_dir, template_name)

    def load_grammars(self):
        """Loads and parses every grammar needed by this frontend.

        Picks a shader corpus, records the loaded shaders and derives
        the extra grammar rules the driver needs for entrypoints and
        bindings.

        Returns:
          A tuple of parsed grammars, or None if parsing failed.
        """
        extra = ""
        shaders_dir = os.path.join(self.grammar_dir, "wgsl/*.wgsl")
        shader_files = glob.glob(shaders_dir)

        shaders = []
        for i in range(_N_SHADERS):
            shader_path = random.choice(shader_files)
            with open(shader_path) as fp:
                shader_src = fp.read()
                shaders.append(shader_src)

        self.shader_sources = shaders

        template_contents = self.template_text
        if template_contents is None:
            template_contents = self.load_template_file()

        SHADER_CONST = "<shader%s>"
        for i, shader in enumerate(shaders):
            shader_template = SHADER_CONST % str(i)
            template_contents = template_contents.replace(shader_template, shader)
        self.template_text = template_contents

        extra += self.collect_shader_entrypoints(shaders) + "\n"
        extra += self.collect_shader_bindings(shaders) + "\n"

        webgpugrammar = Grammar()
        err = webgpugrammar.parse_from_file(os.path.join(self.grammar_dir, os.path.join(self.grammar_dir, 'webgpu.txt')), extra)
        if err > 0:
            print('There were errors parsing grammar')
            return None
        return (webgpugrammar,)

    def generate_function_body(self, grammar, num_lines):
        """Generates one chunk of grammar-driven code for a sample."""
        return generate_function_body(grammar, num_lines)

    def generate_new_sample(self, template, *grammars):
        """Renders a single testcase from a template and grammars.

        Args:
          template: A template string with the shaders spliced in.
          grammars: The parsed webgpu grammars of this frontend.

        Returns:
          A string containing sample data.
        """
        (webgpugrammar,) = grammars
        result = template
        while '<webgpufuzz>' in result:
            result = result.replace(
                '<webgpufuzz>',
                self.generate_function_body(webgpugrammar, _N_MAIN_LINES),
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
        return extract_shader_stages_and_functions(code)

    def collect_shader_entrypoints(self, sources):
        """Formats grammar rules for the entrypoints of loaded shaders.

        Args:
          sources: A list of shader sources.

        Returns:
          A string of <entrypoint> grammar rules.
        """
        return parse_entrypoints(sources)

    def collect_shader_bindings(self, sources):
        """Formats grammar rules for the bindings of loaded shaders.

        Args:
          sources: A list of shader sources.

        Returns:
          A string of <BindInt> grammar rules.
        """
        return parse_bindings(sources)

    def prepare_html_context(self):
        """Creates the element variable bookkeeping for DOM samples.

        WebGPU samples render into a canvas element configured by the
        template, so the bookkeeping tracks that element first.
        """
        return {'canvas_id': 'htmlvar00001', 'element_ids': []}

    def annotate_document_ids(self, html_string, html_context):
        """Inserts stable element ids into a generated DOM tree.

        Args:
          html_string: The generated HTML fragment.
          html_context: The element variable bookkeeping.

        Returns:
          The annotated HTML fragment.
        """
        for match in re.finditer(r'<([a-zA-Z0-9_-]+) ', html_string):
            if match.group(1) not in html_context['element_ids']:
                html_context['element_ids'].append(match.group(1))
        return html_string

    def spawn_document_elements(self, count, html_context):
        """Emits element constructor lines into the bookkeeping."""
        html_context.setdefault('element_lines', [])
        for i in range(count):
            html_context['element_lines'].append(
                '/* htmlvar%05d */' % (i + 1))

    def describe_grammar_stats(self, grammar):
        """Returns a JSON-able summary of a parsed grammar.

        Args:
          grammar: The grammar to describe.

        Returns:
          A dictionary with rule and creator totals, plus the shader
          corpus size this frontend loaded.
        """
        return {
            'root': grammar._root,
            'rules': len(grammar._all_rules),
            'creators': len(grammar._creators),
            'shaders': len(self.shader_sources),
        }

    def supports_language(self, language):
        """Reports whether the frontend emits a given target language.

        Args:
          language: The name of the target language.
        """
        return language in ('wgsl', 'js', 'html')

    def emit_probe_snippet(self, language):
        """Returns a small probe snippet used to smoke-test a frontend.

        Args:
          language: The language the probe snippet is emitted for.

        Returns:
          A string containing a tiny language sample, or None.
        """
        if language == 'wgsl':
            return '@compute @workgroup_size(1) fn probe() {}'
        if language == 'js':
            return 'var probe = navigator.gpu;'
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

    frontend = WebGpuFuzzFrontend(grammar_dir=fuzzer_dir)

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
    else:
        print('Arguments missing')
        print("Usage:")
        print("""--input_dir <directory>. ** not used. **
                --output_dir <directory>. This is the output directory which the fuzzer should write to.
                --no_of_files <n>. This is the number of testcases which the fuzzer should write to the output directory.
                """)

if __name__ == '__main__':
    main()
