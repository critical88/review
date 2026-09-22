# Refactor request — misplaced card behavior in the addon settings pages

## What we are seeing

While modernizing the addon list surfaces onto the shared card components, we
introduced small presentation-model classes alongside the card views on the
settings pages: one behind the Themes page list card, one behind the Plugins
page list card, and one behind the Plugin Browser repository cards. Each of
these classes takes an addon record in its constructor and exposes the
rendering fragments its card needs (labels, description text, author
information, identifiers), plus, on the Themes side, the selection switch and
the overflow actions.

Reviewing those classes, almost none of their logic is about their own state.
Their methods dig through the wrapped record for the card's title, subtitle,
description, author names, author ids, author totals, and selection status;
the Themes class additionally routes selection and delete/refetch actions
around the record and the installed-themes storage. The class's own fields
hold nothing but the record it was handed, and the fragment behavior that
prepares each card's data is now one step removed from the records and
storage it actually reads. The wrapped records themselves already have
data-owning surfaces in this codebase — the themes addon module and its
storage, and the unified plugin model that pairs each installed plugin with
its page-facing behaviors.

## Where this shows up

- the Themes page list card: description parts, the safe-mode-aware radio
  toggle and apply/clear selection, and the refetch/copy/delete overflow
  actions;
- the Plugins page list card and the plugin info sheet header: the shared
  heading fragments, the author roster (names, ids, total) and the user
  fetching driven from it;
- the Plugin Browser repository cards: manifest display text (title, byline
  with fallback, version label, description) and the manifest identity handed
  to the trailing buttons.

All three surfaces render from the same wrapped-record pattern, so please
investigate all of them rather than treating any single class or card in
isolation.

## Desired outcome

Reorganize this behavior so the fragment and action logic sits with the data
it identifies with — on the data-owning surfaces behind those records or
immediately beside the records' access layer — instead of in intermediate
classes that exist only to reach into a record. Keep every card rendering,
toggle, sheet, toast, confirmation, fallback string, and list interaction
behaviorally identical to today. Remove the intermediate classes once their
behavior is relocated, rather than leaving forwarding shells behind.

## Behavior and API boundary

- User-visible behavior on the pages is unchanged: same label/subtitle/
  description text and fallbacks, same selection radio semantics, same
  apply/clear error path, same refetch/copy/delete toasts and confirmation
  flow on the Themes card, same search-result highlighting, badges and author
  line on the Plugins card, same avatar-pile and pressable author chips on
  the plugin info sheet, and the same byline fallback and version label on
  the Plugin Browser cards.
- The unified plugin model remains the Plugins page's data boundary, and
  both installed-plugin kinds keep exposing the full behavior set the page
  consumes.
- Storage subscriptions that drive re-rendering (proxied theme records,
  store-driven hooks in the sheet header) keep working as before; do not
  move reactivity out of the components in a way that would break updates.
- The repository's build and its code conventions must keep passing.
