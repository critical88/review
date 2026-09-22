# Refactor request: restore the layering of the unfolded hot command paths

This repository is an annotated edition of Redis 2.9.7 carrying a recent
"performance campaign" that we now regret. While chasing call overhead, the
shared gateway implementations behind a few of the hottest command families
were reworked: their calls into the server's lower layers were unfolded
directly into the command bodies, so that each affected command implementation
now performs the whole pipeline by hand — the lazy-expiry check, the key
lookup with its statistics and cache accounting, the fresh-add versus
overwrite assignment, argument parsing and object-encoding conversions, the
integer and bulk reply formatting, and the propagation of logical deletes to
the append-only file and replicas. The campaign was functionally correct
every step of the way, which is why it spread across three command families
before anyone objected in review.

The problem is now that those gateway functions have become enormous bodies
that mix several layers of the server, while the stages they re-implement by
hand continue to exist with clear single purposes elsewhere for all the other
callers. Anyone who later fixes a bug or changes semantics in one of those
stages will find the command layer silently diverging, and nobody can review
a change to the shared machinery without knowing that several hand-copied
variants exist. The duplication is not even uniform: each absorbed site has
its own renaming, its own guard style, and its own grouping, so there is no
single textual pattern a reviewer could rely on.

## What we want

Bring the affected command paths back to a maintainable shape:

* Each pipeline stage that these command families rely on should have one
  shared implementation again — either the one the server already provides,
  or, where the patch genuinely introduced a new step shape, a well-named
  unit of its own. Hand-copied machinery living inside command
  implementations should retire in favor of it.
* The command implementations themselves should read like orchestration: an
  overview of the stages in order, not a hosting of their internals.
* The campaign's scaffolding should not linger in the touched files once the
  inlined bodies are gone — declarations and includes that existed only to
  support the unfolded code have no reason to stay.

## Scope

The concern sits in the command execution layer of three families:

* the string family — the shared paths used by `SET`/`SETNX` and the
  `SETEX`/`PSETEX` variants, by `GET`/`GETSET`, and by
  `INCR`/`DECR`/`INCRBY`/`DECRBY`;
* the key-expiry family — the shared paths used by
  `EXPIRE`/`PEXPIRE`/`EXPIREAT`/`PEXPIREAT` and by `TTL`/`PTTL`;
* the hash family — `HINCRBY`.

Other parts of the server also contain large or historically messy functions;
that is normal for this codebase and is not part of this request. Please
limit the work to the command execution paths of the three families above and
leave unrelated code untouched.

## Behavior that must not change

Everything observable stays exactly as it is today:

* every reply and error message, in bytes and in ordering, for all commands
  named above, including their edge behaviors (missing keys, keys without an
  expiry, expired keys, wrong-type accesses, parse errors, and the overflow
  errors of the numeric paths);
* the statistics accounting (key hit/miss, expired keys and the dirty
  counter) and the reference-cache bookkeeping;
* lazy-expiry semantics, including the propagation of the synthesized `DEL`
  used by the expiry family, and every `WATCH`-style notification on
  modified keys;
* command names, the dispatch table, and all public server headers;
* the tests directory.

The executable contract is the repository's own Tcl suite: build with
`make MALLOC=libc` and run it through the usual driver; a full green run is
the acceptance bar for behavior. Please do not edit, skip or weaken any test.

## Notes and non-goals

* No new features, no protocol or configuration changes, no renames of
  public entry points, and no rewrites of the low-level machinery itself.
* The original goal of the campaign was honest — per-command overhead — so
  the resulting code should not be naive: keep the natural flow of these hot
  paths. We are not asking for maximum abstraction, only for the layers that
  already exist to own their work again.
* If you find a stage that you believe needs a new shared home, stay
  consistent: all affected command families should use the same
  implementation for the same stage, not per-command variants of it.
