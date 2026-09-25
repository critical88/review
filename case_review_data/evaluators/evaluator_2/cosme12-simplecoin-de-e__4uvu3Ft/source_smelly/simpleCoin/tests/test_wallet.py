"""Behaviour tests for the wallet client: the interactive menu flow, sending
signed transactions, and the ECDSA key handling.

wallet.py is a terminal client, so the menu tests drive the documented user
paths (generate wallet, send coins, check transactions, quit) with
monkeypatched stdin. Network calls towards the miner node are simulated, so
the tests document the client-server contract without running miner.py.
"""

import base64
import types

import ecdsa
import pytest
import requests

import miner
import miner_config
import wallet


def _menu_inputs(sequence):
    """Sequenced stdin reader that records how often the menu prompts."""
    calls = {"count": 0}

    def read(_prompt=None):
        calls["count"] += 1
        return sequence.pop(0)

    return read, calls


def _fake_post(recorder, text="Transaction submission successful"):
    class _Response:
        pass

    def post(url=None, json=None, headers=None, **kwargs):
        recorder.append({
            "url": url, "json": json, "headers": headers,
        })
        response = _Response()
        response.text = text
        return response

    return post


def test_wallet_menu_generates_new_wallet(monkeypatch):
    generated = []
    monkeypatch.setattr(wallet, "generate_ECDSA_keys", lambda: generated.append(True))
    monkeypatch.setattr("builtins.input", lambda _prompt=None: "1")
    wallet.wallet()
    assert generated == [True]


def test_wallet_menu_sends_confirmed_transaction(monkeypatch):
    sent = []

    def record_send(addr_from, private_key, addr_to, amount):
        sent.append((addr_from, private_key, addr_to, amount))

    monkeypatch.setattr(wallet, "send_transaction", record_send)
    inputs = ["2", "PUBLICKEY", "01" * 32, "DESTINATION", "25", "y"]
    monkeypatch.setattr("builtins.input", lambda _prompt=None: inputs.pop(0))
    wallet.wallet()
    assert sent == [("PUBLICKEY", "01" * 32, "DESTINATION", "25")]


def test_wallet_send_flow_aborts_without_confirmation(monkeypatch):
    sent = []
    monkeypatch.setattr(wallet, "send_transaction",
                        lambda *args: sent.append(args))
    inputs = ["2", "PUBLICKEY", "01" * 32, "DESTINATION", "25", "n", "4"]
    monkeypatch.setattr("builtins.input", lambda _prompt=None: inputs.pop(0))
    with pytest.raises(SystemExit):
        wallet.wallet()
    assert sent == []


def test_wallet_menu_checks_transactions_then_returns_to_menu(monkeypatch):
    checked = []
    monkeypatch.setattr(wallet, "check_transactions",
                        lambda: checked.append(True))
    inputs = ["3", "4"]
    monkeypatch.setattr("builtins.input", lambda _prompt=None: inputs.pop(0))
    with pytest.raises(SystemExit):
        wallet.wallet()
    assert checked == [True]


def test_wallet_menu_quits(monkeypatch):
    monkeypatch.setattr("builtins.input", lambda _prompt=None: "4")
    with pytest.raises(SystemExit):
        wallet.wallet()


def test_wallet_menu_reprompts_on_invalid_choice(monkeypatch):
    read, calls = _menu_inputs(["9", "4"])
    monkeypatch.setattr("builtins.input", read)
    with pytest.raises(SystemExit):
        wallet.wallet()
    assert calls["count"] == 2


def test_wallet_menu_recovers_after_invalid_input(monkeypatch):
    sent = []
    monkeypatch.setattr(wallet, "send_transaction",
                        lambda *args: sent.append(args))
    inputs = ["abc", "2", "PUBLICKEY", "01" * 32, "DESTINATION", "25", "n", "4"]
    monkeypatch.setattr("builtins.input", lambda _prompt=None: inputs.pop(0))
    with pytest.raises(SystemExit):
        wallet.wallet()
    assert sent == []


def test_send_transaction_posts_signed_payload_to_local_node(monkeypatch):
    recorder = []
    monkeypatch.setattr(wallet.requests, "post", _fake_post(recorder))
    monkeypatch.setattr(
        wallet, "sign_ECDSA_msg",
        lambda private_key: (base64.b64encode(b"raw signature"), "1700000000"))

    wallet.send_transaction("PUBLICKEY", "01" * 32, "DESTINATION", "25")

    assert len(recorder) == 1
    call = recorder[0]
    assert call["url"] == "http://localhost:5000/txion"
    assert call["headers"] == {"Content-Type": "application/json"}
    assert call["json"] == {
        "from": "PUBLICKEY",
        "to": "DESTINATION",
        "amount": "25",
        "signature": base64.b64encode(b"raw signature").decode(),
        "message": "1700000000",
    }


def test_send_transaction_rejects_wrong_key_length(monkeypatch):
    recorder = []
    monkeypatch.setattr(wallet.requests, "post", _fake_post(recorder))
    monkeypatch.setattr(
        wallet, "sign_ECDSA_msg",
        lambda private_key: (base64.b64encode(b"raw signature"), "1700000000"))

    wallet.send_transaction("PUBLICKEY", "ab" * 31, "DESTINATION", "25")

    assert recorder == []


def test_send_transaction_prints_node_response(monkeypatch, capsys):
    recorder = []
    monkeypatch.setattr(
        wallet.requests, "post",
        _fake_post(recorder, text="Transaction queued by node"))
    monkeypatch.setattr(
        wallet, "sign_ECDSA_msg",
        lambda private_key: (base64.b64encode(b"raw signature"), "1700000000"))

    wallet.send_transaction("PUBLICKEY", "01" * 32, "DESTINATION", "25")

    assert "Transaction queued by node" in capsys.readouterr().out


def test_check_transactions_prints_pretty_json(monkeypatch, capsys):
    class _Response:
        text = None

    response = _Response()
    response.text = '[{"index": "0", "hash": "abc"}]'
    monkeypatch.setattr(wallet.requests, "get", lambda url: response)
    wallet.check_transactions()
    out = capsys.readouterr().out
    assert '"hash": "abc"' in out
    assert '"index": "0"' in out
    assert '\n    {' in out


def test_check_transactions_reports_connection_errors(monkeypatch, capsys):
    def raise_error(url):
        raise requests.ConnectionError("connection refused")

    monkeypatch.setattr(wallet.requests, "get", raise_error)
    wallet.check_transactions()
    out = capsys.readouterr().out
    assert "Connection error. Make sure that you have run miner.py" in out


def test_generate_ecdsa_keys_writes_credentials_file(monkeypatch, tmp_path):
    monkeypatch.chdir(tmp_path)
    monkeypatch.setattr("builtins.input", lambda _prompt=None: "mywallet")
    wallet.generate_ECDSA_keys()

    content = (tmp_path / "mywallet.txt").read_text()
    lines = content.splitlines()
    assert lines[0].startswith("Private key: ")
    private_key = lines[0].split(": ", 1)[1]
    assert len(private_key) == 64
    int(private_key, 16)
    assert lines[1].startswith("Wallet address / Public key: ")

    public_key = lines[1].split(": ", 1)[1]
    sk = ecdsa.SigningKey.from_string(bytes.fromhex(private_key),
                                      curve=ecdsa.SECP256k1)
    vk_hex = sk.get_verifying_key().to_string().hex()
    expected_public = base64.b64encode(bytes.fromhex(vk_hex)).decode()
    assert public_key == expected_public


def test_generate_ecdsa_keys_prints_created_filename(monkeypatch, tmp_path, capsys):
    monkeypatch.chdir(tmp_path)
    monkeypatch.setattr("builtins.input", lambda _prompt=None: "mywallet")
    wallet.generate_ECDSA_keys()
    assert "now in the file mywallet.txt" in capsys.readouterr().out


def test_wallet_key_format_is_accepted_by_node_validation():
    sk = ecdsa.SigningKey.generate(curve=ecdsa.SECP256k1)
    vk_hex = sk.get_verifying_key().to_string().hex()
    public_key = base64.b64encode(bytes.fromhex(vk_hex)).decode()
    message = "transfer"
    signature = base64.b64encode(sk.sign(message.encode())).decode()
    assert miner.validate_signature(public_key, signature, message) is True


def test_sign_ecdsa_message_signs_rounded_timestamp(monkeypatch):
    monkeypatch.setattr(wallet, "time",
                        types.SimpleNamespace(time=lambda: 1700000000.4))
    signature, message = wallet.sign_ECDSA_msg("01" * 32)
    assert message == "1700000000"
    assert isinstance(signature, bytes)


def test_sign_ecdsa_message_produces_valid_signature():
    private_key = "01" * 32
    sk = ecdsa.SigningKey.from_string(bytes.fromhex(private_key),
                                      curve=ecdsa.SECP256k1)
    signature, message = wallet.sign_ECDSA_msg(private_key)
    decoded = base64.b64decode(signature)
    assert sk.get_verifying_key().verify(decoded, message.encode()) is True


def test_sign_ecdsa_message_signature_is_base64(monkeypatch):
    monkeypatch.setattr(wallet, "time",
                        types.SimpleNamespace(time=lambda: 1700000000.9))
    signature, _ = wallet.sign_ECDSA_msg("01" * 32)
    assert base64.b64encode(base64.b64decode(signature)) == signature


def test_miner_config_defines_miner_address():
    assert miner_config.MINER_ADDRESS == \
        "q3nf394hjg-random-miner-address-34nf3i4nflkn3oi"


def test_miner_config_defines_local_node_url():
    assert miner_config.MINER_NODE_URL == "http://localhost:5000"


def test_miner_config_defines_peer_nodes():
    assert miner_config.PEER_NODES == []


def test_wallet_module_documents_operations():
    for operation in ("wallet", "send_transaction", "check_transactions",
                      "generate_ECDSA_keys", "sign_ECDSA_msg"):
        assert callable(getattr(wallet, operation))


def test_miner_module_documents_node_services():
    assert callable(miner.node.test_client)
    genesis = miner.create_genesis_block()
    assert isinstance(genesis, miner.Block)
    assert callable(miner.proof_of_work)
    assert callable(miner.consensus)
