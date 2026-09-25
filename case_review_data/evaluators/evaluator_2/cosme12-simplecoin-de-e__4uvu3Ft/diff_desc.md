# Injection design record — SimpleCoin dead-code case

## Maintenance motivation

This case models the most common way dead code actually accumulates in a small
hobby crypto node: a "stability pass" retires risky or unused machinery, the
retirement is performed by flipping configuration switches rather than by
deleting the implementation, and the retired pieces keep living in the tree
because nobody wants to be the one who deletes protocol code. The chain keeps a
pennies-worth of code for every feature that was announced, half-shipped, or
replaced, and the three files that carry the node, the wallet, and the
configuration each end up with a different flavor of retired machinery.

The story being told is coherent repo history, not one author's single
mistake: the first release had a louder mining flow that announced proofs to
neighbour monitors; the stability pass decommissioned the monitors and made the
proof handover immediate. Peer sync previously spoke a "command protocol"
alongside the HTTP `/blocks` API, and it kept serving stale chains, so the
maintainers disabled it in configuration and kept the fetch code "as a
reference". Chain validation was made strict by default, leaving the older
relaxed hash-recomputation mode behind an always-on flag. A chain-health
monitor and an explorer-style chain renderer were drafted during the same
period and never wired into the entry points. The wallet grew a receipt-time
chain preview behind a feature switch for a panel that never shipped, plus an
address checksum helper for upload tooling that was cancelled.

## Development evolution being modeled

Real dead code arrives attached to realistic engineering decisions:

- **Feature retirement by switch.** Disabling a protocol in configuration is
  the cautious move, and the implementation stays because reverting is "easier"
  if someone still runs the old mode. The switches then quietly become part of
  the documented surface (the wallet help text and configuration comments
  describe them as intentional).
- **Handing over early.** A release note of the form "the search loop used to
  tell the monitors and only then hand the proof over" leaves the announcement
  tail behind a `return`, where it survives every later edit.
- **Drafts that were never wired.** Node-support classes and report helpers
  get written, reviewed, and merged on their own merits; the wiring task (call
  them from `mine()`, ship the explorer panel) simply never happens.
- **Tooling that lost its consumer.** A checksum helper and a short chain
  preview are small, well-tested-looking, and completely idle.

The case reproduces the result of several such episodes layered on the pinned
revision, each shaped the way the corresponding episode would really shape it.

## Overall design of the change

The change touches the node (`simpleCoin/miner.py`), the configuration
(`simpleCoin/miner_config.py`), and the wallet client
(`simpleCoin/wallet.py`), and additionally brings a behavioral test suite under
`simpleCoin/tests/`.

- The node hosts the majority of the retired machinery because it is the file
  with the most execution paths: the mining handover tail, the retired peer
  sync protocol behind its disabled switch, the strict-validation bypass, the
  never-instantiated health monitor, and the explorer-support helpers.
- The configuration file hosts the four retired-feature switches, because that
  is where the maintainers of this repository actually disable things; the
  comments describe them the way real configuration comments do ("it is
  disabled for good", "leave as True unless...", "kept so the preview can be
  restored quickly").
- The wallet hosts the client-side flavor of the same kind of leftovers: a
  receipt-time preview behind a disabled switch and a fully orphaned address
  helper with its import.
- The test suite documents the pinned behavior of all three production files
  before someone cleans up: block hashing and genesis determinism, the
  proof-of-work search and consensus semantics, node endpoints and signature
  validation, wallet menu flows, key handling, and the documented configuration
  values. The
  upstream release shipped no tests at this revision, so a refactor of this
  scale arriving together with a regression suite is the realistic way this
  work would land; it pins the behavior the cleanup must respect.

The injection is spread across three modules around the repo's two processes
rather than concentrated in one, so that recognizing reachability facts is the
work, not reading one large method.

## Design per location or cluster

### `simpleCoin/miner.py` — mining handover tail

`proof_of_work` ends on `return incrementer, blockchain`, and the statement
that used to run before the return — an announcement call to
`announce_proof_of_work(incrementer)` — now physically follows it, preceded by
the release comment explaining that the handover is immediate now. The
announcement helper itself (`requests.get(MINER_NODE_URL + '/proofs', ...)`)
stays at module level with its docstring history. Site chosen because it is the
single unconditional return of the mining search, making the tail factually
unreachable while reading like a leftover rather than a bug. Shape chosen as a
plain call statement rather than a block, so the leftover mirrors how one
instruction survives a flow rework.

### `simpleCoin/miner.py` — retired command-protocol sync

In `consensus`, right after `find_new_chains()`, a guard on the imported
`LEGACY_COMMAND_PROTOCOL_ENABLED` switch extends `other_chains` with
`legacy_peer_sync(PEER_NODES)` "for the small networks that still wanted it
around". `legacy_peer_sync` sleeps for `LEGACY_PEER_SYNC_TIMEOUT` seconds and
then GETs `peer + "/legacy-chain"` from each peer, validating and collecting
chains the same way its live sibling does. Site chosen because consensus is
the protocol-selection point of the node — exactly where a second, retired
transport would hook in — and the guard placement (immediately after the live
fetch, before longest-chain selection) is where a complementary source would
really be appended. Shape chosen with the guard reading a configuration flag
so the disabled state is a deployment decision, not a code bug.

### `simpleCoin/miner.py` — strict chain validation

`validate_blockchain` becomes a guard on `STRICT_CHAIN_VALIDATION`: strict
validation (the documented default since the stability pass) accepts the peer
chain immediately; everything after the guard is the relaxed-mode fallback that
recomputes the hash link of every block via `inspect_chain_hashes`. Site chosen
because validation is where config-driven mode selection genuinely happens;
shape deliberately the opposite polarity from the command-protocol guard (the
guard is always on and its accepting branch returns, so it is the fallback
that can never run), so the two disabled regions do not read as one duplicated
pattern. `inspect_chain_hashes` is a module-level helper of the same "reference
implementation" character as `legacy_peer_sync`.

### `simpleCoin/miner.py` — chain health monitor

`ChainHealthMonitor` — drafted to sit next to the mining loop and summarizes
mining statistics over a rolling `WINDOW_MINUTES` window — carries a class
constant, a constructor defaulting the window, `record`, `report`, and
`is_stalled`. It is placed after the module-level pending-transactions list,
i.e., in the support-code area of the module, with a docstring stating the
intended role and admitting the wiring never happened. Site and shape chosen
to exercise class-level reachability: an entire stateful support class that no
entry point constructs, whose methods are not individually interesting once
the owner is unreachable.

### `simpleCoin/miner.py` — explorer support helpers

Three small pieces drafted for a chain explorer that never shipped:
`Block.summary` (a one-line `Block #N -> hash` description on the otherwise
live block class), `explore_chain_report` (prints the chain using shortened
hashes), and `shorten_block_hash` (trims a hash to four plus four characters).
`Block.summary` specifically exercises the "unused member on a live owner"
shape: adding a rendering method to the block class is exactly what an explorer
feature would do first. The other two form a helper chain where the report
calls the hasher — the report has no caller, so the hasher is reachable only
from dead code.

### `simpleCoin/miner.py` — import line

The `from miner_config import ...` line is extended with the three retired
node-side switches (`LEGACY_COMMAND_PROTOCOL_ENABLED`,
`LEGACY_PEER_SYNC_TIMEOUT`, `STRICT_CHAIN_VALIDATION`). Two of the three stay
readable-by-live-code precisely because the guards read them; the timeout is
consumed only by the retired fetch. That mix is deliberate: it mirrors how
import lines look after partial retirement, and it keeps the constants cluster
in the configuration file honest — a flag is only as dead as its readers.

### `simpleCoin/miner_config.py` — retired feature switches

Four documented constants: `LEGACY_COMMAND_PROTOCOL_ENABLED = False`,
`LEGACY_PEER_SYNC_TIMEOUT = 30`, `STRICT_CHAIN_VALIDATION = True`, and
`WALLET_EXPLORER_ENABLED = False`, each with its maintainer comment. Site
chosen because configuration is the single documented wiring point of this
repository; the comments give the historical reason for each value the way
real configuration comments do. The timeout value coexists with a disabled
protocol because that is what retired-config drift looks like.

### `simpleCoin/wallet.py` — receipt-time chain preview

`send_transaction` keeps its documented flow (sign, POST, print the response
text) and adds a guard on `WALLET_EXPLORER_ENABLED` that prints a short chain
preview next to the receipt "while the explorer panel was being tested", with
`preview_wallet_chain` fetching `/blocks` and joining block hashes
(Connection errors degrade to a friendly line). Site chosen because the
receipt path is where a client-side preview would attach; shape chosen so the
guard sits after the documented print, keeping the documented output
untouched. In the clean file this guard shape did not exist — it belongs
entirely to the retired feature.

### `simpleCoin/wallet.py` — address checksum tooling

`checksum_address` (a `binascii.hexlify`-based eight-character digest) plus the
`import binascii` binding that exists only for it. Site chosen on the client
side to balance the node-side cluster; shape chosen as a fully orphaned
stdlib-consuming helper with its import, so the case includes the
import-binding flavor of retirement in the second process.

### `simpleCoin/tests/` — behavioral documentation

`conftest.py` puts the package on the path for the suite's module layout;
`test_block.py`, `test_miner.py`, and `test_wallet.py` describe the pinned
behavior of hashing, genesis, mining, consensus, endpoints, signature
validation, menu flows, key management, and configuration values. Cluster
chosen because the incoming cleanup task needs the behavior boundary pinned
from the outside, exactly as a regression suite would before touching
protocol-selection code; clock and network seams are monkeypatched so the
suite describes semantics, not wall-clock outcomes.

## Deliberate structural variation

The injected regions intentionally differ from each other so that the case is
not solvable by applying one syntactic template:

- Guard polarity is split: two always-False switches disable their bodies,
  while the one always-True switch disables what follows its terminated
  accepting branch.
- Reachability flavors differ: statement-after-return, guard-disabled branch,
  post-terminated-branch fallback, no-caller helper, caller-only-in-dead-code
  helper, dead-owner class, unused method on a live class, orphaned import
  binding, and orphaned configuration constant.
- Ownership differs: module-level functions, a class with four members, a
  method on the block class, an import line, and configuration constants.
- The comment voices differ per cluster (release note, configuration caveat,
  half-shipped feature recollection) so no single rationale text marks the
  injected code.

The variation reflects the story: each episode of the stability pass leaves
behind a different artifact of its own kind.
