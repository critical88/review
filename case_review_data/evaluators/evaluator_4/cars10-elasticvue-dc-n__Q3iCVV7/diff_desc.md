# Injection design record — cluster-connection transport rework (elasticvue)

## Maintenance motivation

elasticvue's cluster management supports four authentication modes (none, basic auth,
API key, AWS IAM signing), and the cluster record itself is deliberately nested: a
cluster is `{ name, uri, auth }`, with `auth` carrying the mode plus exactly the
fields that mode uses. During the 1.15 cycle this nesting kept getting in the way of
small, focused features that already held the *pieces* of a connection without
holding a connection-shaped record:

- the connect flow validates a form value before anything is worth persisting, so it
  has a draft, not a cluster;
- the background health refresh wants to talk to a cluster it was handed, without
  caring which other cluster fields exist around it;
- the in-app REST runner builds its own `fetch`, so it wants the header, not a
  client;
- the cluster edit dialog pushes changed form values back into the saved cluster
  list.

Sweeping over those paths, the seams were loosened so each caller hands over exactly
the values it already has, instead of wrapping or re-wrapping them into the nested
records. That removes some ceremony from the calling code. The price is that the
address of a cluster, the choice of authentication mode, and the half-dozen
credential fields now cross each seam as separate values, and every calling path has
to open the records it holds and hand the pieces over one by one.

## Evolution being modeled

This is the kind of change that lands in several small steps rather than one design
decision, which is why the changes do not read as uniform:

1. The request adapter's construction contract was widened from a nested connection
   record to loose values, because two different callers (form drafts during
   connect/test, saved clusters during background work) were building records just
   to immediately have them unwrapped again.
2. The auth-header helper followed so that the same style is used at every
   authentication decision point, including the one fetch path that never goes
   through the adapter.
3. The connection store's cluster-update action was next: the edit dialog had the
   changed values lying around, and assembling a connection-shaped record to pass
   across the store call only to unpack it inside felt indirect.
4. Every calling path was then adjusted by whoever touched that path, at different
   times and in their own local idiom — one destructures the record first, another
   reads chains off the raw ref, a third widens the auth payload into permissive
   locals. The per-site differences are copy-paste drift, not a single template.

## Overall design

The cluster-connection subsystem is the scope: the service adapter, the small
header-derivation helper, the pinia connection store, and the five composables that
either construct the adapter or derive authentication values from cluster state
(connect/test, the lazily created request client, home-cluster health, the in-app
REST runner, cluster editing). The seams receive loose values; the feeding paths
enumerate the fields of whatever record they hold. Three receiving seams were
loosened, with three different shapes, because they genuinely need different
subsets of the data:

- the adapter takes the address, the auth mode, and six credential values — the
  full surface, because it must both build the AWS signing client and derive the
  header;
- the header helper takes the auth mode and three credential values — no address
  needed, and AWS signing never needs a header;
- the store update takes name, address, auth, and the index — a persistence merge,
  where the index is a selector for the saved cluster, not part of the data being
  merged.

All flows that feed those seams then enumerate fields per call. The behavior of
every path is meant to be identical to before: same headers per auth mode, same
signed requests, same persisted clusters, same lazy client creation.

## Per-location rationale

### `src/services/ElasticsearchAdapter.ts` — adapter construction

The adapter is the single gateway every data view uses to reach Elasticsearch. Its
constructor is the natural place a full connection record used to be consumed. It
now takes `uri, authType` and the six credential fields positionally, builds the
AWS signing client from the four IAM values when the mode says so, and derives the
Authorization header from mode plus the three header-credential values. This site was
chosen because it is the one seam every consumer of a cluster must cross, and the
widest flattening (nine values) naturally belongs at the deepest seam. The AWS
branch keeps reading exactly the same values it did before, only now as parameters
instead of record members.

### `src/helpers/elasticsearchAdapter.ts` — header derivation

`clusterAuthHeader` maps an auth mode to an Authorization header and is the one
authentication decision shared by the adapter and by raw fetch paths. It now takes
the mode plus `username, password, apiKey` and keeps its mode-by-mode switch over the
tested base64 helper. The header helper is flattened one step less than the adapter
(three credential values instead of six) because header derivation genuinely touches
no IAM fields — a deliberate difference showing the seams were loosened per need,
not by template. The tested helpers around it (base64 header building, URI helpers)
are untouched.

### `src/store/connection.ts` — cluster update action

`updateCluster` merges an edited cluster into the saved list: same merge into the
old record, same auth-cleanup on save. It now takes `name, uri, auth, index`
positionally — the three values the edit dialog produces, plus the index selecting
which saved cluster to merge into. Keeping the index as a separate positional
argument records that it is a selector, not data; and keeping `auth` whole here
while other seams take it apart is another intentional asymmetry — at this seam the
caller holds the auth record already, and the persisted shape is per-cluster.

### `src/composables/ClusterConnection.ts` — connection test

`testConnection` services the "test this connection" affordance on the connect
screen. It destructures the form value once into `uri` and `auth`, widens the auth
payload into a permissive local view, and hands the adapter the address, the mode
and seven fields. The destructure-first style was chosen because the flow also
validates the form and reports two separate request states around the call; the
flattened call now reads as the longest argument list in the app.

### `src/composables/ClusterConnection.ts` — connect

`connect` is the flow that, after a successful test, assembles and stores the full
saved-cluster record (name, version, distribution, flavor, status, uuid, auth). It
reaches the same adapter seam by reading `formCluster.value` chains directly and
widening the auth payload into a local named `credentials`. This second site in the
same file intentionally does not reuse the first site's style — two passes touched
these flows, and the connect path deals with the active cluster record afterward,
so it was written against the raw ref it continues to use.

### `src/composables/CallElasticsearch.ts` — lazy request client

`callElasticsearch` lazily creates and memoizes the single shared request client
against the currently active cluster on first use. The site opens the active
cluster's auth payload into a permissive local view and passes address, mode and
seven fields to the adapter. This is the hot path carrying every index, snapshot and
search request, so it is the most load-bearing of the feeds; the laziness and the
initial ping are preserved exactly.

### `src/composables/components/home/ClusterHealth.ts` — two health paths

Two functions share this file and were flattened the same way but from different
sources, which is why both feed the seam independently:

- `checkHealth` is the interval-driven home-screen refresh. It receives a saved
  cluster record (with status, version and loading state), opens only its
  address/auth parts, and constructs an adapter for a ping plus cluster-health
  call. Its parameter stays a full cluster record because the caller iterates the
  saved cluster list, but internally it now takes that record apart.
- `checkClusterHealth` is a standalone status helper that receives a bare
  connection record and returns a status string. It, too, now opens the record
  internally. The split between an outer record type and an internal field-by-field
  handover at both sites is the drift that accumulates when a helper seam loosens.

### `src/composables/components/rest/RestQueryForm.ts` — in-app REST runner

The REST tab builds its own `fetch` rather than going through the adapter (custom
request body, custom response rendering), so it needs the authorization header
directly. `sendRequest` now takes the active cluster's auth mode and payload apart
right at the fetch site and calls the header helper with three loose credential
values. This is the only seam feeding header derivation rather than the adapter,
and it was chosen because it bypasses the client entirely — the last path where
the connection pieces had to be walked manually.

### `src/composables/components/clusterselection/EditCluster.ts` — cluster save

`saveCluster` in the edit dialog pushes the edited values into the store: name,
uri, auth, and the index supplied by the dialog's props. It was chosen because it
is the caller that made the store action's loosening feel worthwhile — the dialog
holds changed form values, and under the previous contract it assembled a partial
connection-shaped record only for the action to unwrap it again. Success message
and page reload behavior are unchanged.

## Structural variation, deliberately kept

The sites are intentionally not uniform, mirroring how this pattern actually grows:

- two feeding styles — destructure-first in the test flow, raw member chains in the
  connect flow and elsewhere;
- two local names for the widened auth payload (`authData`, `credentials`) across
  the sites, because they were not written together;
- per-site widened typings of the discriminated auth payload (a small inline
  option-record at each site) — the price of reading credential fields that are
  only valid for one mode at a time;
- three different flattened shapes at the three seams (9 values, 4 values, 3 values
  plus a selector), matching what each seam actually consumes;
- one seam (`checkClusterHealth`) that still receives a whole record and opens it
  internally, next to a sibling that receives a bigger record and opens it too —
  the loosening shows up inside, not in exports.

The vue components, the tested helper implementations, the type definitions of the
cluster/connection records themselves, and the persisted storage format are all
outside this sweep.
