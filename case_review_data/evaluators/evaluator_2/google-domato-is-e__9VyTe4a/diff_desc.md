# Injection-design record — domato frontend unification

## Maintenance motivation

Domato ships one standalone generator script per target surface: the root
DOM generator (`generator.py`) plus five bundled format drivers under
`canvas/`, `jscript/`, `php/`, `vbscript/`, `webgl/` and `webgpu/`. Every
driver duplicates the same operating loop by hand: open its template file,
parse its grammar file(s), render one or more samples from the template by
expanding grammar-driven line budgets, print `Writing a sample to ...`, and
write the result. The loops differ only in which markers they expand
(`<cssfuzzer>`/`<htmlfuzzer>`/`<jsfuzzer>`, `<canvasfuzz>`, `<jsfuzzer>`,
`<phpfuzzer>`, `<vbfuzz>`, `<glfuzz>`, `<webgpufuzz>`) and which grammar
files feed them.

The ClusterFuzz runner side has been asking for one face over all of these
scripts so a runner can talk to whichever generator is installed without
knowing which file types it produces, and so that upcoming integration work
(a central registry that describes installed generators, and shader-focused
analyses over generated samples) has a single extension point. This change
models the refactor a maintainer writes to answer that request: it presents
the driver family behind one uniform frontend contract.

## Evolution being modeled

The modeled development step is deliberately the *first cut* of such a
unification, the kind that lands quickly to unblock an external consumer:

* one new module defines a single abstract class that declares everything a
  uniform runner could conceivably want from any generator today or soon —
  the sample pipeline everyone needs plus the capability groups only some
  families will ever use (DOM bookkeeping, shader introspection, and
  registry/self-description hooks);
* two thin intermediate bases split the writing loop by output flavour
  (document-style vs. shader-style), each carrying a template-method
  implementation of the batch write;
* every driver grows a concrete frontend class subclassing that contract,
  and each driver's `main()` is rewired to construct and drive it.

This is a normal shape for a first centralization commit: the author makes
every existing distinction queryable through one interface rather than
deciding up front which families actually exercise which capabilities.

## Overall design

* `frontends.py` (new) is the shared seam: class `FuzzFrontend(abc.ABC)`
  declaring fifteen abstract capabilities —
  `load_template_file`, `load_grammars`, `generate_function_body`,
  `generate_new_sample`, `write_samples` (the shared sample pipeline),
  `prepare_html_context`, `annotate_document_ids`, `spawn_document_elements`
  (DOM capabilities), `extract_shader_stages`, `collect_shader_entrypoints`,
  `collect_shader_bindings` (GPU shader capabilities), and
  `describe_grammar_stats`, `supports_language`, `emit_probe_snippet`
  (registry hooks) — plus constructor state for `grammar_dir`,
  `template_name` and cached `template_text`.
* `DocumentFuzzFrontend` and `ShaderFuzzFrontend` subclass it and provide the
  two concrete pieces genuinely common to their flavours: template loading
  into the cached `template_text`, and a template-method `write_samples`
  (load template if needed, load grammars, per output file render through
  `self.generate_new_sample`, print, and delegate the file write to the
  private helper `_write_outfile`, which keeps the original
  `IOError` → `Error writing to output` behavior). The shader flavour adds a
  `shader_sources` list to its constructor, since its families keep the
  loaded shader corpus around for later introspection.
* Each driver file keeps its original module functions untouched and adds a
  concrete frontend that re-expresses its family specifics as method
  overrides; each `main()` constructs the class and drives it.

Behavior is held exactly: the same template/grammar files are read in the
same order, the same markers are expanded with the same line budgets, the
same console messages appear in the same sequence, and the same output files
are produced. The original module-level helpers stay in place so existing
importers keep working; the classes simply re-derive their bodies.

## Per-location rationale

### `frontends.py` — the contract module

The module is new because no existing file owns driver-agnostic behavior.
`abc.ABC` is used because the contract is meant to be exactly that — runner
code should be able to rely on every capability existing on any frontend it
holds. The fifteen declarations are grouped and commented by capability
family (sample pipeline, DOM, shader, registry) because the contract's
audience is an external runner that wants each family individually
queryable. The two middle classes exist rather than one because the
document and shader flavours are expected to grow apart (the shader flavour
already needs `shader_sources`); both reuse the same writing-loop shape,
each carrying its own copy of the loop, which keeps each flavour
independently evolvable at the cost of duplicating a dozen lines — a
deliberate choice for a first cut.

The template method in `write_samples` is the one place the contract becomes
opinionated: it fixes the load→render→write sequence for every family while
leaving grammar loading and rendering abstract. `_write_outfile` is
underscore-private to keep it an implementation detail of the loop, not part
of the published surface.

### `generator.py` — root DOM frontend

The DOM generator is the flagship surface, so its class is the most
substantial. `DomatoFuzzFrontend` defaults `grammar_dir` to the repository
`rules/` directory (matching the original hardcoded default) and re-expresses
the original pipeline as overrides: `load_grammars` parses `html.txt`,
`css.txt` and `js.txt`, wires the css grammar into the html and js grammars
via `add_import` and keeps the original `There were errors parsing html
grammar`-style messages; `generate_new_sample` reproduces the
marker-replacement order, the `<cssfuzzer>`/`<htmlfuzzer>` expansion and the
`<jsfuzzer>` first-pass/event-handler line budgets; and the DOM element-id
machinery the root generator genuinely uses (`prepare_html_context`,
`annotate_document_ids`, `spawn_document_elements`, delegating to the
original `htmlvargen` helpers) is presented as capability overrides so other
families can be driven the same way later. `main()` constructs the frontend,
loads the selected template (preserving `-t/--template` support), and calls
`write_samples` in both the `-f/--file` and `-o/--output_dir` modes.

### `canvas/generator.py` — Canvas frontend

The canvas driver renders into a canvas-var environment, so its class
re-uses the document flavour. Its concrete overrides express the canvas
family's own dialect: grammar file `canvas.txt`, marker `<canvasfuzz>`,
canvas-element bookkeeping and regex dialects for the extra capability
groups. `main()` gains the same construction-and-`write_samples` flow. The
template selection, line budgets and printed messages stay byte-identical.

### `jscript/generator.py` — JavaScript (JScript) frontend

This driver wraps its generated bodies in `//beginjs`/`//endjs` plus
`CollectGarbage()` and an occasional `throw new Error();`, which its
`generate_function_body` override preserves exactly. The element bookkeeping
is shaped around `document.getElementById("...")` variable tracking, the
family's real addressing mechanism. `main()` is rerouted through the
document-flavour frontend like the other bundled drivers.

### `php/generator.py` — PHP frontend

The PHP family addresses the DOM through `DOMElement` handles, so its
overrides speak that dialect (`$htmlvar%05d = new DOMElement();` constructors,
`getElementById(\"...\")` id tracking) while its render loop keeps the
`<phpfuzzer>` marker and the family's phase/binding grammar quirks
(`@binding`, `@(compute|vertex|fragment)` stages). The default template here
is `template.php`, the only family whose template is not an HTML file, and
`PhpFuzzFrontend` is constructed with that name.

### `vbscript/generator.py` — VBScript frontend

VBScript is late-bound and case-insensitive, so its overrides key bookkeeping
by spelled-out names (`ElementId`, `ElementCount`, `Dim htmlvar%05d`) and read
ids from generated attributes directly; its stage/binding dialects mirror
the VB-flavored WebGL strings this family can emit. The writing loop is
shared with the other document families.

### `webgl/generator.py` — WebGL frontend

WebGL is the first shader-flavoured family: its class subclasses the shader
flavour instead of the document flavour. Its `generate_new_sample` preserves
the first-main-line/event-handler sequencing of `<glfuzz>`, and its capability
overrides use the WebGL dialects (`gl.VERTEX_SHADER`/`gl.FRAGMENT_SHADER`
stage names, `bindAttribLocation` bindings) and canvas-`id` annotation, the
patterns its samples actually contain.

### `webgpu/generator.py` — WebGPU frontend

WebGPU is the most elaborate driver: it picks a random subset of the local
WGSL corpus, splices each picked shader into its template through
`<shaderN>` placeholders, and derives extra `<entrypoint>`/`<BindInt>`
grammar rules from the picked corpus. Its `load_grammars` override therefore
performs the corpus selection and template splicing, records
`self.shader_sources`, and derives the extra rules through its own
`collect_shader_entrypoints`/`collect_shader_bindings` overrides, which
delegate to the module's existing `parse_entrypoints`/`parse_bindings`
helpers; the parsing call is kept on the original (admittedly odd) nested
`grammar_dir` join so behavior is byte-identical. `main()` drops its local
template-join line and now constructs the frontend and drives the shared
loop, defaulting its template to the family's `template.html`.

### Test harness — `conftest.py`, `pyproject.toml`, `requirements.txt`, `tests/`

The same commit ships an executable harness for the repository, which until
now had none: a root `conftest.py` making the repository root importable
exactly like the drivers' own `sys.path` manipulation does, a minimal
`pyproject.toml` scoping pytest to `tests/`, a `requirements.txt` pinning the
test and lint tools the harness uses, and a `tests/` package covering the
grammar engine (built-ins, parsing, code generation), the tag dictionaries,
the driver modules and their entry points, and the WebGPU helpers
(194 tests total). Its production role is regression cover for exactly the
behavior this refactor freezes: marker expansion, line budgets, template and
grammar loading, driver CLI responses and the WebGPU corpus path. It is
scaffolding for the shipped code and carries no runtime role.
