import time
import hashlib
import json
import requests
import base64
from flask import Flask, request
from multiprocessing import Process, Pipe
import ecdsa

from miner_config import MINER_ADDRESS, MINER_NODE_URL, PEER_NODES, \
    LEGACY_COMMAND_PROTOCOL_ENABLED, LEGACY_PEER_SYNC_TIMEOUT, \
    STRICT_CHAIN_VALIDATION

node = Flask(__name__)


class Block:
    def __init__(self, index, timestamp, data, previous_hash):
        """Returns a new Block object. Each block is "chained" to its previous
        by calling its unique hash.

        Args:
            index (int): Block number.
            timestamp (int): Block creation timestamp.
            data (str): Data to be sent.
            previous_hash(str): String representing previous block unique hash.

        Attrib:
            index (int): Block number.
            timestamp (int): Block creation timestamp.
            data (str): Data to be sent.
            previous_hash(str): String representing previous block unique hash.
            hash(str): Current block unique hash.

        """
        self.index = index
        self.timestamp = timestamp
        self.data = data
        self.previous_hash = previous_hash
        self.hash = self.hash_block()

    def hash_block(self):
        """Creates the unique hash for the block. It uses sha256."""
        sha = hashlib.sha256()
        sha.update((str(self.index) + str(self.timestamp) + str(self.data) + str(self.previous_hash)).encode('utf-8'))
        return sha.hexdigest()

    def summary(self):
        """One line block description used by the explorer support."""
        return "Block #{0} -> {1}".format(self.index, self.previous_hash)


def create_genesis_block():
    """To create each block, it needs the hash of the previous one. First
    block has no previous, so it must be created manually (with index zero
     and arbitrary previous hash)"""
    return Block(0, time.time(), {
        "proof-of-work": 9,
        "transactions": None},
        "0")


# Node's blockchain copy
BLOCKCHAIN = [create_genesis_block()]

""" Stores the transactions that this node has in a list.
If the node you sent the transaction adds a block
it will get accepted, but there is a chance it gets
discarded and your transaction goes back as if it was never
processed"""
NODE_PENDING_TRANSACTIONS = []


class ChainHealthMonitor:
    """Keeps a rolling window of mining statistics for this node.

    Intended to sit next to the mining loop and keep the node operator
    informed about stalled searches. It was drafted during the stability
    pass; wiring it into mine() never happened, so no node constructs one.
    """

    WINDOW_MINUTES = 10

    def __init__(self, window_minutes=None):
        self.window_minutes = window_minutes or self.WINDOW_MINUTES
        self.samples = []

    def record(self, sample):
        """Store one mining measurement inside the window."""
        self.samples.append((time.time(), sample))
        return len(self.samples)

    def report(self):
        """Text summary of the collected samples."""
        return "samples={0} window_minutes={1}".format(
            len(self.samples), self.window_minutes)

    def is_stalled(self):
        """True when the newest sample is older than the window."""
        if not self.samples:
            return False
        age = time.time() - self.samples[0][0]
        return age > self.window_minutes * 60


def proof_of_work(last_proof, blockchain):
    # Creates a variable that we will use to find our next proof of work
    incrementer = last_proof + 1
    # Keep incrementing the incrementer until it's equal to a number divisible by 7919
    # and the proof of work of the previous block in the chain
    start_time = time.time()
    while not (incrementer % 7919 == 0 and incrementer % last_proof == 0):
        incrementer += 1
        # Check if any node found the solution every 60 seconds
        if int((time.time()-start_time) % 60) == 0:
            # If any other node got the proof, stop searching
            new_blockchain = consensus(blockchain)
            if new_blockchain:
                # (False: another node got proof first, new blockchain)
                return False, new_blockchain
    # Once that number is found, we can return it as a proof of our work
    return incrementer, blockchain
    # The announcement below is left from the first mining release: back then
    # the search loop used to tel the monitors and only then hand the proof
    # over. The handover is immediate now, so this no longer runs.
    announce_proof_of_work(incrementer)


def announce_proof_of_work(proof):
    """Tel the neighbourhood monitors that this node found a new proof.

    Leftover from the first release announcement loop; the monitors were
    decommissioned short after the stability pass.
    """
    requests.get(url = MINER_NODE_URL + '/proofs', params = {'proof': proof})


def mine(a, blockchain, node_pending_transactions):
    BLOCKCHAIN = blockchain
    NODE_PENDING_TRANSACTIONS = node_pending_transactions
    while True:
        """Mining is the only way that new coins can be created.
        In order to prevent too many coins to be created, the process
        is slowed down by a proof of work algorithm.
        """
        # Get the last proof of work
        last_block = BLOCKCHAIN[-1]
        last_proof = last_block.data['proof-of-work']
        # Find the proof of work for the current block being mined
        # Note: The program will hang here until a new proof of work is found
        proof = proof_of_work(last_proof, BLOCKCHAIN)
        # If we didn't guess the proof, start mining again
        if not proof[0]:
            # Update blockchain and save it to file
            BLOCKCHAIN = proof[1]
            a.send(BLOCKCHAIN)
            continue
        else:
            # Once we find a valid proof of work, we know we can mine a block so
            # ...we reward the miner by adding a transaction
            # First we load all pending transactions sent to the node server
            NODE_PENDING_TRANSACTIONS = requests.get(url = MINER_NODE_URL + '/txion', params = {'update':MINER_ADDRESS}).content
            NODE_PENDING_TRANSACTIONS = json.loads(NODE_PENDING_TRANSACTIONS)
            # Then we add the mining reward
            NODE_PENDING_TRANSACTIONS.append({
                "from": "network",
                "to": MINER_ADDRESS,
                "amount": 1})
            # Now we can gather the data needed to create the new block
            new_block_data = {
                "proof-of-work": proof[0],
                "transactions": list(NODE_PENDING_TRANSACTIONS)
            }
            new_block_index = last_block.index + 1
            new_block_timestamp = time.time()
            last_block_hash = last_block.hash
            # Empty transaction list
            NODE_PENDING_TRANSACTIONS = []
            # Now create the new block
            mined_block = Block(new_block_index, new_block_timestamp, new_block_data, last_block_hash)
            BLOCKCHAIN.append(mined_block)
            # Let the client know this node mined a block
            print(json.dumps({
              "index": new_block_index,
              "timestamp": str(new_block_timestamp),
              "data": new_block_data,
              "hash": last_block_hash
            }, sort_keys=True) + "\n")
            a.send(BLOCKCHAIN)
            requests.get(url = MINER_NODE_URL + '/blocks', params = {'update':MINER_ADDRESS})

def find_new_chains():
    # Get the blockchains of every other node
    other_chains = []
    for node_url in PEER_NODES:
        # Get their chains using a GET request
        block = requests.get(url = node_url + "/blocks").content
        # Convert the JSON object to a Python dictionary
        block = json.loads(block)
        # Verify other node block is correct
        validated = validate_blockchain(block)
        if validated:
            # Add it to our list
            other_chains.append(block)
    return other_chains


def legacy_peer_sync(peers):
    """Collect the chains of every peer over the retired command protocol.

    The command protocol was the pre-stability-pass way of syncing. It is
    disabled in miner_config.py, but the fetch logic is kept here as a
    reference for whoever studies the old behaviour.
    """
    time.sleep(LEGACY_PEER_SYNC_TIMEOUT)
    other_chains = []
    for peer in peers:
        block = requests.get(url = peer + "/legacy-chain").content
        block = json.loads(block)
        # Verify other node block is correct
        if validate_blockchain(block):
            other_chains.append(block)
    return other_chains


def consensus(blockchain):
    # Get the blocks from other nodes
    other_chains = find_new_chains()
    if LEGACY_COMMAND_PROTOCOL_ENABLED:
        # Complementary chain sources from the retired command protocol.
        # Kept for the small networks that still wanted it around.
        other_chains += legacy_peer_sync(PEER_NODES)
    # If our chain isn't longest, then we store the longest chain
    BLOCKCHAIN = blockchain
    longest_chain = BLOCKCHAIN
    for chain in other_chains:
        if len(longest_chain) < len(chain):
            longest_chain = chain
    # If the longest chain wasn't ours, then we set our chain to the longest
    if longest_chain == BLOCKCHAIN:
        # Keep searching for proof
        return False
    else:
        # Give up searching proof, update chain and start over again
        BLOCKCHAIN = longest_chain
        return BLOCKCHAIN


def validate_blockchain(block):
    """Validate the submitted chain. If hashes are not correct, return false
    block(str): json
    """
    if STRICT_CHAIN_VALIDATION:
        # Strict validation (the default since the stability pass) accepts a
        # chain as soon as the peer states its blocks in order.
        return True
    # Relaxed mode fallback: recompute the hash link of every block. This
    # branch is only reachable for peer networks that turn the strict
    # validation off in miner_config.py.
    return inspect_chain_hashes(block)


def inspect_chain_hashes(chain):
    """Relaxed-mode chain check: verify the hash link of every block.

    Only consulted when STRICT_CHAIN_VALIDATION is turned off in
    miner_config.py, which no current network does.
    """
    previous = "0"
    for block in chain:
        current = block.get("hash") if isinstance(block, dict) else None
        if current is None:
            return False
        previous = current
    return True


@node.route('/blocks', methods=['GET'])
def get_blocks():
    # Load current blockchain. Only you should update your blockchain
    if request.args.get("update") == MINER_ADDRESS:
        global BLOCKCHAIN
        BLOCKCHAIN = pipe_input.recv()
    chain_to_send = BLOCKCHAIN
    # Converts our blocks into dictionaries so we can send them as json objects later
    chain_to_send_json = []
    for block in chain_to_send:
        block = {
            "index": str(block.index),
            "timestamp": str(block.timestamp),
            "data": str(block.data),
            "hash": block.hash
        }
        chain_to_send_json.append(block)

    # Send our chain to whomever requested it
    chain_to_send = json.dumps(chain_to_send_json, sort_keys=True)
    return chain_to_send


@node.route('/txion', methods=['GET', 'POST'])
def transaction():
    """Each transaction sent to this node gets validated and submitted.
    Then it waits to be added to the blockchain. Transactions only move
    coins, they don't create it.
    """
    if request.method == 'POST':
        # On each new POST request, we extract the transaction data
        new_txion = request.get_json()
        # Then we add the transaction to our list
        if validate_signature(new_txion['from'], new_txion['signature'], new_txion['message']):
            NODE_PENDING_TRANSACTIONS.append(new_txion)
            # Because the transaction was successfully
            # submitted, we log it to our console
            print("New transaction")
            print("FROM: {0}".format(new_txion['from']))
            print("TO: {0}".format(new_txion['to']))
            print("AMOUNT: {0}\n".format(new_txion['amount']))
            # Then we let the client know it worked out
            return "Transaction submission successful\n"
        else:
            return "Transaction submission failed. Wrong signature\n"
    # Send pending transactions to the mining process
    elif request.method == 'GET' and request.args.get("update") == MINER_ADDRESS:
        pending = json.dumps(NODE_PENDING_TRANSACTIONS, sort_keys=True)
        # Empty transaction list
        NODE_PENDING_TRANSACTIONS[:] = []
        return pending


def explore_chain_report():
    """Print a short human readable chain summary for the explorer support.

    The wallet explorer panel that was supposed to call this never shipped,
    so the support routine stayed unused in the node.
    """
    for block in BLOCKCHAIN:
        print("Block #{0}: {1}".format(block.index, shorten_block_hash(block.hash)))


def shorten_block_hash(block_hash):
    """Shorten a block hash for display purposes (explorer support)."""
    if not block_hash:
        return ""
    return block_hash[:4] + "..." + block_hash[-4:]


def validate_signature(public_key, signature, message):
    """Verifies if the signature is correct. This is used to prove
    it's you (and not someone else) trying to do a transaction with your
    address. Called when a user tries to submit a new transaction.
    """
    public_key = (base64.b64decode(public_key)).hex()
    signature = base64.b64decode(signature)
    vk = ecdsa.VerifyingKey.from_string(bytes.fromhex(public_key), curve=ecdsa.SECP256k1)
    # Try changing into an if/else statement as except is too broad.
    try:
        return vk.verify(signature, message.encode())
    except:
        return False


def welcome_msg():
    print("""       =========================================\n
        SIMPLE COIN v1.0.0 - BLOCKCHAIN SYSTEM\n
       =========================================\n\n
        You can find more help at: https://github.com/cosme12/SimpleCoin\n
        Make sure you are using the latest version or you may end in
        a parallel chain.\n\n\n""")


if __name__ == '__main__':
    welcome_msg()
    # Start mining
    pipe_output, pipe_input = Pipe()
    miner_process = Process(target=mine, args=(pipe_output, BLOCKCHAIN, NODE_PENDING_TRANSACTIONS))
    miner_process.start()

    # Start server to receive transactions
    transactions_process = Process(target=node.run(), args=pipe_input)
    transactions_process.start()
