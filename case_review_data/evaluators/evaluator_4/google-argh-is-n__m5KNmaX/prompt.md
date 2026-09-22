# Every command type is being forced through one interface

## What we ran into

argh builds command-line programs out of derived types: a top-level struct,
per-command structs, and an enum that groups the commands of a subcommand
field. While tracing why a plain argument struct now needs to provide command
metadata, we noticed that all of these roles share one interface these days.
Whatever a type actually does, it has to implement that interface in full:
the descriptive entry for the type itself, a table of child commands, and a
hook for commands discovered at runtime.

Because the interface demands everything of everyone, the implementations
are now padded with members that exist only to satisfy it:

- top-level argument structs that recognize no child commands must still
  provide an empty child-command table, an empty runtime-discovery hook, and
  a made-up descriptive entry whose name is just the type's own name;
- a grouping enum must also expose a descriptive entry "for itself" even
  though nothing dispatches through the enum directly;
- boxed subcommand types forward every one of these members even though their
  composition pattern only ever needs the descriptive entry;
- the generic functions that parse the command line (including the
  cargo-oriented one) are declared against this same interface, although
  their bodies only rely on parsing behavior and never touch any of the
  command-metadata members.

## The concern

Capabilities that belong to different roles in the command tree — being a
single named command, exposing a list of child commands, discovering child
commands at runtime — have been folded into one contract. Types now depend on
capability groups they do not use, the derive-generated code carries
placeholder metadata that is never meaningful, and anything generic that
accepts command types transitively depends on the whole command-tree surface.
We would like the command-tree abstraction reorganized so each role depends
only on what its role actually needs.

## Where to look

The command-metadata trait definitions live in the main argh crate. The
code that generates command metadata for derived types lives in the argh
derive crate, which also renders the child-command sections of help output
and of the missing-subcommand message, and generates the dispatch code that
matches child commands by name or short letter. Start from the interface
definition, trace it to the derive-generated implementations, then to every
caller that reads either the descriptive-entry capability or the
child-command capability. Address every instance of the underlying issue
across both crates, not just the first site — the roles, the codegen, and
the rendering paths are all affected.

## Requirements

- A type that plays only one role in the command tree must not be obligated
  to provide the other roles' members or stand-ins for them.
- Generic parsing entry points must not be restricted by command-metadata
  capability groups that their bodies never use.
- The finished design should be consistent: the trait surface, the derive
  expansions, the help and missing-subcommand rendering, and any wrapper
  impls should read as one arrangement rather than a mix of the old and the
  new.

## Behavior that must stay stable

- Command-line parsing behavior, help output, and error messages are
  unchanged, including dynamic (runtime-discovered) subcommands, dispatch by
  command name or single-letter short name, and boxed subcommand fields.
- The public parsing entry points keep their current signatures and
  semantics.
- The repository's own test suite keeps passing with no new failures.
