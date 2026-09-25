#   Domato - shared frontend interfaces
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


"""Abstract frontend contracts shared by every generator driver.

Historically every bundled driver (the DOM generator plus the canvas,
jscript, php, vbscript, webgl and webgpu generators) implemented its own
copy of the same operational loop: load a template, load grammar files,
render one or more samples from the template and write them to disk. As
domato started shipping more frontends, the ClusterFuzz side wanted a
single face for all of them so that a runner can interact with whichever
generator is installed without knowing which file types it produces.

``FuzzFrontend`` is that single face. It bundles the sample-writing
pipeline we know every driver needs today with the extra capabilities a
uniform runner may want tomorrow: knowledge about DOM elements for the
frontends that emit documents, knowledge about GPU shader stages for the
frontends that emit shader code, and a few registry hooks used by the
ClusterFuzz fuzzer registry to describe a generator.
"""

from __future__ import print_function

import abc
import os


class FuzzFrontend(abc.ABC):
    """Uniform contract implemented by every domato generator frontend.

    A frontend owns the complete path from template and grammar files to
    written testcase files. The contract covers the shared sample
    pipeline, the document (DOM) capabilities, the shader capabilities
    and the ClusterFuzz registry hooks so that any frontend can be
    driven through one interface.
    """

    def __init__(self, grammar_dir=None, template_name=None):
        self.grammar_dir = grammar_dir
        self.template_name = template_name
        self.template_text = None

    # ------------------------------------------------------------------
    # Shared sample pipeline.
    # ------------------------------------------------------------------

    @abc.abstractmethod
    def load_template_file(self, template_path=None):
        """Loads the template file used to render samples.

        Args:
          template_path: Path of the template file. If None, the path is
              derived from the frontend's grammar directory.

        Returns:
          A string containing the template contents.
        """

    @abc.abstractmethod
    def load_grammars(self):
        """Loads and parses every grammar needed by this frontend.

        Returns:
          A tuple of parsed grammars, or None if parsing failed.
        """

    @abc.abstractmethod
    def generate_function_body(self, grammar, *args, **kwargs):
        """Generates one chunk of grammar-driven code for a sample."""

    @abc.abstractmethod
    def generate_new_sample(self, template, *grammars):
        """Renders a single testcase from a template and grammars."""

    @abc.abstractmethod
    def write_samples(self, outfiles):
        """Renders and writes a batch of testcase files.

        Args:
          outfiles: A list of output filenames.
        """

    # ------------------------------------------------------------------
    # Document (DOM) capabilities.
    # ------------------------------------------------------------------

    @abc.abstractmethod
    def prepare_html_context(self):
        """Creates the element variable bookkeeping for DOM samples."""

    @abc.abstractmethod
    def annotate_document_ids(self, html_string, html_context):
        """Inserts stable element ids into a generated DOM tree.

        Args:
          html_string: The generated HTML fragment.
          html_context: The element variable bookkeeping.

        Returns:
          The annotated HTML fragment.
        """

    @abc.abstractmethod
    def spawn_document_elements(self, count, html_context):
        """Emits element constructor lines into the bookkeeping."""

    # ------------------------------------------------------------------
    # Shader capabilities.
    # ------------------------------------------------------------------

    @abc.abstractmethod
    def extract_shader_stages(self, code):
        """Extracts (stage, function) pairs from a shader source."""

    @abc.abstractmethod
    def collect_shader_entrypoints(self, sources):
        """Formats grammar rules for the entrypoints of loaded shaders."""

    @abc.abstractmethod
    def collect_shader_bindings(self, sources):
        """Formats grammar rules for the bindings of loaded shaders."""

    # ------------------------------------------------------------------
    # ClusterFuzz registry hooks.
    # ------------------------------------------------------------------

    @abc.abstractmethod
    def describe_grammar_stats(self, grammar):
        """Returns a JSON-able summary of a parsed grammar."""

    @abc.abstractmethod
    def supports_language(self, language):
        """Reports whether the frontend emits a given target language."""

    @abc.abstractmethod
    def emit_probe_snippet(self, language):
        """Returns a small probe snippet used to smoke-test a frontend."""


class DocumentFuzzFrontend(FuzzFrontend):
    """Base class for frontends whose samples are DOM documents.

    Provides the template bookkeeping and the sample writing loop that
    is shared by every document-oriented frontend, so concrete
    frontends only provide their grammar and rendering specifics.
    """

    def __init__(self, grammar_dir=None, template_name=None):
        super(DocumentFuzzFrontend, self).__init__(grammar_dir, template_name)

    def load_template_file(self, template_path=None):
        """Loads the frontend's template file into memory.

        Args:
          template_path: Path of the template file. If None, the path is
              derived from the frontend's grammar directory.

        Returns:
          A string containing the template contents.
        """
        path = template_path
        if path is None:
            path = os.path.join(self.grammar_dir, self.template_name)
        f = open(path)
        template = f.read()
        f.close()
        self.template_text = template
        return template

    def write_samples(self, outfiles):
        """Renders and writes a batch of testcase files.

        Args:
          outfiles: A list of output filenames.
        """
        template = self.template_text
        if template is None:
            template = self.load_template_file()

        grammars = self.load_grammars()
        if not grammars:
            return

        for outfile in outfiles:
            result = self.generate_new_sample(template, *grammars)

            if result is not None:
                print('Writing a sample to ' + outfile)
                self._write_outfile(outfile, result)

    def _write_outfile(self, outfile, result):
        """Writes one rendered sample to disk."""
        try:
            f = open(outfile, 'w')
            f.write(result)
            f.close()
        except IOError:
            print('Error writing to output')


class ShaderFuzzFrontend(FuzzFrontend):
    """Base class for frontends that render GPU shader pipelines.

    Mirrors the document frontend's writing loop, and additionally
    keeps track of the shader sources a frontend loaded so that the
    shader analysis capabilities can be queried after generation.
    """

    def __init__(self, grammar_dir=None, template_name=None):
        super(ShaderFuzzFrontend, self).__init__(grammar_dir, template_name)
        self.shader_sources = []

    def load_template_file(self, template_path=None):
        """Loads the frontend's template file into memory.

        Args:
          template_path: Path of the template file. If None, the path is
              derived from the frontend's grammar directory.

        Returns:
          A string containing the template contents.
        """
        path = template_path
        if path is None:
            path = os.path.join(self.grammar_dir, self.template_name)
        f = open(path)
        template = f.read()
        f.close()
        self.template_text = template
        return template

    def write_samples(self, outfiles):
        """Renders and writes a batch of testcase files.

        Args:
          outfiles: A list of output filenames.
        """
        template = self.template_text
        if template is None:
            template = self.load_template_file()

        grammars = self.load_grammars()
        if not grammars:
            return

        for outfile in outfiles:
            result = self.generate_new_sample(template, *grammars)

            if result is not None:
                print('Writing a sample to ' + outfile)
                self._write_outfile(outfile, result)

    def _write_outfile(self, outfile, result):
        """Writes one rendered sample to disk."""
        try:
            f = open(outfile, 'w')
            f.write(result)
            f.close()
        except IOError:
            print('Error writing to output')
