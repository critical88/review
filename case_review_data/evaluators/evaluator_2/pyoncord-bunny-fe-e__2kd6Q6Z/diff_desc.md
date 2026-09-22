# Injection design record — pyoncord-bunny-fe-e

Repository: Bunny (pyoncord/Bunny), TypeScript, pinned revision
`ccc33ef2403d4016ff336814d787df0379d06efe`.

## Maintenance motivation

Bunny's addon settings pages had been converging on two shared UI families:

- `AddonCard` / `AddonPage` components, which render a uniform card
  (header label, sublabel, description, toggle, overflow menu) for anything
  addon-shaped;
- the unified plugin model, where the Plugins page builds one object per
  installed plugin (from a Bunny manifest or a Vendetta plugin) that carries
  both the plugin's data and the page-facing behaviors (badges, enabled
  state, toggling, sheet resolution, settings-component resolution).

The list surfaces that still did their own fragment preparation inline were
the theme list card, the plugin list card and info sheet header, and the
Plugin Browser repository cards. The theme card in particular had grown into
the largest `AddonCard` consumer in the repository: label/sublabel
derivation, a selection radio, an apply/clear switch, and three overflow
actions live in one component body.

## Evolution being modeled

The change is modeled as the step a developer takes when bringing those
three remaining list surfaces onto the card-component idiom: each page gets
a presentation model next to its card, the card component becomes a thin
view over that model, and the fragment expressions that used to sit inline in
the components are carried over verbatim into the model methods. This keeps
the visible behavior of the pages unchanged while normalizing the structure
of the three cards, and it mirrors the modeling idiom the Plugins page
already uses for installed plugins.

## Overall design

* `ThemeCardModel` — a class that receives the stored theme record in its
  constructor and exposes the fragments the Themes page card renders: the
  description parts, the safe-mode-aware toggle kind, the radio state read
  from the installed-themes map, the apply/clear wrapper around the selection
  API (with the same error log path), and the three overflow actions (refetch
  with success/failure toasts, copy-id with clipboard toast, destructive
  delete with confirmation that reverts an applied-but-removed theme).
* `ThemeCard.tsx` — rewritten as a thin view: it keeps the storage-proxy
  subscription and the removed-state guard, then hands every fragment to
  `AddonCard` from the model.
* `PluginCardModel` — a class over the unified plugin model exposing the
  card headings (title, icon resolved through the asset registry, joined
  author names) and the author roster (display names, author identifiers,
  total) shared by the plugin list card and the plugin info sheet header.
* `PluginCard.tsx` — the card context now also carries the model; the card
  title and authors rows consume its headings; the description, actions, and
  switch rows are untouched.
* `TitleComponent.tsx` — the info sheet header builds its user-fetching
  store hook from the roster identifiers and renders the roster title,
  names, and total instead of re-deriving them.
* `BrowseablePluginModel` — a class over a fetched repository manifest
  exposing the display text (title, byline with the "Unknown" fallback,
  version label, description) and the manifest identifier.
* `PluginBrowser/index.tsx` — the repository card renders from that model
  and keys its trailing buttons off the model identifier; the rest of the
  page (query flow, install/remove flows, alerts) is untouched.

## Per-location rationale

### Themes page list card

**`ThemeCardModel.ts` (new).** The Themes page card is the most stateful
`AddonCard` consumer: it reads display data out of a stored theme record,
selection state out of the global themes map, and mutates selection through
the themes API. Concentrating those fragment preparations in one class gives
the page a single place where the card's data contract lives, matching the
role the unified plugin model plays for the Plugins page. The method bodies
are the exact expressions previously written inline in `ThemeCard.tsx`,
so every fallback (`authors ? … : ""`, `?? "No description."`), toast,
and the `selectAndApply` try/catch behavior are carried over unchanged.

**`ThemeCard.tsx`.** Becomes a thin view over the model: it retains the
storage-proxy subscription that drives re-renders, the removed-state guard
from the delete flow, and passes the model fragments to `AddonCard`. This is
the shape the Fonts and Plugin list cards already use — a component that
subscribes and renders, with derivation nearby rather than inline.

### Plugins page card and info sheet

**`PluginCardModel.ts` (new).** Both the plugin list card and the plugin
info sheet header format the same plugin data: which title and icon to show,
how author names join, which author identifiers to fetch users for, and how
many authors there are in total. Preparing those fragments once, over the
unified model, keeps the two surfaces from diverging the way they had begun
to (the card joined names inline; the sheet mapped identifiers and totals
inline). The heading expressions match the card's previous inline code,
including the icon asset-registry lookup and the optional-chaining author
mapping.

**`PluginCard.tsx`.** The card context value now holds the model alongside
the plugin and search result so the title and authors rows stop re-running
heading-formatting per row; description, actions, and the enable switch are
unchanged.

**`TitleComponent.tsx`.** The Flux store hook now drives from the roster's
identifier list, so the fetch-and-read loop and the avatar pile receive the
same roster slots the sheet title uses. The author-text chips keep reading
the authors array directly; only the derived fragments moved.

### Plugin Browser repository cards

**`BrowseablePluginModel.ts` (new).** Repository manifests are a distinct
display shape (display name, authors, version, description) with a byline
fallback for missing authors, rendered by a page that otherwise deals with
repository maintenance rather than presentation. A presentation model gives
those cards the same fragment contract the other list surfaces have, while
the id remains raw for the trailing buttons.

**`PluginBrowser/index.tsx`.** The repository card requests its display text
and identifier from the model and no longer destructures the manifest
inline, aligning it with the card style of the other addon pages.

## Behavior carried over

All expressions moved into the new model methods are the ones previously
written inline at their consumers, with unchanged fallbacks, string
arguments, toast and confirmation flows, and error handling; the storage
subscriptions (`useProxy` on the theme record, the Flux store hook in the
sheet header) remain at the view layer exactly as before. No data schema,
storage key, component props, or public API of existing modules changed.
