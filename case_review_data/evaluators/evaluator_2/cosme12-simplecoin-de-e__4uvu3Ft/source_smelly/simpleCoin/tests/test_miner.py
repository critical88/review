"""Behaviour tests for the miner node: proof of work, consensus, the Flask
HTTP surface and transaction signature validation.

The tests exercise miner.py exactly the way the README documents it: running
the miner starts a node that keeps a blockchain copy, searches proofs of work
and serves /blocks and /txion to wallets and peer nodes. The wallet/peer
dependencies of those flows are simulated with monkeypatched network calls, so
no miner.py process or external node is required here.
"""

import base64
import json

import ecdsa
import pytest

import miner
import miner_config


def _make_keypair():
    """Real ECDSA keypair in the exact textual formats SimpleCoin uses."""
    sk = ecdsa.SigningKey.generate(curve=ecdsa.SECP256k1)
    vk_hex = sk.get_verifying_key().to_string().hex()
    public_key = base64.b64encode(bytes.fromhex(vk_hex)).decode()
    return sk, public_key


def _signed_message(sk, message):
    """Signature exactly like wallet.py encodes it for the /txion endpoint."""
    return base64.b64encode(sk.sign(message.encode())).decode()


class _FakeResponse:
    def __init__(self, content):
        self.content = content

    @property
    def text(self):
        return self.content.decode("utf-8")


def test_proof_of_work_finds_proof_of_genesis(monkeypatch):
    monkeypatch.setattr(miner, "consensus", lambda blockchain: False)
    proof, chain = miner.proof_of_work(1, miner.BLOCKCHAIN)
    assert proof == 7919
    assert proof % 1 == 0


def test_proof_of_work_proof_honours_both_conditions(monkeypatch):
    monkeypatch.setattr(miner, "consensus", lambda blockchain: False)
    proof, _ = miner.proof_of_work(3, miner.BLOCKCHAIN)
    assert proof % 7919 == 0
    assert proof % 3 == 0


def test_proof_of_work_for_larger_last_proof(monkeypatch):
    monkeypatch.setattr(miner, "consensus", lambda blockchain: False)
    proof, _ = miner.proof_of_work(7919, miner.BLOCKCHAIN)
    assert proof == 15838


def test_proof_of_work_returns_the_same_blockchain(monkeypatch):
    monkeypatch.setattr(miner, "consensus", lambda blockchain: False)
    local_chain = list(miner.BLOCKCHAIN)
    proof, returned = miner.proof_of_work(1, local_chain)
    assert proof == 7919
    assert returned is local_chain


def test_proof_of_work_yields_to_peer_chain_from_consensus(monkeypatch):
    peer_chain = [{"index": str(i)} for i in range(5)]
    monkeypatch.setattr(miner, "consensus", lambda blockchain: peer_chain)
    proof, returned = miner.proof_of_work(1, miner.BLOCKCHAIN)
    assert proof is False
    assert returned is peer_chain


def test_proof_of_work_keeps_searching_when_consensus_finds_nothing(monkeypatch):
    monkeypatch.setattr(miner, "consensus", lambda blockchain: False)
    proof, returned = miner.proof_of_work(1, miner.BLOCKCHAIN)
    assert isinstance(proof, int)
    assert returned is miner.BLOCKCHAIN


def test_consensus_adopts_longer_peer_chain(monkeypatch):
    peer_chains = [[{"index": str(i)} for i in range(4)]]
    monkeypatch.setattr(miner, "find_new_chains", lambda: peer_chains)
    result = miner.consensus(miner.BLOCKCHAIN)
    assert result is peer_chains[0]


def test_consensus_keeps_local_chain_when_it_is_longest(monkeypatch):
    peer_chains = [[{"index": "0"}]]
    monkeypatch.setattr(miner, "find_new_chains", lambda: peer_chains)
    assert miner.consensus(miner.BLOCKCHAIN) is False


def test_consensus_ignores_equal_length_peer_chains(monkeypatch):
    genesis = miner.create_genesis_block()
    local = [genesis, miner.Block(1, 100, {"proof-of-work": 1}, genesis.hash)]
    peer = [{"index": str(i)} for i in range(len(local))]
    monkeypatch.setattr(miner, "find_new_chains", lambda: [peer])
    assert miner.consensus(local) is False


def test_find_new_chains_returns_nothing_without_peers():
    assert miner.find_new_chains() == []


def test_find_new_chains_queries_every_peer(monkeypatch):
    requested = []

    def fake_get(url=None, **kwargs):
        requested.append(url)
        return _FakeResponse(json.dumps([{"index": "0"}]).encode("utf-8"))

    monkeypatch.setattr(miner, "PEER_NODES", ["http://peer-a:5000", "http://peer-b:5000"])
    monkeypatch.setattr(miner.requests, "get", fake_get)
    chains = miner.find_new_chains()
    assert requested == [
        "http://peer-a:5000/blocks",
        "http://peer-b:5000/blocks",
    ]
    assert chains == [[{"index": "0"}], [{"index": "0"}]]


def test_find_new_chains_drops_chains_failing_validation(monkeypatch):
    def fake_get(url=None, **kwargs):
        return _FakeResponse(json.dumps([{"index": "9"}]).encode("utf-8"))

    monkeypatch.setattr(miner, "PEER_NODES", ["http://peer-a:5000"])
    monkeypatch.setattr(miner.requests, "get", fake_get)
    monkeypatch.setattr(miner, "validate_blockchain", lambda block: False)
    assert miner.find_new_chains() == []


def test_validate_blockchain_accepts_candidate_chain():
    assert miner.validate_blockchain([{"index": "0"}]) is True
    assert miner.validate_blockchain([]) is True


def test_blocks_endpoint_serves_the_local_chain():
    client = miner.node.test_client()
    response = client.get("/blocks")
    assert response.status_code == 200
    served = json.loads(response.text)
    assert len(served) == 1
    assert served[0]["hash"] == miner.BLOCKCHAIN[0].hash


def test_blocks_endpoint_serialises_fields_as_strings():
    client = miner.node.test_client()
    served = json.loads(client.get("/blocks").text)
    genesis = miner.BLOCKCHAIN[0]
    assert served[0]["index"] == str(genesis.index)
    assert served[0]["timestamp"] == str(genesis.timestamp)
    assert served[0]["data"] == str(genesis.data)


def test_txion_get_releases_pending_transactions(monkeypatch):
    pending = [{
        "from": "wallet-a", "to": "wallet-b",
        "amount": "5", "signature": "sig", "message": "1700000000",
    }]
    monkeypatch.setattr(miner, "NODE_PENDING_TRANSACTIONS", pending)
    client = miner.node.test_client()
    response = client.get(
        "/txion", query_string={"update": miner_config.MINER_ADDRESS})
    assert response.status_code == 200
    assert json.loads(response.text) == [{
        "from": "wallet-a", "to": "wallet-b",
        "amount": "5", "signature": "sig", "message": "1700000000",
    }]
    assert pending == []


def test_txion_post_rejects_wrong_signature():
    sk, public_key = _make_keypair()
    payload = {
        "from": public_key,
        "to": "somebody",
        "amount": "2",
        "signature": _signed_message(sk, "a different message"),
        "message": "1700000000",
    }
    client = miner.node.test_client()
    response = client.post("/txion", json=payload)
    assert response.text == "Transaction submission failed. Wrong signature\n"


def test_txion_post_accepts_signed_transaction(monkeypatch):
    sk, public_key = _make_keypair()
    message = "1700000000"
    payload = {
        "from": public_key,
        "to": "somebody",
        "amount": "2",
        "signature": _signed_message(sk, message),
        "message": message,
    }
    monkeypatch.setattr(miner, "NODE_PENDING_TRANSACTIONS", [])
    client = miner.node.test_client()
    response = client.post("/txion", json=payload)
    assert response.text == "Transaction submission successful\n"
    assert miner.NODE_PENDING_TRANSACTIONS == [payload]


def test_validate_signature_accepts_genuine_signature():
    sk, public_key = _make_keypair()
    message = "send-coins"
    assert miner.validate_signature(
        public_key, _signed_message(sk, message), message) is True


def test_validate_signature_rejects_tampered_message():
    sk, public_key = _make_keypair()
    signature = _signed_message(sk, "original message")
    assert miner.validate_signature(
        public_key, signature, "tampered message") is False


def test_validate_signature_rejects_garbage_public_key():
    # The catch-all except in validate_signature only wraps the verify call,
    # so malformed public keys surface their decoding error unguarded.
    garbage = base64.b64encode(b"not a compressed key at all").decode()
    with pytest.raises(Exception):
        miner.validate_signature(garbage, "anything", "message")


def test_validate_signature_rejects_undecodable_input():
    with pytest.raises(Exception):
        miner.validate_signature("###not base64###", "sig", "message")


def test_welcome_msg_prints_network_banner(capsys):
    miner.welcome_msg()
    out = capsys.readouterr().out
    assert "SIMPLE COIN v1.0.0 - BLOCKCHAIN SYSTEM" in out
