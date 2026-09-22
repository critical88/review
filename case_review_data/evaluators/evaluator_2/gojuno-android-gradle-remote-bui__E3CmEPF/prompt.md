# Every remote operation now has to answer for everything

## What I ran into

This tool pushes a project to a remote machine, executes a command there,
pulls the result back, and reports timings. After the recent rework of that
flow, every remote operation is expressed through one shared "remote
operation" model. I was reviewing a configuration change last week and had
to give up halfway: every operation type now provides an answer for
everything the model can ask about — where the remote project lives, which
files to exclude for each transfer direction, compression levels for each
direction, which pull pacing applies, the shell snippet used to run things
remotely — regardless of what the operation actually does. The push
operation answers pull questions, the pull operation answers push
questions, and the operation that runs a command over ssh answers rsync
questions although it never moves a file. The small driver helpers that run
an operation through the model only ever use the start-and-report part of
it, but because the model is the only way to consume an operation,
everything else rides along in scope.

It is more than a reading annoyance: I wanted to adjust how one side of the
file transfer tunes its compression and could not tell which of these
answers are actually consulted at runtime without reading all three
operations at once. New operation kinds would have to start life by
answering questions they cannot meaningfully answer.

## What I want

Restructure the remote-operation model so that dependencies reflect real
use:

- each concrete remote operation should expose only the capability surface
  its execution actually consumes — its direction of transfer and its
  exclusion/compression behavior, or its command behavior;
- whatever is genuinely shared across the operations (the basic
  start/consolidate lifecycle) may stay shared, but consuming it should not
  drag per-direction or per-role capability into scope that the consumer
  never uses;
- the restructure must cover the whole flow — both file transfer
  directions (including the variant where a pull waits for the command to
  finish and the variant where it polls alongside it) and the remote
  command execution — not just the most visible operation;
- if parts of the current model (the broad capability answers, the
  category markers, helper functions) exist only to keep the wide shape
  working, remove them instead of leaving them behind as dead answers.

## What must not change

This is an internal restructuring; nothing user-visible should move:

- console output lines, the push → execute → pull ordering, and the exit
  codes for missing arguments, missing/invalid configuration, and failed
  push/execute/pull outcomes;
- how rsync is invoked per direction: argument content and order, project
  directory creation on the remote machine, ssh usage, compression levels,
  and which exclude files apply to which direction;
- the ssh command transport: change into the remote project directory, echo
  the command and an empty line, run it through bash on the remote host,
  and keep broadcasting its result to the readers that are created before
  the command starts;
- serial pulls wait for the command result before pulling; parallel pulls
  keep their pause, their polling loop and the final consistency pull;
  perceived pull durations still subtract the command duration;
- the existing unit tests keep passing unmodified.

Please investigate the affected subsystem end to end before changing code:
the operation model itself, the push/pull implementation, and the remote
command implementation all take part, and any of them left in the old shape
leaves the problem in place.
