import time

from .base import XGateway
from ..device import BLE
from ..mini_mqtt import MQTTMessage
from ..shell.shell_mgw import ShellMGW


def ble_identity(did: str, model: str | int, mac: str) -> tuple:
    """Normalize identity of the BLE device.

    Can be used for devices from the gateway DB and for devices
    created on the fly from the report.
    """
    return did, model, mac.lower()


# noinspection PyMethodMayBeStatic,PyUnusedLocal
class BLEGateway(XGateway):
    async def ble_read_devices(self, sh: ShellMGW):
        db = await sh.read_db_bluetooth()
        rows = db.read_table("gateway_authed_table")
        for row in rows:
            did = row[4]
            device = self.devices.get(did)
            if not device:
                did, model, mac = ble_identity(did, row[2], reverse_mac(row[1]))
                device = self.init_device(model, did, BLE, mac)
            self.add_device(device)

    def ble_on_mqtt_publish(self, msg: MQTTMessage):
        if msg.topic in ("miio/report", "central/report"):
            if b'"_async.ble_event"' in msg.payload:
                self.ble_process_event(msg.json["params"])
            elif b'"_sync.ble_keep_alive"' in msg.payload:
                self.ble_process_keepalive(msg.json["params"])

    def ble_process_event(self, data: dict):
        """
        {
            'dev': {'did': 'blt.3.xxx', 'mac': 'AA:BB:CC:DD:EE:FF', 'pdid': 2038},
            'evt': [{'eid': 15, 'edata': '010000'}],
            'frmCnt': 36, 'gwts': 1636208932
        }
        """

        did = data["dev"]["did"]
        device = self.devices.get(did)
        if not device:
            # https://github.com/AlexxIT/XiaomiGateway3/issues/24
            if "mac" not in data["dev"]:
                self.debug("Unknown device without mac", data=data)
                return
            # create device "on the fly"
            did, model, mac = ble_identity(
                did, data["dev"]["pdid"], data["dev"]["mac"]
            )
            device = self.init_device(model, did, BLE, mac)
            device.available = True
            self.add_device(device)

        ts = device.on_keep_alive(self)

        if data["frmCnt"] == device.extra.get("seq"):
            return
        device.extra["seq"] = data["frmCnt"]

        device.on_report(data["evt"], self, ts)
        if self.stats_domain:
            device.dispatch({BLE: ts})

    def ble_process_keepalive(self, data: list):
        ts = int(time.time())

        for item in data:
            if device := self.devices.get(item["did"]):
                # noinspection PyTypedDict
                device.extra["rssi_" + self.device.uid] = item["rssi"]
                device.on_keep_alive(self, ts)


def reverse_mac(s: str):
    return f"{s[10:]}:{s[8:10]}:{s[6:8]}:{s[4:6]}:{s[2:4]}:{s[:2]}"
