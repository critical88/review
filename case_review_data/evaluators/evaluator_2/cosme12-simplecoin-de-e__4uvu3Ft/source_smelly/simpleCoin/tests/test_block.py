"""Behaviour tests for the Block data structure and the genesis block.

These tests pin the block hashing contract and the shape of the node's local
blockchain copy when miner.py is freshly imported.
"""

import hashlib
import types

import miner


def test_block_holds_fields():
    block = miner.Block(3, 1500000000, {"proof-of-work": 7}, "abc")
    assert block.index == 3
    assert block.timestamp == 1500000000
    assert block.data == {"proof-of-work": 7}
    assert block.previous_hash == "abc"


def test_block_hash_is_stable_for_same_content():
    first = miner.Block(1, 100, "payload", "previous")
    second = miner.Block(1, 100, "payload", "previous")
    assert first.hash == second.hash


def test_block_hash_changes_with_index():
    base = miner.Block(1, 100, "payload", "previous")
    other = miner.Block(2, 100, "payload", "previous")
    assert base.hash != other.hash


def test_block_hash_changes_with_timestamp():
    base = miner.Block(1, 100, "payload", "previous")
    other = miner.Block(1, 200, "payload", "previous")
    assert base.hash != other.hash


def test_block_hash_changes_with_data():
    base = miner.Block(1, 100, "payload", "previous")
    other = miner.Block(1, 100, "other payload", "previous")
    assert base.hash != other.hash


def test_block_hash_changes_with_previous_hash():
    base = miner.Block(1, 100, "payload", "previous")
    other = miner.Block(1, 100, "payload", "different")
    assert base.hash != other.hash


def test_block_hash_is_sha256_of_joined_fields():
    block = miner.Block(1, 2, "data", "prev")
    expected = hashlib.sha256("12dataprev".encode("utf-8")).hexdigest()
    assert block.hash == expected


def test_block_hash_is_64_char_hex_digest():
    block = miner.Block(1, 2, "data", "prev")
    assert len(block.hash) == 64
    int(block.hash, 16)


def test_block_data_accepts_dict_payload():
    payload = {"proof-of-work": 9, "transactions": [{"from": "a", "to": "b"}]}
    block = miner.Block(4, 300, payload, "deadbeef")
    assert block.data is payload


def test_hash_block_returns_hexdigest_string():
    block = miner.Block(1, 2, "data", "prev")
    assert isinstance(block.hash_block(), str)


def test_genesis_block_is_index_zero():
    genesis = miner.create_genesis_block()
    assert genesis.index == 0


def test_genesis_block_previous_hash_is_zero_string():
    genesis = miner.create_genesis_block()
    assert genesis.previous_hash == "0"


def test_genesis_block_carries_seed_payload():
    genesis = miner.create_genesis_block()
    assert genesis.data["proof-of-work"] == 9
    assert genesis.data["transactions"] is None


def test_genesis_block_hash_is_deterministic(monkeypatch):
    monkeypatch.setattr(miner, "time", types.SimpleNamespace(time=lambda: 0.0))
    genesis = miner.create_genesis_block()
    expected = hashlib.sha256(
        "00.0{'proof-of-work': 9, 'transactions': None}0".encode("utf-8")
    ).hexdigest()
    assert genesis.hash == expected


def test_node_blockchain_copy_starts_with_only_genesis():
    assert len(miner.BLOCKCHAIN) == 1
    assert miner.BLOCKCHAIN[0].index == 0
