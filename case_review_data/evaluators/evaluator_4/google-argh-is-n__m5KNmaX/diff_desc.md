# Injection design record — consolidated command metadata interface in argh

This is the design record for the change captured in `smell.diff`. It describes
the motivation, the modeled evolution, the overall design, and the rationale
for each changed location so that the intent of every edit is auditable from
the diff alone. It is design rationale only and does not assert that the
captured diff coheres as a single reorganization or contains no unrelated
work; those are verdicts evaluated separately and are not recorded here.

## Maintenance motivation

argh models a command tree with three role-specific abstractions: a marker for
the top-level program type, a contract for a type that *groups* several child
commands (the subcommand enum), and a contract for a *single* child command.
Each one carries only the capability its role needs, and the derive macro
decides which contract to implement per derived type.

A very common maintenance move on a surface like this is *consolidation*: the
three contracts sit next to each other, always describe the same metadata type
(`CommandInfo`), and every derived type ends up implementing one of them, so
"one unified interface for everything that parses arguments" looks like a
simplification. It removes the need for the derive to choose a contract per
kind, lets generic helper code accept any command type unconditionally, and
reads well in the trait-level documentation: one place to look for command
metadata.

## Normal development evolution modeled

This change models that consolidation refactor: a developer merges a family of
closely related role traits into a single "fully specified" interface during
API cleanup, re-points every implementation and consumer at the unified
surface, and pads out the roles that cannot naturally provide all of the
unified members. No new features are introduced; the reorganization is purely
about the shape of the command-metadata surface.

## Overall design

- **Runtime crate (`argh`)**: the three role contracts are replaced by one
  trait, `CommandSpec: FromArgs`, which declares together: `COMMANDS` (the
  static list of child commands), `COMMAND` (the command's own descriptive
  entry), and `dynamic_commands()` (runtime-discovered child commands). A
  blanket impl forwards the whole interface for boxed command types (`Box<T>`),
  preserving how optional subcommand fields parse today. The generic entry
  points (`try_from_env`, `from_env`, `cargo_from_env`) are re-bounded from
  the top-level marker to the unified interface, since the developer doing the
  consolidation wants "any command type" to be accepted generically.
- **Derive crate (`argh_derive`)**: because the unified interface now has no
  role-specific shape and no default-provided members, the derive must emit a
  complete implementation for *every* derived type:
  - top-level argument structs (which recognize no child commands and have no
    per-command descriptive entry today) receive a minimal identity entry whose
    name is the type's own name, an empty `COMMANDS` list, and an empty
    `dynamic_commands()` body;
  - single-subcommand structs keep their real `COMMAND` and re-declare
    `COMMANDS` as `&[Self::COMMAND]` so grouped dispatch still works, plus the
    empty dynamic list;
  - subcommand-grouping enums list their variants' entries in `COMMANDS`,
    forward a `COMMAND` from their first variant (or a placeholder entry for a
    variant-less enum), and gain an explicit `dynamic_commands()` fallback of
    `&[]` when no `#[argh(dynamic)]` variant exists;
  - every call site that reads command metadata in generated code and in the
    help renderer (parsing, redaction, missing-subcommand reporting, help
    child-command listing) is re-pointed from the role contracts to the
    unified interface.

## Per-location rationale

### `argh/src/lib.rs` — trait surface (replaced block)

**What changed**: the `TopLevelCommand` marker, the `SubCommands`
static list + runtime discovery contract (which previously carried a default
empty discovery body), the `SubCommand` descriptive-entry contract, and the
two small blanket impls (`SubCommands for T` composing a grouped listing from
one entry, `SubCommand for Box<T>`) are collapsed into one `CommandSpec`
trait plus a single forwarding impl for `Box<T>`.

**Why here / this form**: this block is where the command-tree roles are
*defined*; consolidation has to happen at the definition site, and the
`Box<T>` impl replaces the old composition blanket so that optional
subcommand fields composed of a single command type still list exactly one
child command. The impl forwards all three members — including the
capability groups a boxed single command never uses — which is a direct,
visible consequence of requiring every role to satisfy the unified surface.

**Production role**: the public capability surface every derived type
implements and every child-command consumer reads.

### `argh/src/lib.rs` — parsing entry points (`try_from_env`, `from_env`, `cargo_from_env`)

**What changed**: bounds changed from the top-level marker to the unified
interface. Bodies untouched.

**Why here**: generic entry points are the natural place a consolidating
developer wants "accepts any command type" behavior, but these functions only
parse arguments and never read any command-metadata member, so the new bound
drags the full merged capability surface into signatures that do not use it.

**Production role**: the program-facing API for building a command from the
real command line (and from cargo-style arguments).

### `argh_derive/src/lib.rs` — `top_or_sub_cmd_impl`

**What changed**: this function used to choose *which* role contract to
implement per derived struct (empty marker for non-subcommands, descriptive
entry for subcommands). It now always emits a complete `CommandSpec`
implementation: for non-subcommand types it synthesizes a plausible identity
entry (name = the type's own name, the previous description text) plus empty
child-command and discovery members; for subcommand types it emits the real
entry plus `&[Self::COMMAND]` and the empty discovery member.

**Why here / this form**: this is the single decision point for what a derived
struct implements, so it carries the whole "no more per-role choice" policy of
the consolidation. The synthesized members have realistic content rather than
dummy text so the generated code remains valid and the file-to-help behavior
for subcommands is unchanged; for the types that never had a
`COMMAND`, the synthesized entry is never read by any pre-existing runtime
path.

**Production role**: decides the generated trait surface of every
`#[derive(FromArgs)]` struct.

### `argh_derive/src/lib.rs` — `impl_from_args_enum`

**What changed**: the child-command-grouping impl for enums now implements the
unified interface: `COMMANDS` from the variants' entries (unchanged shape), a
forwarded `COMMAND` from the first variant (with a placeholder fallback for an
enum with no variants, which previously had no identity requirement at all),
and an explicit `dynamic_commands()` fallback when no dynamic variant exists
(necessary because the unified trait provides no default).

**Why here**: the enum expansion is the second half of the derive-side policy —
grouping types now also carry identity/discovery capability stand-ins. The
variant-matching code itself (name/short dispatch in `from_args` and
`redact_arg_values`) is re-pointed to read each variant's entry through the
unified interface.

**Production role**: generates child-command grouping and variant dispatch
for enums.

### `argh_derive/src/lib.rs` — `impl_from_args_struct_from_args`, `impl_from_args_struct_redact_arg_values`, `append_missing_requirements`

**What changed**: the generated parse and redaction code for a struct with a
subcommand field, and the missing-subcommand requirement reporting, now obtain
the child-command lists (`COMMANDS` + `dynamic_commands()`) through the
unified interface instead of the grouping contract.

**Why here**: these are *consumers* of the collection/discovery capability;
consolidation re-points them mechanically. No generated token content changes
beyond the trait path, so parsing, redaction, and the missing-commands message
compose identically.

**Production role**: struct-level subcommand parsing, argument redaction, and
the "missing required subcommand" diagnostic listing.

### `argh_derive/src/help.rs` — `help`

**What changed**: the child-command section of `--help` now iterates
`COMMANDS` and `dynamic_commands()` through the unified interface.

**Why here**: with the merge, help rendering joins the other collection
consumers reading through the one surface; the produced help text (format
string and ordering) is untouched.

**Production role**: renders the "Commands:" section of the help text.

## Behavior shape of the merged members

The synthesized members exist to satisfy the unified requirement, not to be
read: top-level types never had their identity entry consulted, grouping types
never dispatch through their own forwarded entry, and the dynamic lists are
empty exactly where they were empty before. All previously meaningful reads
(variant dispatch, help listing, boxed composition, dynamic discovery) resolve
to the same values as before the change.
