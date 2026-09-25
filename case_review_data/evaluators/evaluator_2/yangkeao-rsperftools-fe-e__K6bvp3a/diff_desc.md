# Design Record for the Repository Change

## Maintenance Motivation

The profiler can only report what the sampling layer records, and the report
layer can only present what the profiler hands it. Between capture and export,
one raw sample travels through three modules in this repository:

1. the collection layer captures a backtrace plus the thread identity of the
   interrupted thread and stores it verbatim, because the sampling signal
   handler runs in a context where symbolication work must be deferred;
2. the report layer later resolves those raw frames (symbol names, source
   files, line numbers), groups samples, and renders them for humans
   (debug text), for flamegraph tooling, and for the pprof consumers;
3. the timing domain records when the profiling window opened and how long
   it ran, and the report layer attaches that metadata to the finished
   report.

Both sides of each boundary evolve independently: the capture side gains new
unwinders and new perfmap sources, while the presentation side gains new
export formats. Every time a report-facing feature needed "one more piece of
the sample" or a slightly different rendering decision, the temptation in this
codebase's history is to implement the detail where the demand appeared — in
the report or profiler module — by reading the recorded data's fields
directly, instead of asking the module that defines the sample type to own
that decision.

## The Evolution Modeled Here

This change models a series of maintenance episodes in which report-facing and
profiler-facing features were implemented at the point of demand, with the
data owner progressively demoted to a passive record type:

- **Sample assembly at the capture site.** A change to the sampling path that
  wanted to control exactly how a captured backtrace, thread name, thread id
  and timestamp become one stored sample stopped using the sample type's
  constructor and started assembling the record inline (its fixed-size thread
  name buffer, length slice bookkeeping and all) in the collection module,
  where the data arrives. What used to be one owning constructor call became
  open knowledge of the layout at the call site.
- **Timing metadata taken over by the guard.** A report-metadata tweak
  removed the timing domain's one operation that produced the elapsed-time
  record from its own private fields (the timer's frequency, wall-clock start
  time and monotonic start). The guard, which merely holds the timer, now
  reads those fields itself and assembles the metadata record.
- **Resolution moved into the report builder.** A report-building change
  moved the raw-to-resolved sample conversion — perfmap lookup, symbol
  resolution, the signal-handler frame filtering rule, and the thread name
  decoding from the fixed-size buffer — out of the sampling model and into a
  report-layer helper that walks the record's fields directly. The
  perfmap-lookup helper the conversion needs had to become crate-visible so
  the report module could call it; the model's convenience imports it no
  longer used were dropped, which also forced its equality and hash
  implementations to spell out the frame type's trait explicitly.
- **Human-readable rendering re-homed with the other report outputs.** The
  debug text of a resolved sample — frame sections and the thread name or
  numeric thread id — moved from the model's home module to the report
  module, next to the report type's own debug output. Rendering now reads the
  resolved sample's fields from the outside.
- **Flamegraph formatting extracted as a report-layer helper.** An
  inferno-options refactor extracted the collapsed-stack line building into
  a report-layer function that reads the sample's thread identity fields and
  frame list directly, instead of asking the model for the thread identifier
  it used to expose.
- **pprof export flattening its own symbol handling.** Supporting a
  demangling change in the protobuf export stopped calling the symbol's own
  accessors (demangled name, system name, filename, line number) and started
  reading the symbol record's raw byte-buffer name and optional path/line
  fields directly inside the export function — demangling, lossy UTF-8
  conversion and the "Unknown" fallbacks duplicated at each of the several
  call points of that export's string tables. A copy of the thread-identity
  rule (name preferred over numeric id, with the empty-name fallback) was
  also made privately for the pprof export path.

The overall shape after these episodes: the modules that define the sample
data types and the timer still own the layout, but the decisions about what
that data means — how a raw capture becomes a reportable sample, how a sample
is spelled for humans, for flamegraph or for pprof, how a symbol is named,
and how long the run took — now live in the modules that demand the data
rather than the modules that own it.

## Materially Changed Locations

### Collection module (`src/profiler.rs`)

- **Sample storage call site (`sample`).** No longer constructs the stored
  record through its type's constructor; reads the raw thread-name bytes,
  computes the filled length, fills a fixed-size buffer, and assembles the
  record field by field inline.
- **Timing metadata (`report` path of `ProfilerGuard`, new `timing_of`).** The
  guard's report handoff now derives the elapsed-time metadata by reading the
  timer's frequency, wall-clock start time and monotonic start instant,
  instead of asking the timer for the record. A new private helper performs
  this read-and-assemble; the guard itself only delegates.

### Sampling model (`src/frames.rs`)

- The record constructor was removed: the fixed-size thread-name buffer logic
  now has no owner-side entry point.
- The raw-to-resolved conversion (with the perfmap lookup, symbol resolution
  and signal-handler frame skip) moved out of this module; the perfmap lookup
  helper it shared became crate-visible for the report module's use.
- The thread-identity helper (thread name if set, numeric id otherwise) was
  removed from the resolved sample's impl block.
- The human-readable debug rendering of the resolved sample moved out of this
  module.
- The equality and hash implementations still work the same but now have to
  name the frame trait explicitly, since the convenience import that the
  removed impls justified was dropped. This is a mechanical consequence of
  the import cleanup, in the same module, touching no behavior.

### Report layer (`src/report.rs`)

- **New resolution helper.** The conversion of one stored sample into its
  resolved presentation (perfmap lookup, symbol resolution, mac-underscore
  handler frame filtering, thread name decoding from the fixed-size buffer)
  is now a private helper here, walking the stored record's fields directly.
  The builder's report construction calls the helper instead of a conversion
  the model used to provide.
- **New debug rendering of the resolved sample.** Moved here next to the
  report type's own debug output; reads the resolved sample's frame list and
  thread identity fields from the outside.
- **New flamegraph line helper (`flamegraph` feature).** Extracted from the
  svg export; reads the sample's thread identity fields and frame list to
  build the collapsed-stack line.
- **New thread-identity helper and inlined symbol handling (protobuf
  feature).** The pprof export now has its own copy of the
  thread-name-or-id rule, and reads each symbol's raw name buffer and
  optional filename/line fields directly — demangling, lossy UTF-8 decoding
  and the "Unknown" fallbacks implemented at the export's several
  string-table call points instead of via the symbol's own accessors.

### Timing domain (`src/timer.rs`)

- The operation that turned the timer's private fields into the report
  metadata record (frequency, wall-clock start time, elapsed monotonic
  duration) was removed from the timer. The timer now only starts and stops
  the interval timer; whoever holds it must read its fields to build timing
  metadata.
