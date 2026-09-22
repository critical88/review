# Injection design record — nginx-ui site configuration backend

## Maintenance motivation

nginx-ui started as a compact Flask prototype: a handful of blueprint endpoints in
`app/api/endpoints.py` talking straight to the filesystem with `os.listdir`,
`io.open`, and `os.rename`, deriving display names and enabled-state from raw
file names inline. As the project gained users, the maintainers wanted to reuse
this logic outside the request path — a CLI, a REST facade, background
consistency jobs — and needed one shared representation of "a site configuration
file" instead of that knowledge living only in endpoint bodies. At the same time
the number of per-endpoint surprises (a site file is sometimes `.conf`, sometimes
`.conf.disabled`; timestamps must come from the file mtime; matching is
prefix-based) had grown past what a single long endpoints module could carry, and
reviewers asked for the HTTP layer to stop knowing filesystem details.

## Modeled evolution

The diff models the first half of that refactoring as it is usually written in
practice. A small site record type is introduced as the shared value object, and
the endpoint module is decomposed into focused collaborators: a registry that
turns the configuration directory into records, an editor that applies writes, an
overview builder that assembles the domain cards, and the blueprint endpoints
reduced to HTTP translation. This is the natural intermediate stage of an
in-place extraction: the *data* has been pulled out of the endpoints and unified
behind one record type, but the *rules that operate on that data* — the `.conf` /
`.conf.disabled` convention, where a record's file actually lives, when its
content is loaded, which suffix carries the enabled state, what display name and
timestamp a record yields — have stayed behind in the modules that already owned
them, now operating over the new record's fields. Together with the reshuffle,
the site-configuration behavior itself gets a dedicated test suite under `tests/`
covering the domains listing, the single-domain page envelope, and the create/
update/delete/enable lifecycle, so that the extraction work is guarded while it
proceeds — a customary companion change for this kind of restructuring.

## Overall design

Five production files participate:

- `app/api/model.py` (new): the `Site` record — `config_path`, `filename`,
  `content`, `modified`. Deliberately a plain data carrier: the maintainers'
  intent was to first stabilize one representation of a site file everywhere in
  the codebase, then grow behavior onto it.
- `app/api/registry.py` (new): `SiteRegistry` owns turning the configuration
  directory into records (`scan`) and finding the record answering a given name
  (`find`), including loading the file content it matched.
- `app/api/editor.py` (new): `SiteEditor` owns the write side — creating new
  disabled sites, applying updated content, removing, and switching the
  enabled/disabled state of the files behind records.
- `app/api/presenters.py` (new): `DomainOverview` owns the read model of the
  domains page, turning records into the card list and the enabled-site names
  the template expects.
- `app/api/endpoints.py` (rewired): the blueprint endpoints now translate HTTP
  to the three collaborators; the configuration-file endpoints under
  `/config/<name>` are untouched.

`tests/` (new) pins the observable behavior of the whole surface: app/factory
details, index/config rendering, the domains overview, and the full domain
lifecycle.

## Per-cluster rationale

### The site record (`app/api/model.py`)

Site and shape: the record carries exactly the four facts every collaborator
needs — where the configuration directory is, the raw file name, the file
content, and the modification time. It was placed in its own module (rather than
inside the registry module) because the registry, the editor, and the overview
all construct or consume it. It has no behavior on purpose at this stage of the
modeled history: the extraction was data-first, and the file conventions were
not yet moved onto it.

### The registry (`app/api/registry.py`)

The registry keeps the responsibility it inherited from the original
`get_domain`/`get_domains` bodies: reading the directory and matching names.
`scan` filters to regular files (as the original does) and stamps each record
with `os.path.getmtime` translated through `datetime.fromtimestamp`, mirroring
the original timestamp treatment. `find` keeps the original lookup semantics:
name matching is `startswith` on the raw file name, and content is loaded by the
finder once a match exists — including the original early return on the first
matching file. The registry fills the content field of the record itself rather
than teaching the record to load, because the maintainers treated the record as
a value to be filled in by whoever owns the loop; the `startswith` convention
stays where listdir is performed.

### The editor (`app/api/editor.py`)

The editor concentrates the write paths that used to be inline in
`post_domain`, `put_domain`, `delete_domain`, and `enable_domain`. Each method
mirrors the exact file semantics of the original implementation: creation
writes the rendered template as `<name>.conf.disabled` (the original convention
that newly created sites start disabled); update rewrites every matching file
with no early exit (the original looped over all files); removal deletes the
first matching file and re-checks existence in a same-directory guard, returning
whether it acted; state switching renames either by stripping the last suffix
(`enable=True`) or appending `.disabled` (`enable=False`), again without early
exit. The editor derives each on-disk path from the matched record's directory
and file-name fields, keeping all file-naming knowledge inside the write
collaborator — that is where it was easiest for the maintainers to put it while
the record had no behavior.

### The overview builder (`app/api/presenters.py`)

`DomainOverview.cards` is the extract of the original `get_domains` body: it
splits each record's raw file name at the last dot, reads a `conf` site as
enabled under its own name and a `conf.disabled` site as disabled under the
name-stripped-once-more variant, takes the card timestamp from the record, sorts
by displayed name, and returns the available/enabled pair the template renders.
The two-level name handling was kept character-for-character because the page
layout (available vs. enabled groups) encodes product decisions; the derivation
remained in the view-model builder reading the record's fields.

### The endpoint rewiring (`app/api/endpoints.py`)

The blueprint keeps its route table public and the bodies become translation:
`get_domains` delegates to the overview builder; `get_domain` asks the registry
for the record, and then reads only the two values it renders into the template
(the enabled flag derived from the file-name suffix, and the stored content);
`post_domain` creates through the editor and keeps its original success/error
response shape including the exception path; `delete_domain`, `put_domain`, and
`enable_domain` hand over to the editor with their original status codes and
JSON bodies. The configuration endpoints (`get_config`, `post_config`) are
deliberately not rewired: they operate on a different directory and produce HTML
responses, so they were out of scope for the site-record extraction.

### The test suite (`tests/`)

The suite exercises the app factory and configuration wiring, the rendered
index/config pages, the domains overview (site cards by name, enabled vs.
disabled grouping), the single-domain envelope, the create/update/delete/enable
lifecycle over a temporary configuration directory, and the WSGI entry point.
It belongs to this change set because the reshuffle above is exactly the kind of
step that silently changes file conventions and response envelopes; the tests
state the conventions the maintainers intended to freeze before continuing the
refactoring.

## Deliberate red herrings

The single-domain endpoint and the creation path are intentionally quiet: the
endpoint renders the two values it is handed rather than re-deriving them, and
creation composes a fresh path through the registry member without a record in
scope. The configuration endpoints and templates carry no site-record logic at
all. These give the reshuffle its realistic texture: not every collaborator
that mentions a record participates equally in the convention problem.
