# Injection design record — gojuno/android-gradle-remote-build (mainframer)

## Maintenance motivation

Mainframer runs three remote operations in a fixed order: push the project to
the remote machine, execute a command there, and pull the result back. The
original implementation expressed these as three untyped function groups:
`push` (rsync local→remote), `pull` with serial/parallel variants (rsync
remote→local), and `execute_remote_command` (ssh). Each group duplicated the
same shape of work with slight variations — take a snapshot of configuration,
assemble an `rsync`/`ssh` argv, measure the duration, report the outcome.

A maintainer wants to unify this recurring shape so every remote operation
follows the same template (`prepare` the operation, `run` it in the
background, `finish` by consolidating the outcome), so that new operation
kinds can slot into the same flow, and so the push, pull and command code can
stop repeating timing and plumbing they share. This is the change recorded
here: a remote operation model introduced in one module, the three operation
groups recast as implementor types, and a small set of driver functions that
run any operation through the template.

## The development evolution being modeled

This change mirrors a common way a data-oriented codebase grows a contract
layer:

1. A lifecycle abstraction (`prepare` / `run` / `finish`) is introduced with a
   completion channel and outcome record so an operation can complete in the
   background, which the pull path needs anyway.
2. The template is extended with every question an operation *might* be asked
   during the unification, rather than the questions any one operation is
   actually asked: where the remote project lives, how to exclude files in
   either direction, compression for either direction, pull mode, the shell
   snippet to run remotely, and the human-readable phase label.
3. The three existing groups are mechanically fitted to that single contract.
   To keep the fitting easy, members with an obvious answer for a given
   operation are answered from existing data rather than from the operation's
   own role — every operation knows the config, so every operation can answer
   push compression, pull compression and pull mode from it; the project
   directory layout is centralized already, the remote command runner even
   stores the resolved directory for its snippet.
4. Marker traits (`FileTransferOperation`, `CommandOperation`) are added as a
   coarse way to talk about subcategories, while the driver functions still
   route through the wide contract the markers elaborate.

## Overall design

- `src/operation.rs` (new module): the contract layer. `RemoteOperation` names
  the whole remote-operation surface (lifecycle, host and directory
  knowledge, transfer-direction tuning, command synthesis, phase label).
  `OperationSignal`/`OperationOutcome` carry a started operation's
  completion. Marker subtraits narrow the contract into file transfers and
  remote commands. Four driver functions run operations through the template:
  run a transfer to completion, start a transfer without waiting, start a
  remote command, and consolidate a previously started operation (through a
  trait object, to allow erasure at the sync boundary).
- `src/sync.rs`: push and pull reworked as `PushOperation` and
  `PullOperation` implementing `RemoteOperation`. The serial/parallel pull
  variants become methods that hand back a pending `OperationSignal` instead
  of an anonymous channel pair. The public `push`/`pull` functions keep their
  signatures and construct the operation types. Direction-specific questions
  are answered from config for both operations alike.
- `src/remote_command.rs`: the command path reworked as
  `RemoteCommandOperation`, holding the bus and its readers so they are
  created exactly as before, and implementing `RemoteOperation` for
  template conformance. The public `execute_remote_command` keeps its
  signature and drives the operation through a starter.
- `src/main.rs`: only registers the new module; the orchestration order,
  console output, and exit-code behavior are untouched.

## Per-location rationale

### `src/operation.rs` — new contract module

A dedicated module keeps the contract, the completion plumbing and the
driver functions in one place; the sync and command modules then depend on a
single, small import surface. The lifecycle trio was chosen because push and
pull genuinely differ in when consolidation happens (push waits inside its
own call, pull consolidates later from another thread), and centralizing the
signal/outcome types expresses that difference once. The wider members —
exclude arguments per direction, per-direction compression, pull mode, remote
command string, host, directory, phase label — were collected here because
during unification they were the questions that any operation *could* be
asked once everything is "a remote operation", and because they let the
rsync/ssh builders in the implementors be written uniformly. The marker
subtraits give the two operation families a name for bounds without carving
up the contract; the driver functions bound by them inherit the full surface
elaborated through the markers. `complete` takes a trait object because the
pull path consolidates on another thread after the concrete type has already
been moved out of scope.

### `src/sync.rs` — the two transfer operations

Push and pull are the natural implementors: they are the parts of the tool
whose shape differed only in direction and pacing. Extracting them as types
lets the bus-reader handoff between command completion and the pull thread be
expressed as operation state instead of a stray channel argument threaded
through functions. Each operation answers the whole contract: the transfer
direction it does not perform is answered with the config's values, and the
remote command string is answered with the plain rsync string the operation
would need if it were asked to run itself remotely. The serial pull keeps its
wait-then-pull thread; the parallel pull keeps the pause, the loop, the final
consistency pull and the perceived-duration calculation, so local observation
of a pull matches the previous behavior, and `RemoteOperation::run` picks
between variants from the pull mode exactly like the old call site did.

### `src/remote_command.rs` — the command operation

The command path becomes `RemoteCommandOperation` so the bus and reader
creation (one bus, as many readers as requested, created up front so a
parallel pull can observe the command while it runs) is owned by a single
object, instead of the bus being an incidental local of a free function. The
operation answers the contract's rsync questions with declarations that no
exclusion applies and with the config's per-direction values — the command
runner runs no rsync in either direction — and answers the remote command
string with the existing `ssh <host> "… | bash"` snippet, unchanged in
shape. Its `run` never produces a consolidated outcome through its signal:
the result arrives through the bus, so the started signal is released
immediately.

### `src/main.rs` — module registration

The top-level flow stays as it was: the same push → execute → pull sequence,
the same printed lines, the same failure exit code. The only change is
registering the new module, since push, pull and remote command are consumed
through unchanged entry points.
