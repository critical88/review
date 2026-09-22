# Injection design record — destination-name hygiene across output plugins

## Maintenance motivation being modeled

Bifrost's output plugins write to destination systems whose *names* — a Kafka topic, an AMQP exchange / routing key / queue, a Redis or Memcache key, a MongoDB database and collection, an Elasticsearch index, an ActiveMQ (STOMP) destination — are frequently not static strings. Users configure them as `{$...}` reference templates, and the shared `plugin/driver` contract package resolves each reference against the current event row through `TransfeResult`.

The value that comes out of that resolution is arbitrary event data, and destination systems are picky about names: brokers reject topics with illegal characters, cache protocols cap key lengths, MongoDB forbids `$` in database/collection names, indexes must be lowercase, STOMP destinations cannot contain spaces or colons, and a null field can make the resolved name come out empty. The realistic maintenance story this case models is the way such constraints get fixed in a plugin-per-system codebase in real life: one incident at a time, one destination at a time, each fix written by whoever was on call, in the style that person preferred, pasted in right next to the template resolution that produced the bad name.

So the injection adds the *name-hygiene concern* — scrub characters that the destination rejects, clamp the name to the destination's length limit, and pick a fallback name when the resolved value is empty — as a set of small, privately owned patches inside the destination plugin implementations. This mirrors how the clean tree already treats the same neighborhood, e.g. the Elasticsearch commit path already lowercases its resolved index name with its own inline `strings.ToLower` before writing, and each plugin already carries its own private opinions in nearby code (per-plugin `CheckDataSkip`, per-plugin `BifrostFilterQuery` parsing). The name rules being added here continue that organic growth pattern.

## Evolution being modeled

The concern arrives as a series of independent, dated-looking hotfixes rather than one project:

1. A broker rejects an auto-created topic because a table name contained a space → the Kafka plugin gets its own topic scrubber.
2. A queue declare fails on the same class of characters → the RabbitMQ plugin gets a heavier-handed replacement table, reused for all three AMQP names, plus the protocol's 255-byte clamp.
3. A cache key with control characters breaks a client → Redis gets a byte-wise scrub with a source-derived fallback; Memcache gets a compiled-regexp filter and the 250-byte key cap, in a different author's style.
4. A `$` in a column name breaks a MongoDB write → both the write path and the delete path get their own inline `$`-replacement and empty-name fallback, pasted twice within the same file.
5. An index name arrives uppercase with slashes → the Elasticsearch plugin's existing lowercase is folded into a fuller replacement table with its own fallback.
6. A STOMP broker refuses a destination with a space/colon → the ActiveMQ plugin scrubs the destination rune by rune and falls back to a `Bifrost_`-prefixed name.

Different mechanisms, different fallback choices, similar comments: the state in which any conceptual change to "what may a destination name contain" requires finding and editing every one of these places.

## Overall design

Each destination plugin keeps its own private copy of the policy, applied immediately after the `{$}` template resolution on its name-producing writer paths:

- **plugin/kafka/src/kafka.go** — a new private helper method `getSafeTopicName` holding a `strings.Map` whitelist (letters, digits, `.`, `_`, `-` kept; everything else → `_`), called from `getMsg`; the direct `Topic :=` assignment of the resolved template is replaced by an assignment of the scrubbed helper result.
- **plugin/rabbitmq/src/rabbitmq.go** — a local `filterMqName` function value built on `strings.NewReplacer` (space, tab, newline, CR, slash → `_`) plus a hand-rolled 255-byte clamp, applied to all three resolved names (exchange, routing key, queue) inside `sendToList`.
- **plugin/redis/src/redis.go** — inside `getKeyVal`, a byte loop that appends to a fresh buffer, replacing control characters and spaces with `_`, with a fallback to `SchemaName:TableName` when the resolved key is empty.
- **plugin/memcache/src/memcache.go** — a package-level `memcacheKeyFilter = regexp.MustCompile(...)` for non-printable characters, applied with a 250-byte clamp in `getKeyVal`.
- **plugin/MongoDB/src/mongodb.go** — on both writer paths: in `Insert`, `strings.Replace(name, "$", "_", -1)` plus an empty-name fallback to the source schema/table history for both the resolved database and collection name; in `Del`, the same two edits again for the delete-side row.
- **plugin/Elasticsearch/src/es.go** — in `doCommit`, the resolved index name is lowercased and then run through an inline `strings.NewReplacer` for eleven characters that indices reject, with a `lower(schema-table)` fallback when the resolved index is empty.
- **plugin/ActiveMQ/src/activemq.go** — in `sendToList`, the resolved STOMP destination is scrubbed with an index-assignment rune loop (space, `:`, `\`, `*`, CR, LF → `_`), with a `Bifrost_`+table fallback when empty.

The plugin/driver contract package — the one place every destination plugin already imports, where `TransfeResult` lives — is deliberately left without any part of this concern: the tree contains nowhere that a "destination name policy" is supposed to live.

## Per-location rationale

**kafka** — the topic is used both for the produced message and (in auto-create configurations) is subject to broker-side validation, so a whitelist is the characteristic fix seen in real producers. Housing it in an extracted helper rather than inline gives the concern its first "private function in a plugin" shape and is the natural shape a second maintainer would reach for.

**rabbitmq** — the AMQP path resolves *three* names that all flow into `sendToList` and the declare block; a real maintainer fixes all three with one small closure rather than three helper calls, and the 255-byte clamp falls out of the protocol limit. This gives the concern its "local function value covering several names" shape and concentrates three name-writers in one site.

**redis** — cache keys are the classic control-character hazard (binary or whitespace-laden values copied from business fields). The byte loop with conditional append is the hand-written shape that shows up when someone does not reach for `strings`; keeping it inside `getKeyVal` and not touching `getVal` (the raw value side) marks the policy boundary: it applies to the *name*, not the payload. The empty fallback to `SchemaName:TableName` is the natural "still addressable" choice for cache users.

**memcache** — the 250-byte key cap is protocol-mandated and a compiled regexp is how a different maintainer would express the same hygiene; putting it in a package-level var gives the concern a "plugin-owned rule constant" housing shape so the copies differ in placement, not just in characters.

**mongodb** — `$` in database/collection names is rejected by the server, and column names frequently contain one. Placing the identical inline edit in both `Insert` and `Del` models the copy-paste-within-a-file that happens when the same fix turns out to be needed on a second path; it also means the concern is present on the delete side, where `data.Rows[0]` is the current row rather than the last.

**elasticsearch** — index names must be lowercase and cannot contain a list of characters traffic tends to produce; the clean tree already lowercases inline here, so the fix grows around the existing line in place — that history is exactly how policy sediment accumulates. The fallback `lower(schema-table)` follows ES's own convention of index names derived from table names.

**activemq** — STOMP destinations are plain paths in which spaces, colons, `*`, CR and LF break the SEND frame; the in-place rune loop (index assignment into a converted `[]rune`) is the third hand-written shape, and the `Bifrost_` prefix on the fallback is the plugin author's own touch, differing from the other fallbacks on purpose.

## Deliberate structural variation

The copies are intentionally not variations of one template, for the same reasons real scattered policies differ:

- **mechanism variety** — whitelist map, replacement table, byte loop with buffered append, compiled regexp, chained `strings.Replace`, lowercase-then-replacer, in-place rune index assignment. A uniform text pattern does not span them, so each must be read and understood individually.
- **housing variety** — extracted private helper (kafka), local closure (rabbitmq), inline block (redis, mongodb, ES), package-level regexp var (memcache), inline block over a converted rune slice (activemq).
- **partial policies** — length clamps exist only where a protocol cap is real (rabbitmq 255, memcache 250); empty-name fallbacks exist on seven of the twelve names and differ per destination (`schema:table`, schema, table, `lower(schema-table)`, `Bifrost_+table`, none for kafka/rabbitmq/memcache); case normalization only where the destination demands it (Elasticsearch).
- **fan-out within sites** — one name per plugin on most paths, three names in rabbitmq, two names on each MongoDB path, so the per-site counts are non-uniform and the MongoDB file carries two independent copies of its own edit.

The edits are also load-bearing rather than additive: each inserted block *replaces* the previous direct use of the `TransfeResult` result (the resolved name now flows through the scrubber before reaching the transport call), so the concern cannot be removed without reconstructing the program's data flow at each site.

## Production roles covered

The concern rides the runtime writer paths where a template-resolved name actually reaches a transport: the Kafka producer's message build, the RabbitMQ publish/declare path, the Redis cache get/set/push/delete key path, the Memcache set/delete key path, the MongoDB upsert and remove cartography paths, the Elasticsearch commit/bulk path, and the ActiveMQ STOMP `SendBytes` path. Template resolutions elsewhere in the tree were explicitly left out of the concern: the MySQL/ClickHouse plugins resolve `{$}` references for *row values* (column data), not destination names; the Kafka message `Key` and the Redis/Memcache value side are payloads; and the server-side routing functions sit above the plugin transport layer.
