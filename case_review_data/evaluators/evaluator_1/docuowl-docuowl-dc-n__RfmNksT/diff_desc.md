# Injection design record — document pipeline identity handling

## Maintenance motivation

docuowl's documentation tree is built from three cooperating packages:
`frontmatter` parses YAML frontmatter into a small `Meta` record (title and
ID, with the ID defaulted from the title and slugified), `fs` walks a
documentation directory into `Group`/`Section` entities, and `fts` indexes
sections for client-side full-text search. Templates in `parts` render the
sidebar and content from those entities.

The evolution modeled here is the kind of cleanup pass that shows up in real
projects: a maintainer wants the tree packages to stop depending on
`frontmatter.Meta` as a shared value object. The stated motivation is that
identity is "just two strings" and that constructors should accept plain
values so that trees and indexes can be built without coupling the `fs` and
`fts` layers to the YAML-parsing package's type. The developer therefore
unwraps the record everywhere it used to be carried whole and hands its two
fields through the pipeline as individual `string` parameters instead.

## Overall design of the change

The change unwraps document identity and re-plumbs every exchange that used to
carry the record as a whole:

- Parsed identity leaves `frontmatter` as two parallel strings produced by a
  dedicated defaults step, instead of being finalized on the record itself.
- `fs` entities store two adjacent string fields (`Title`, `ID`) rather than a
  reference to a metadata record; a small shim reassembles that record on
  demand inside `Meta()` so consumers outside the pipeline keep compiling
  unchanged.
- Constructors take the two strings as leading parameters, and the walker
  threads them from parsed metadata into each entity.
- `fts` derives its compound slug from the same bare pair plus the parent
  chain, keeping its own fallback helper for the ID-empty case.
- The interface's former parent-walk helper disappears; compound identifiers
  are computed by walking the new parent fields directly.

Error paths, content parsing, side-note loading, search serialization, and
all rendering behavior are deliberately left as they were.

## Per-location explanations

### `frontmatter/frontmatter.go` — parsed identity defaults

`ExtractFromLines` used to finish the parsed record in place (default the ID
to the title, slugify it). The change extracts that into
`applyIdentityDefaults(title, id string) (string, string)`, and the caller now
receives the two strings back and assigns them onto the struct fields. This
site was chosen because it is the pipeline's entry point for identity data:
whatever leaves `frontmatter` defines the shape every later stage sees. The
production role is parse-time identity defaulting and it participates by
introducing the two-string signature that the rest of the pipeline adopts.

### `fs/fs.go` — entity storage, construction, and the consumer shim

Three clusters were changed in this file:

- **Storage (structs, and methods `Meta`/`CompoundID`).** `Section` and
  `Group` previously held one `Metadata *frontmatter.Meta` reference each.
  They now hold adjacent `Title string` and `ID string` fields. Because every
  consumer of the `Entity` interface reads `Meta()` (sidebar rendering,
  anchors), `Meta()` now rebuilds a `*frontmatter.Meta` from the two fields on
  each call via `metaFromIdentity(title, id string)`, returning `nil` when
  both strings are empty so the previous "metadata-less" signal survives.
  This shim keeps `parts` and the rest of the rendering pipeline source- and
  behavior-compatible while the storage layer no longer keeps the record.
- **Construction (`makeSection`, `makeGroup`).** Tree building previously
  wrote keyed literals directly; it now funnels through two small
  constructors whose leading parameters are the unpacked identity pair. The
  walker's group branch reads parsed metadata, copies the two strings out
  with an explicit nil check, and hands them to `makeGroup`; the content-file
  branch hands the parsed strings to `makeSection` together with content,
  side notes, and the parent; a group's inline content becomes a section via
  the same constructor with empty identity strings. These constructors are
  the reason the package boundary now exchanges identity as two parallel
  strings rather than one record reference.
- **Hierarchy derivation (`CompoundID`).** The old implementation walked the
  `Entity` interface upward using a parent-entity accessor and a nil-guard
  helper. With the record reference gone from the structs, the accessor and
  its helper were dropped, and both `Section.CompoundID` and
  `Group.CompoundID` walk the typed `Parent` fields directly, collecting each
  ancestor's ID string. The production role — joining the ID chain from node to
  root — is unchanged; only the plumbing type changed.

`fs` is the natural second site: it is where identity is stored and where both
consumers (`parts`, `fts`) attach, so its storage shape defines what every
other exchange looks like.

### `fts/fts.go` — index-time identity derivation

`AddSection` previously received the section and followed its metadata
reference; it now pulls `sec.Title` and `sec.ID` out and hands them to
`slugifySectionName(title, id string, parent *fs.Group)`, which reconstructs
the compound slug. The ID-empty fallback that the storage record used to
encode is rebuilt locally by `identityFor(title, id string)`. Word frequency
handling also changed shape: instead of taking the section, it takes the
`content`, `sideNotes`, and `hasSideNotes` values separately. The roles
performed here are index-time identity handling and index-time compound
naming; the site was selected because `fts` is the third genuine owner of
identity data (its slugs are indexed for search) and duplicate derivation of
the ID fallback is exactly what an unwrapped pipeline drifts into.

## Why not further

Three genuinely related owners of document identity exist: the parser that
produces it, the tree that stores it, and the index that derives from it. The
remaining packages were examined and left out as they carry no document
identity: `watch` serves rendered files over HTTP, `markdown` renders text
without identity, `slug` and `fts/lang` are pure value tables, and `parts`/`cmd`
consume the already-published `Entity` interface (kept compiling unchanged via
the `Meta()` shim). The content-text triplet handled alongside identity in the
constructors is part of the same unwrap-and-pass-through edit at the two
sites that build sections; the identity strings are the portion that also
spreads across package boundaries and into derivation logic.
