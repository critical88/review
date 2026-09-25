# Injection design record — device identity traveling as parallel parameters

## Realistic maintenance motivation

XiaomiGateway3 supports several gateway generations (Multimode ZNDMWG03LM, Multimode 2 ZNDMWG02LM, Gateway E1 ZNCZ12LM and siblings) and five transports: miio/SSH gateway info, zigbee, bluetooth, mesh (miot) and matter. Every child device the integration exposes — the gateway itself, zigbee sensors, BLE peripherals, mesh bulbs and groups, matter devices — must end up in the shared device registry with a well-defined identity: a `did` registry key, a type, a model, optional hardware addresses (`mac`, `mac2`, `ieee`, `nwk`) and optional versions (`fw_ver`, `hw_ver`) plus a cloud-mapping entry. That identity mapping already exists on `XDevice` as a fixed `XDeviceExtra` dict — so the data set itself is objective, not a matter of taste.

In the upstream state, every adapter assembles this mapping by hand at its discovery site (a dict literal plus a couple of conditional inserts) and pushes it into the shared registration factory through `**kwargs`. New identity sources keep arriving: the E1 generation introduced a second lan mac; issue 24 required skipping BLE reports that carry no mac; matter support added a `device.json` reader. The modeled evolution is the refactor a maintainer typically reaches for at that point: stop passing open-ended keyword soup into the factory, give the factory an explicit signature that names the identity fields once, extract tiny per-transport helpers so that each transport's raw-format quirks (EUI64 without colons, byte-reversed database macs, mixed-case report macs) live in one function, and pass the gateway miio fields positionally instead of as a dict. Each step is commit-sized and individually defensible; code review of any single step sees harmless tidying.

## Normal development evolution being modeled

- The registration factory grows an explicit identity signature because open `**kwargs` invites misspelled keys and hides what a device actually needs. (Signature expansion is the classic "make the contract visible" move.)
- Per-transport row/report normalization gets extracted into small module functions while touching the bluetooth and mesh table readers for an unrelated fix — the two transports are kept symmetric.
- A dict-vs-fields change ripples into the two callers of the gateway self-registration path, which gets unpacked "one level earlier" because the dict was considered annoying at the call site.
- The deferred unknown-zigbee registration task, which already carries a device identity across a 10-second time gap, picks up the fields it needs as explicit arguments because that is the path of least resistance once the factory signature changes.

The cumulative shape this kind of incremental refactoring lands on: the identity of a device — data with one use, one meaning and one lifetime — is handed around the adapter layer as groups of parallel positional parameters, destructured tuples of __bare fields__, and per-field locals that arrive under different names at different layers, while each adapter re-implements a fragment of the same assembly work.

## Overall design

Four moves, each matching a plausible maintainer step:

1. **The factory names the identity fields.** `XGateway.init_device` replaces its `(model, **kwargs)` contract with an explicit parameter list (`model`, `did`, `dtype`, `mac`, `ieee`, `nwk`, plus a residual `**kwargs` tail), lowercases `mac` on the way in, and builds the `XDeviceExtra` mapping itself. Adapters stop building the mapping and pass the identity positionally.
2. **Per-transport identity normalizers.** `lumi.py` gains `zigbee_identity`, `ble.py` gains `ble_identity`, `mesh.py` gains `mesh_identity`. Each takes the raw row/report fields for its transport and returns a positional tuple that the discovery loop immediately unpacks.
3. **Gateway self-registration goes positional.** `XGateway.base_read_device` no longer takes the miio `info` dict: it takes `did`, `model`, `mac`, `fw_ver`, `mac2` and forwards them into the factory. The matter adapter adopts the same positional style inline; `MultiGateway.prepare_gateway` extracts the miio fields once and threads them through its firmware-capability branches.
4. **Identity crosses the task boundary as scalars.** The silabs receive path computes the unknown device's `ieee` at enqueue time and hands `did`, `ieee`, `nwk` into the pre-existing 10-second background task alongside the `uid` bookkeeping key.

## Per-location rationale

### `core/gate/base.py` — `XGateway.init_device` (shared registration factory)

What changed: signature expanded from `(model, **kwargs)` to `(model, did, dtype, mac, ieee, nwk, **kwargs)` with defaults for the address fields; the mapping construction that previously lived in every adapter now happens in one place; `mac` is lowercased here since both database and report sources deliver mixed-case values; transport-specific members still flow through the `**kwargs` tail.

Why this site: it is the single sink every transport already funnels through, so expanding it is the smallest change that makes "which fields make up a device" visible in one signature. Why this shape: an explicit parameter list reads as hygiene (callers can no longer invent keys), it keeps `XDevice(model, **extra)` and the `devices[did]` registry write untouched, and it preserves the production role — create the device, debug-log it, register it, return it.

### `core/gate/base.py` — `XGateway.base_read_device` (gateway self-registration)

What changed: signature `(info: dict)` → `(did, model, mac, fw_ver, mac2=None)`; the conditional `lan_mac` insert became an explicit `mac2 is not None` guard.

Why this site: it has exactly one caller, so unpacking the dict one level earlier looks like free tidying; the E1-generation second lan mac now reads as a first-class member of the signature rather than a conditional key insert. Production role — register the gateway itself as a device on first prepare, idempotent per `did` — is preserved.

### `core/gate/lumi.py` — `zigbee_identity` + `LumiGateway.lumi_read_devices`

What changed: a module function normalizes one `device.info` row into a positional tuple (`hex_to_ieee` conversion included); the discovery loop unpacks it into individual locals and assembles only the firmware-version part of the mapping before calling the factory.

Why this site/shape: the zigbee row is where the awkward raw knowledge lives (`mac` is actually the colon-less EUI64, `shortId` is the nwk, `appVer`/`hardVer` are firmware versions, the cloud mapping may or may not know the `did`) — knowledge a "name this transport's identity once" helper captures, which is why the helper takes seven parameters even though it returns only a tuple. After the loop destructures it, the same fields travel on as separate locals.

### `core/gate/ble.py` — `ble_identity` + `BLEGateway.ble_read_devices` + `BLEGateway.ble_process_event`

What changed: one helper covers both bluetooth transports of this adapter — the database row (whose mac is stored byte-reversed and needs `reverse_mac`) and the report payload (whose mac arrives colon-formatted and mixed-case); both callers unpack the tuple and pass the identity positionally into the factory.

Why this site/shape: the two paths genuinely share the "did, model, lowercase mac" concern, so the extracted function documents that sharing — and the identity group is now visible at both entry points. The mac-less report guard (issue 24) stays ahead of the helper call, preserving the "no mac, no device" behavior.

### `core/gate/mesh.py` — `mesh_identity` + `MeshGateway.mesh_read_devices`

What changed: bulb rows (`did`, byte-swapped DB mac, `model`) go through a lowercasing helper identical in spirit to the bluetooth one; bulb registration becomes positional; group registration switches to the same positional call shape (groups carry only `model`).

Why this site/shape: symmetry with the bluetooth table reader — the two database-table transports get the same treatment so the adapter layer stays uniform. The group `childs` bookkeeping is not identity data and stays outside the helper, unchanged.

### `core/gate/matter.py` — `MatterGateway.matter_read_devices`

What changed: two locals (`model`, `fw_ver`) followed by a positional factory call.

Why this site/shape: the smallest adapter follows the new factory contract without inventing a one-user helper; `device.json` supplies no hardware addresses, so a short field list is inherent to this transport rather than to the design.

### `core/gate/silabs.py` — `SilabsGateway.silabs_process_recv` + `SilabsGateway.silabs_process_unknown`

What changed: the receive path computes `ieee = hex_to_ieee(uid)` at enqueue time and passes `did`, `ieee`, `nwk` into the background task; the task keeps the `unknown` bookkeeping mapping keyed by `uid` and registers the device after the timeout using the scalar arguments.

Why this site/shape: the pre-existing task boundary already forces identity to survive a delay, an unknown-list removal and a "meanwhile became known" dedup test; once the factory signature changed, threading the fields it needs as explicit arguments was the path of least resistance. All production invariants remain: the 10-second delay, the dedup against an already-known `did`, and uid-keyed cleanup on removal.

### `core/gateway.py` — `MultiGateway.prepare_gateway`

What changed: extracts `did`, `model`, `mac` from the miio info once, renames the local `fw` to `fw_ver`, keeps the two firmware-based capability branches, and calls the new positional `base_read_device` (passing `info.get("lan_mac")` for the second lan mac).

Why this site/shape: it is the flow that drives every adapter, and it now re-exercises the field-by-field threading by hand — the very unpacking the self-registration used to receive as a dict. Its production role (one prepare flow orchestrating all discovery) is unchanged; the `ret` value, capability gates and the unsupported-firmware warning behave identically.

## Production role summary

| Location | Production role |
| --- | --- |
| `base.py init_device` | create + register a device for every transport |
| `base.py base_read_device` | register the gateway itself from miio handshake info |
| `lumi.py zigbee_identity` / `lumi_read_devices` | zigbee discovery from `device.info` |
| `ble.py ble_identity` / `ble_read_devices` | bluetooth discovery from the gateway DB |
| `ble.py ble_process_event` | BLE device created on the fly from a report |
| `mesh.py mesh_identity` / `mesh_read_devices` | mesh bulb and group discovery from DB tables |
| `matter.py matter_read_devices` | matter discovery from certification `device.json` |
| `silabs.py silabs_process_recv` / `silabs_process_unknown` | deferred registration of unknown zigbee devices |
| `gateway.py prepare_gateway` | the prepare flow feeding all adapters |
