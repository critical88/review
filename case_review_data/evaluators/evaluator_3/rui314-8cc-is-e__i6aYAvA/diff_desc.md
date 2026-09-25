## Maintenance motivation

8cc reads source text from two different character sources (a `FILE *` and an
in-memory string) and writes text to two different places (the generated
assembly file and the compiler's diagnostics on stderr). Each of those four
backends had, historically, its own ad-hoc access pattern: `file.c` branched on
`f->file != NULL` to decide between `readc_file` and `readc_string`, `gen.c`
kept a static `FILE *outputfp` and `fprintf`-ed to it directly, and `error.c`
`fprintf`-ed to `stderr` directly. A maintainer setting out to add a third
character source (and to keep a single, testable place for I/O tracing and
future buffering) would want one polymorphic handle that can stand in for any
of those backends.

## Normal evolution being modeled

The change models the common sequence where a developer first introduces one
"unified" abstraction that looks reasonable -- a single stream type carrying a
function-pointer table -- and routes every existing backend through it, so the
abstraction has to cover the union of capabilities any backend needs. It is kept
small and self-consistent (every file compiles, every test passes), which is
exactly when the cost of the shape is hardest to see: each backend now
completes a table whose shape was dictated by the union of all backends, not by
its own needs.

## Overall design

A new `stream.h` defines `struct StreamOps`, a function-pointer table with one
slot per capability any I/O backend might expose: reading (`read`, `unread`,
`eof`), writing (`emitv`, `putstr`, `flush`), positioning (`tell`, `seek`), and
lifecycle (`close`). The header also provides small `static inline` no-op
implementations for every slot, so a backend can complete the table for the
capabilities it does not have without writing new code each time. `8cc.h` adds a
minimal `struct Stream { const StreamOps *ops; }` and forward-declares the ops
type, and `File` embeds `Stream` as its first member so a `File *` is usable
wherever a `Stream *` is expected. Each of the three backends obtains its own
`StreamOps` instance and dispatches through `ops` instead of branching or
calling the backend function directly.

## Per-cluster rationale

### `stream.h` -- the stream abstraction (new)

Holds the single `struct StreamOps` definition and the shared no-op stubs. It is
a new private header included only by the three backend `.c` files that own a
concrete stream, which keeps the self-bootstrapped compiler (which parses
`8cc.h` but never this header) unaffected. The production role is to be the one
place that describes "what a stream can do"; a maintainer adding a new backend
includes it and fills in a table.

### `8cc.h` -- the polymorphic handle

Adds the incomplete `StreamOps` forward declaration, a `struct Stream` carrying
just the `ops` pointer, and embeds `Stream` as the first field of `File`. The
review/ratio is intentionally minimal: only the type needed to pass a `Stream *`
around lives here, so units compiled by 8cc itself continue to parse cleanly.
Production role: a shared handle type the rest of the compiler can hold
opaquely.

### `file.c` -- character input (two backends)

Both input flavors are rebuilt as StreamOps instances: `file_read`/`string_read`
take the place of the old `readc_file`/`readc_string`, `stream_unread` holds the
shared push-back logic, `file_eof`/`string_eof` report end-of-source, and
`file_close` releases the `FILE *`. `make_file` and `make_file_string` install
the matching table. `get`, `readc`, and `unreadc` dispatch through `ops` rather
than branching on `f->file`. The two input sources do not write or seek, so
their tables complete the output and positioning slots with the shared no-ops.
Production role: the only character source feeding the lexer.

### `gen.c` -- assembly output (owned sink)

The static `FILE *outputfp` becomes a static `OutStream` (a `Stream` plus the
`FILE *`), with `output_emitv`/`output_putstr`/`output_flush`/`output_close`
wrapping the existing `vfprintf`/`fputs`/`fflush`/`fclose` calls, and
`emitf`/`emit_nostack`/`set_output_file`/`close_output_file` dispatch through
`ops`. The emitter never reads from its output file, so the input and
positioning slots are filled with no-ops. Byte output is unchanged: the writes
still go through the same libc calls on the same file. Production role: the one
text sink the code generator writes to.

### `error.c` -- diagnostics (unowned sink)

`print_error` is rerouted through a static `OutStream` bound to `stderr`, with
`err_emitv`/`err_putstr`/`err_flush` wrapping the existing `vfprintf`/`fputs`/
`fflush` to stderr. A constructor sets the sink's file pointer to `stderr`.
`stderr` is a process-owned file the reporter must not close, and the reporter
has no input or positioning behavior, so the input, positioning and close slots
are filled with no-ops. Diagnostic output (including the color and prefix
formatting checked by the negative-test suite) is unchanged, because every
write still resolves to the same libc calls on `stderr`. Production role: the
reporter for compile-time errors and warnings.

## Notes on the shape

The convenience of completing an unfamiliar table with shared no-op stubs is
what makes the design cheap to write; it is also what makes each backend's table
describe capabilities that backend does not have. The two positioning slots are
filled with no-ops by all four backends -- no source or sink in the compiler
needs to tell or seek on its stream.
