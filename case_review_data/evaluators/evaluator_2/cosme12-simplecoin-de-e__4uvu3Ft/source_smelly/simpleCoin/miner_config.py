"""Configure this file before you start mining. Check wallet.py for
more details.
"""

# Write your generated adress here. All coins mined will go to this address
MINER_ADDRESS = "q3nf394hjg-random-miner-address-34nf3i4nflkn3oi"

# Write your node url or ip. If you are running it localhost use default
MINER_NODE_URL = "http://localhost:5000"

# Store the url data of every other node in the network
# so that we can communicate with them
PEER_NODES = []

# The command protocol was the pre-stability-pass way nodes exchanged their
# chains. It kept answering with stale chains after a node had already moved
# on, so it is disabled for good; the peers still speak the /blocks API.
LEGACY_COMMAND_PROTOCOL_ENABLED = False

# Seconds a command-protocol sync request would wait for a peer answer
# before giving up on it.
LEGACY_PEER_SYNC_TIMEOUT = 30

# Leave as True unless a peer network still needs the relaxed hash checks.
STRICT_CHAIN_VALIDATION = True

# The wallet explorer panel was retired with the protocol update. The wallet
# keeps looking at this switch so the preview can be restored quickly.
WALLET_EXPLORER_ENABLED = False
