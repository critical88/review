# Cluster connection plumbing: every caller now has to know every field

I wanted to add one small connection-level option to elasticvue and gave up
halfway, because reaching a cluster currently means threading a pile of
individual values through every layer by hand.

The store keeps each saved cluster as one record: a name, a URI, and an auth
part that holds the auth mode together with the fields that mode uses. That
is also exactly how the settings dialog, the migration code and the
predefined clusters treat a cluster. But at the point of *use*, everything
falls apart. A few of the lower-level seams now take a cluster's pieces
positionally — one takes the address, an auth mode, and then six or seven
separate credential values in a row. As a result, every code path that
touches a cluster connection opens up the record it is holding and hands
the fields across one argument at a time. Several of those sites first
loosen the auth value into a little "anything goes" property bag so they
can read fields that only exist for some auth modes.

Concretely, this shows up all over the connection flow: testing a
connection from the add-cluster screen, saving a newly connected cluster,
the request client that the data views lazily create for the active
cluster, the periodic home-screen health refresh and its standalone status
helper, the REST tab (which builds its own request and needs the
authorization header directly), and the cluster-edit dialog writing back
to the store. Same story everywhere: repeated little value lists that have
to stay in sync with whichever auth mode happens to be active, and casts
that quietly let every field be optional at every site. The type layer
already knows better — a basic-auth cluster has username/password, an API
key cluster has a key, an AWS one has key/secret/token/region — but none of
that survives down at these seams.

Please rework this so that connection data moves through the connection
layer as the bundled value it already is everywhere else. If a boundary
needs a cluster's address together with its authentication, let it receive
that grouping; if a spot genuinely needs only the auth part, the auth value
on its own is fine too. Work through the whole connect/test/request/
health/REST/edit path, not just the most obvious construction call — I
keep finding more of these hand-me-field-by-field sites. Keep the auth
modes properly typed (each mode should carry its own fields, not
everything-optional everywhere), and clean up whatever loose ends the
value-lists and casts leave behind.

Everything observable must stay exactly as it is:

- for every auth mode the same requests go out as before: no authorization
  header when the mode is `none`, unchanged headers for basic auth and API
  keys, and unchanged signed requests for AWS IAM clusters;
- the request client for the active cluster is still created lazily on
  first use and pinged before the first request;
- saving an edited cluster still merges back into the saved cluster list,
  and the on-disk representation of saved clusters must not change —
  existing sessions and the migration code read it;
- the composable wiring used by the cluster screens keeps its current
  shape (the Vue components are not part of this), and the small tested
  helpers for basic header building, URL building, cluster migration and
  predefined clusters stay as they are, with their suites green.

I would expect this to make the "add one more auth mode or connection
option" chore a one-place change again.
