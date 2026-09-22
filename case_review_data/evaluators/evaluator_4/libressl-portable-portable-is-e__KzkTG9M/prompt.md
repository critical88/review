# Non-blocking readiness support in the BIO layer

## How this repository carries changes

LibreSSL portable does not keep its C sources in git. `./update.sh` materializes
`crypto/`, `ssl/`, `tls/` and `include/` from the upstream snapshot in `openbsd/`
and then applies, in filename order, the local patch series under `patches/`.
The materialized tree is regenerated output: it exists to read, build and test
against, but anything edited directly under those directories is lost the next
time `./update.sh` runs. A lasting change to the library is carried by patch
files in that series. The usual verification cycle is re-running the
materialization, rebuilding the cmake build tree and the ctest suite.

## Maintainer observation

A while back we added non-blocking readiness support to the BIO layer so that
applications could drive any BIO without blocking: an entry point to switch the
underlying resource to non-blocking mode, and two entry points that wait until
a BIO can be read or written, with a millisecond bound.

That work was attached directly to the one method contract every BIO
implementation shares. The internal struct behind `BIO_METHOD` now declares
three extra handler slots next to the classic method operations, and the
public waiting entry points in the library core dispatch unconditionally
through those slots for every BIO. As a result the capability is a required
part of the contract for all method families, not an opt-in:

- descriptor- and socket-backed types, which the support was really meant for,
  provide genuine poll(2)-based answers;
- source/sink types whose resource can never block (memory buffers, the null
  sink, stdio streams, the pre-connected BIO pair) had to invent plausible
  answers anyway;
- the filter types can only forward the calls to the next BIO in the chain,
  adding bookkeeping with no value of their own;
- the one method that is constructed at runtime from the public accessor API
  had to receive new construction entry points to install the same hooks, so
  even dynamically built methods cannot exist without answering readiness.

New BIO types now pay a readiness tax before they can do anything else, and
the shared contract has stopped being a description of what a BIO is.

## Desired outcome

Segregate the readiness capability from the shared method contract, while
keeping the capability itself working. We do not prescribe the mechanism
(a control-command dispatch, a separate opt-in capability interface, or
another design that fits this codebase are all acceptable), but the result
should meet all of the following:

- The shared method contract requires only the classic BIO method operations
  from an implementation: adding a new BIO type must not require answering
  readiness.
- The existing application-facing entry points (the non-blocking toggle and
  the two waiting calls) keep working for every BIO whose resource genuinely
  provides readiness, with their current success/failure semantics.
- A BIO whose resource cannot provide readiness fails cleanly through the
  normal dispatch path rather than dispatching through a slot that the
  implementation does not really have; nothing may crash or invoke undefined
  behavior when an unsupported capability is requested.
- Chain members (the filter/relay types) reach the capability of the BIO they
  wrap rather than pretending to provide it themselves.
- The capability is not deleted: the public readiness API stays declared in
  the public header and the supporting implementation stays reachable from it.

- Leave the tree tidy: no orphaned helper functions, prototypes, accessor
  entry points or members left behind for contract slots that no longer
  exist, and no stale comments describing the old arrangement.

## Compatibility boundary

- The eight classic BIO method slots and the public method-construction API
  (`BIO_meth_new` and its classic accessors) keep their current behavior.
- The full ctest suite stays green; TLS over the reworked path keeps
  functioning end to end.
- The change must be carried in the patch series (the patch under `patches/`
  that currently introduces the readiness support), because the materialized
  sources are regenerated. Verify by re-materializing and running the test
  suite after the change.

## Representative scenario

A caller that opens a memory BIO and asks it to wait until readable (a natural
mistake with a generic "drive any BIO" API) must get a clean negative answer
from the plumbing; a caller that does the same on a socket BIO gets the
current poll-based behavior. If a reviewer of your change re-runs
`./update.sh`, then builds and runs the test suite, the outcome must not
depend on anything you did outside the patch series.
