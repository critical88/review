# Injection design record — react-tags input-handling absorption case

## Maintenance motivation

`react-tags` funnels three user entry points into one tag state machine: committing a
typed tag on a separator keypress, committing a batch of tags pasted into the input,
and committing a suggestion picked from the dropdown. An edit mode reuses the same
commit machinery to mutate an existing tag, and every removal from the keyboard or
from a row's remove control has to publish focus restoration and a screen-reader
announcement. Because these flows share so much state (query, selection, editing
index, tags, error), the library keeps a small set of sub-methods — the shared commit
method, the shared removal method and its announcement builder, the tag-limit
predicate, the input reset helper, the module-level separator translation in
`utils` — and routes each entry point through them.

The realistic failure mode for such a hub is *hot-path absorption*: a maintainer
fixing one entry point (say, the paste path needing its own per-entry limit
semantics, or the keydown path needing to be stepped through under a debugger)
finds it faster to copy the shared body directly into their handler, shape the
copy's control flow to the caller, and leave the original behind, than to
parameterize the shared helper for all callers at once. Each copy is defensible
in review — it makes that one handler read self-contained — but the copies then
drift apart on the next gate-condition change, and the shared originals rot with
no callers left.

## Modeled evolution

The diff models the endpoint of that creep rather than a single edit:

1. the keyboard handler first absorbs the commit pipeline (so a keypress can be
   followed end-to-end in one place) and, soon after, the removal bookkeeping in
   its backspace branch (so the two branches of the same handler tell one story);
2. the paste handler follows for its per-entry semantics: the limit guard, the
   input reset, the delimiter translation it used to borrow from `utils`, and a
   per-entry copy of the commit pipeline inside the deduplication loop;
3. the suggestion-click handler picks up the same commit copy, since it is the
   smallest of the entry points and reviewers at that point are past caring;
4. interaction/visual work on the rendered rows comes next: the edit-mode input's
   focus/change/blur contracts, the remove-control behavior and the tag
   click-through are carried directly by the JSX-attribute arrows so the row
   markup can be reviewed without leaving the tree;
5. last, the suggestion list's own render pass absorbs its two markup helpers,
   leaving row assembly, label highlighting and the active-row class in one
   visible place in the component body.

At that endpoint, the higher-level orchestration of every entry point is mixed
with copied lower-level implementations, and absorbing one copy reshapes the
control flow of each site differently (guard branches instead of early returns,
loop-continues instead of function exits).

## Overall design

- Every copied body preserves behavior exactly: consumer callbacks
  (`handleAddition`/`onTagUpdate`/`handleDelete`/`handleTagClick`,
  `handleInputFocus`/`handleInputBlur`, `handleFilterSuggestions`), gate rules
  (`allowUnique`, `maxTags`, editing-vs-new covariance), state resets and focus
  behavior, the deprecation warnings, and the screen-reader announcement
  strings and focus targets for removals.
- A copied callee's early `return` cannot simply become a `return` at the call
  site (the caller has other branches after it), so each inlined copy re-shapes
  control flow locally: guard-nested `if`/`else` in the keydown and
  suggestion-click paths, `continue` inside the paste loop. Intermediates
  introduced by this re-shaping (`candidateIsEmpty`, `isEditingTag`,
  `limitIsReached`/`pasteLimitReached`/`entryLimitReached`, `isDuplicateTag`,
  `noTagsRemain`/`tagsListEmpty`, `keycodeForSeparator`, `pastedEntries`)
  read as natural maintenance scaffolding, not markers.
- Where the original sub-methods live in the same file as the copies, they are
  left in place untouched — the commit method, the removal method, the
  click-through handler all remain declared beside their absorbed call sites,
  with the smaller helpers they call (limit predicate, input reset, announcement
  builder) still reachable only from those retired originals. The one exception
  is the suggestion component's markup helpers: they were local variables of the
  component body, and absorbing them consumed their declarations outright.
- Cross-file absorption is included where it was genuinely composed into a
  covered flow: the paste path carries its own separator-to-keycode translation
  (the original `utils` export stays, now without production callers, since the
  surrounding library no longer imports it). The paste path still builds its
  delimiter-splitting regexp through the shared `utils` helper: that helper owns
  character-class escaping for arbitrary delimiter arrays, which is text plumbing
  rather than the commit flow being absorbed.
- One stray whitespace normalization inside an unchanged effect carries no
  semantic content.

## Per-cluster rationale

### ReactTags.tsx — module imports

The separator translation import from `utils` is dropped along with its only
production call site (the paste handler now carries its own translation), and the
`SEPARATORS` constant joins the constants import to feed that translation.
Role: keeps the boundaries of the component module consistent with the absorbed
flows (module locals, a constants table, and the remaining shared utility).

### ReactTags.tsx — `handleKeyDown` separator branch (keyboard commit path)

Replaces the delegation to the shared commit method with a guard-nested copy of
the full pipeline: candidate emptiness, editing-vs-new and limit gates with the
limit error plus clear-and-focus reset, duplicate suppression against the
existing tag keys, autocomplete resolution against the suggestion pool
(one-match and any-match modes, consumer filter override, the deprecation
warning carried along), consumer dispatch, then the query/selection/editing
resets — including the input reset sequence and the duplicated query clear the
shared path performed. The old always-true guard around the delegation is
replaced by the pipeline's own emptiness test. Role: the keydown handler now
owns the entire keypress story for commits; this is the deepest and most
branch-heavy of the entry points, so it anchors the keyboard path.

### ReactTags.tsx — `handleKeyDown` backspace branch (keyboard removal path)

Replaces the call into the shared removal method with the full bookkeeping
copy: prevent/stop of the event, tags snapshot, the empty-list guard, the
consumer delete callback, and the focus/announcement sequence — re-querying the
rendered remove controls, choosing the previous row's control, the first
row's, or the input as focus target, with the announcement strings preserved
character for character. Role: keeps both branches of the keydown handler
self-contained, and adds a second, control-flow-reshaped copy of the removal
flow beside the row-level copy described below.

### ReactTags.tsx — `handlePaste` (paste commit path)

The limit guard and input reset are copied in (the handler no longer consults
the limit predicate or the reset helper); the configured separators are
translated to keycodes inline through the `SEPARATORS` constant table (the
carriage-return remark travels with the enter case); each deduplicated pasted
entry assembles its own candidate, and the per-entry loop carries a `continue`-
shaped copy of the commit pipeline with its own limit handling, so a full
multi-tag paste runs start to finish inside one function body. The variable
that accumulates entries is named distinctly from the component's tag state to
keep the copy readable. Role: models the per-entry semantics that motivate the
paste flow to diverge from the shared commit method in the first place, and the
cross-file absorption of the module-level separator translation.

### ReactTags.tsx — `handleSuggestionClick` (dropdown commit path)

The smallest entry point takes the same guard-nested commit copy, resolving the
clicked suggestion through validation, duplication and autocomplete resolution
to consumer dispatch and state reset. Role: the dropdown path, being nearly
identical in duty to the keypress path, demonstrates the drift dynamic: the
copy exists so the dropdown flow can evolve independently, at the cost of a
third owner of the same pipeline.

### ReactTags.tsx — `getTagItems` rendered rows (edit-mode input and per-row controls)

The rendered rows wire their handlers as JSX-attribute arrows that carry copies
of the shared handler bodies: the edit-mode input's focus/change/blur contracts
(consumer callbacks, focused-state flips, the input value clear on blur, the
editing-index reset, the trimmed query update), the remove-control's full
removal bookkeeping and announcement sequence (with its own list snapshot and
focus decisions), and the tag click-through into edit mode (read-only guard,
query prefill, input focus, consumer callback). The keydown and paste handlers
of the same input remain shared references, since those flows were not touched
by this round of row-level work. Role: models the UI-layer variant of the same
copy-in creep — handler bodies move next to the markup that renders them so the
row tree can be reviewed in isolation, and the copies are reshaped only where
a declared parameter replaces a closure-bound one.

### ReactTags.tsx — retained originals

The shared commit method, the shared removal method (and the announcement
builder it calls), the tag click-through handler, the limit predicate and the
input reset helper all remain declared, unchanged, beside the copies — the
retired pipeline keeps compiling and keeps its shape, which is exactly the
endpoint of the modeled evolution: no caller left inside the module. Role:
provides the production context that makes the absorbed flows recognizable as
copies rather than freshly written logic.

### Suggestions.tsx — `SuggestionsComp` render pass

The two per-component markup helpers (the escaped-highlight transformation of a
matched label and the per-suggestion renderer that honored the consumer
override) are absorbed into an explicit row-assembly loop in the component body:
each suggestion resolves its node through the custom renderer if one is
supplied, otherwise through the inline highlight markup, then receives its
active-row class and row-level event bindings before being pushed into the
rendered list. The module-level render gate and memo equality predicate remain
shared module functions (the equality predicate still consults the render
gate), as does the scroll-into-view effect. Role: models the render-side of the
same drift — active-class and highlight tweaks now happen where the rows are
assembled, and the markup helpers' declarations are consumed by the absorption.
