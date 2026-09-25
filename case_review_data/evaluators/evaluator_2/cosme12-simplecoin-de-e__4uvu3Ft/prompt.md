# Maintenance request

I maintain SimpleCoin, the little blockchain node and wallet in `simpleCoin/`. Over the last releases we retired a bunch of things: the old peer-sync protocol that kept handing out stale chains, the announcement loop the first release used to shout proofs at neighbourhood monitors, the strict chain validation became the only mode anyone runs, and an explorer panel for the wallet never shipped at all. The result is that `miner.py` now reads like the museum of everything we stopped doing, and `wallet.py` has picked up some of the same habits.

I first ran into this while working around `Block` in `simpleCoin/miner.py`; please start there and follow the related call path.

Please remove the obsolete path and the production scaffolding that exists only to support it, while keeping the active flow straightforward.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
