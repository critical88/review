# Refactor: misplaced host-decision responsibilities on the xxh connector

## What was observed

`xxh` is migrating its host probe from a raw `key=value` mapping to a
structured, mapping-compatible host-probe result type. To make the main
connection flow read more linearly, several host-decision branches that
previously ran inline in that flow were pulled out into small helper methods
on the `xxh` connector. Those helpers receive the new host-probe result value
object (and, for two of them, the parsed ssh destination and the command-line
option namespace) and decide things about the host almost entirely by
inspecting the object passed in; they barely touch the connector's own state.

The host-probe result, the parsed destination, and the option namespace are
the objects that own the data these decisions read. In its current form the
connector is acting as a data-less dispatcher: it receives an object, reads
its fields, and decides on that object's behalf. The structured host-probe
result type is an in-repo, extensible value object introduced for this
migration; the destination result and the option namespace are standard
library / argparse types instead.

## Scope and responsibility to address

Investigate the host connection setup path on the `xxh` connector — the
probe-result consumption, the ssh destination/credential resolution, the
install-prompt predicate, and the writability/transfer-tool gating — and
address every helper that decides over a foreign object's fields instead of
asking that object the question. The in-repo host-probe result type is an
owner you may extend; the destination result and option namespace are
external types you must not subclass or monkey-patch. Address all instances of
the same misplaced-responsibility pattern, not merely one.

## Required outcome

Place each host decision with the object that owns the data it reads, within
the compatibility boundary below, and remove any connector helper whose
responsibility has been relocated or that only inspects a foreign object's
fields between the connector and a type it does not own. The observable
connection setup sequence and every host-decision output (completeness
warnings, the writability consent prompt, the transfer-tool availability
warning, the install prompt, and the resolved ssh port/login) must be
unchanged.

## Compatibility boundary

- Keep the `xxh` connector's construction and public entry behavior;
  `parse_destination`, `b64e`, `get_current_shell`, `get_config_filepath`,
  and the module-level `sigint_handler` must remain.
- `get_host_info` must still return a host probe result that supports legacy
  subscript key access (callers that do `host_info['xxh_version']` and alike
  must keep working) in addition to named-field access.
- Do not subclass or monkey-patch the argparse option namespace or the
  urllib destination parse result type; only the in-repo host-probe result
  type may be extended.
- The repository's smoke test must remain green.
