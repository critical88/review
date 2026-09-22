# Injection design record — tsar module-state output maintenance

## Realistic maintenance motivation

tsar's interactive front-end (`tsar`, `tsar --print`, `tsar --check`) and its
delivery back-ends (nagios/nsca, SQL) all serialize the same thing: the
per-module sample record that the framework layer collects into
`struct module` (`include/framework.h`). The record itself has kept growing
across releases — `spec`, `st_flag`, `p_item`, `print_item`, the
`max_array`/`mean_array`/`min_array` aggregation series and the `info` column
headers were all added while the front-ends accumulated rendering rules that
know how those fields interact.

The realistic problem this models: whenever the framework reshapes the module
record, every output component has to be re-read and re-fixed, because the
knowledge of "which module fields make up a printable sample" is spread inside
the per-view loops of each front-end. Between releases developers repeatedly
extracted the per-module walks out of those loops into small helpers so the
loops would stay readable — and, because the helpers are only used by one view
each, they were placed as file-local (`static`) routines inside the front-end
that needed them. That is a very ordinary C evolution: extract a helper, keep it
`static` next to its only caller, ship it.

## Normal development evolution being modeled

The change is a single coherent maintenance step across the output
subsystem, of the kind a maintainer produces when tidying up the front-ends
before a release:

* the long per-view loops over `statis.total_mod_num` previously mixed three
  concerns: module-list iteration, the per-module walk over the record and the
  concrete output-grammar syntax (printf formats, SQL fragments, nsca pairs);
* this step factors the per-module walk of each view into one small helper
  function taking the module record it walks, and leaves the surrounding loop
  with the iteration/aggregation policy only;
* helper definitions sit above their only caller in each front-end file, marked
  `static`, with a comment explaining the walk they own.

No public interface changes: `struct module` and `struct mod_info` keep their
layout, every registered callback keeps its signature, the CLI surface and the
on-disk formats of all five outputs keep their exact bytes.

## Overall design

Five helpers were extracted, grouped by the output view that uses them:

| front-end | helper | production role |
| --- | --- | --- |
| `src/output_print.c` | `print_module_row` | one module's live/merge summary row |
| `src/output_print.c` | `print_module_tail` | one module's `--max`/`--mean`/`--min` row |
| `src/output_print.c` | `print_module_check_state` | one module's `tsar --check` key=value payload |
| `src/output_nagios.c` | `module_state_to_nagios` | one module's nagios/nsca check payload |
| `src/output_db.c` | `module_state_to_sql` | one module's SQL `INSERT` statement |

Each helper receives the record it walks (`struct module *mod`), plus, where
the view needs it, the destination buffers or session strings the caller
already owned (`sqls` for the accumulated SQL stream, `output`/`output_err`/
`result` for the nagios payloads, `host_name`/`s_time` for the SQL values).
The helpers are deliberately walk-shaped: resolve what the record says
(`enable`, `st_flag`, `spec`, `n_item`, `n_col`, `info` headers), locate the
module's sample window (`st_array` window base, aggregation tails) and emit the
view's syntax for exactly that. Loops that used to carry this logic now read as
`for (i = 0; i < statis.total_mod_num; i++) <helper>(mods[i], ...);`.

## Per-cluster rationale

### `src/output_print.c` — interactive and check views

**`print_module_row`.** Extracted from `print_record`. The live/merge row walk
was the longest-standing piece of per-module knowledge in the console view:
skip disabled modules, honor an item selector (`print_item` matched against
`p_item`), pick the sample window (`st_array` indexed with the `n_col` stride)
or ask `print_array_stat` for the empty layout when `n_item` is zero, and keep
the section separator when multiple items were printed. Because the row depends
on how the framework lays the samples out — not on how a console prints them —
this is the knowledge most likely to drift when the record changes; keeping it
readable as one unit next to its only caller was the ordinary local choice.

**`print_module_tail`.** Extracted from `print_tail`. The aggregation view
duplicates nearly all of the row walk but selects one of the three derived
series (`max_array`, `mean_array`, `min_array`) that the statistics folding
maintains, applies the same item selection, and prints either the per-column
values filtered by `spec`/`info[i].summary_bit` against `conf.print_mode`, or
the empty placeholder via `print_array_stat(mod, NULL)`. The per-module part
of this — the series switch plus the column filter against the column
descriptor — is the record knowledge; only the `MAX`/`MEAN`/`MIN` header line
stays in the calling loop.

**`print_module_check_state`.** Extracted from the `RUN_CHECK_NEW` branch of
`running_check`. This is the machine-consumable view: it derives the displayed
module name from `opt_line`, replays the blank-separated `record` string so
multi-item modules get their per-item prefix, walks `n_item` x `n_col` windows
of `st_array`, honors collection success (`st_flag`) and special modules
(`spec` with `SPEC_BIT` column labeling), and formats each `name:prefix:col`
triple with the trimmed `info` header. It is the module-to-nagios/payroll/tooling
wire format; the walk was worth naming and keeping as one unit next to the
branch that calls it.

### `src/output_nagios.c` — alert delivery view

**`module_state_to_nagios`.** Extracted from the per-module branch of
`output_nagios`. The nagios payload is decided by the record: the check name
is the module name (stripped of the `mod_` prefix by `name + 4`), the item
labels come from replaying `record`, the value window from `st_array` with the
`n_col` stride, and for each matched `check_item` the cmin/cmax/wmin/wmax
threshold rows decide `*result` and the `output_err` pair, falling back to the
`name %s\n do nothing` announcement when the module reported nothing
(`st_flag`) and skipping disabled modules. The helper keeps the original guard
order and the original string-building sequence, and its loops receive the
already-owned static buffers (`output`, `output_err`) from the caller.

### `src/output_db.c` — database delivery view

**`module_state_to_sql`.** Extracted from `send_sql_txt`. The SQL row is the
most literal serialization of the record: the table name comes from
`opt_line + 2`, the column list is rendered from the `mod_info *info` headers
with `n_col`, and the value tuple is one `%.1f` per `st_array` slot between
braces — or the header-only INSERT when the module reported nothing
(`st_flag`), and disabled modules contribute nothing. Inline in
`send_sql_txt` this was a 60-line block that had to be re-read whenever
either the SQL escaping rules or the module record changed; as a helper it is
a named unit with the module record as the only structured argument.

## Boundary and compatibility

* The five helpers are file-local to the front-end that calls them; no header
  changed, no symbol became visible to other translation units.
* Every call site passes the same arguments the inlined code used, in the same
  order, with the same gating conditions evaluated in the same sequence.
* Iteration order, output-grammar literals (`PRINT_SEC_SPLIT`, the `-` value
  placeholder, the nsca field order, the SQL statement shape) and the
  disabled/empty-state fallbacks are preserved statement by statement.
* Nothing outside the three delivery front-ends changed: framework, record
  definition, module registry, lua glue and the module collectors are
  untouched.

Design tradeoff accepted: placing these helpers inside the delivery front-ends
keeps each view self-contained and the diff to a single subsystem, which is
what a developer tidying up the front-ends would naturally do. The alternative
shapes for such helpers existing at all (e.g., export them through a header,
or hold them where the module record and its lifecycle are managed) were not
taken here; they involve choosing a shared home for all five views at once and
were outside the scope of this step in the history being modeled.
