# tsar: module state handling keeps drifting away from the framework

## What we ran into

tsar collects each collector's numbers into a per-module record and then
renders that record in several places: the interactive column layout shown by
`tsar --live` and the merged history view of `tsar --print`, the
`--max`/`--mean`/`--min` aggregate rows over the same history, the machine
readable `tsar --check` line used by monitoring tooling, the nagios/nsca
check pairs produced by the alert integration, and the SQL `INSERT` stream
sent to the database back-end.

At some point we started extracting the per-module part of each of those
views into small file-local helper functions, so the view loops would stay
readable. That worked locally, but looking at the subsystem now the picture is
uncomfortable:

* Each delivery view has its own private copy of the walk that knows how a
  module's collected state is laid out — whether a module is enabled, whether
  its last collection succeeded, how items and columns index into the
  sample arrays, which of the aggregate series applies, and what the column
  headers are.
* When a field on the module record changes, we have to re-read and re-fix
  several output routines that each hold such a block, and fixes landed in one
  view's copy historically did not reach the other views.
* The framework layer — the part of tsar that registers, loads and manages
  modules and folds their samples into that record — is also a natural
  consumer of that state, and its developers end up reasoning about the same
  knowledge that is now scattered inside the delivery code's local helpers.

The upshot is a delivery subsystem that is heavily interested in the
internals of one record, while the interpretation of that record has no
single home.

## What we want

Please investigate the output/delivery side of tsar (interactive console
views, the aggregate tail views, the `--check` payload, the nagios
integration and the SQL output) together with the framework side that owns
module registration and sample collection, and restore a sane ownership
boundary:

* the interpretation of one module's current state — how a sample is read out
  of the record, gated for disabled or uncollected modules and turned into
  the values a view needs — should have one authoritative home with the code
  that owns the module record, so it is consistent across all five views and
  changes with the record rather than with any one view;
* each delivery view should keep exactly what is genuinely its own — its
  layout, formatting and grammar, its separators and buffering, its
  iteration policy — and should obtain the per-module state it renders
  through that shared home instead of carrying its own private record walk.

Address every affected view in one pass; a fix that only relocates one view's
block, or that just renames or un-inlines the helpers in place, does not
resolve the report, nor would deleting functionality. We expect the existing
per-module blocks to stop living as private copies inside the delivery
front-ends.

## Compatibility requirements

* All rendered output stays byte-identical: column layout and separators of
  the live and merged views, the `MAX`/`MEAN`/`MIN` rows, the `--check`
  key=value grammar and its `-` placeholder behavior, the nagios field pairs
  and overall exit classification, and the SQL statement shapes.
* Disabled modules, empty histories, uncollected samples and
  module-flavored special column handling (the `spec`-bit behavior) keep their
  exact current semantics, including the order of the checks.
* Keep the public shape of the codebase: no CLI, config-file, record-layout,
  registered-callback or shared-library ABI changes; the module collectors and
  the framework's own logic should not need to change behavior.
* The build must stay green (`make`) and the behavior checks recorded for this
  repository must keep passing unchanged — this is a restructuring task, not a
  feature change. The practical way to confirm nothing drifted is a rebuild
  plus the recorded checks, with a byte comparison of rendered output from
  the same stored history before and after the change.
