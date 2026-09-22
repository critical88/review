# Refactor request: the tag-input subsystem absorbed its own helper implementations

## What we observed

The main tag-input component of this library has grown handlers that carry the
implementations of the sub-methods they used to call. The keyboard handler now
evaluates a full tag-commit pipeline in its own body before it ever reaches a
consumer callback; the paste handler resolves delimiters and then re-implements
the same commit pipeline per pasted entry; the suggestion-click handler carries
yet another copy. The backspace branch of the keyboard handler and the
remove-control arrow in the rendered rows each re-implement the tag-removal
bookkeeping, including the focus restoration and screen-reader announcements.
The edit-mode input's focus/change/blur behavior is duplicated inside the JSX
wiring of the rendered rows, the suggestion-list component assembles each
suggestion row inline in its render pass, and the tag click-through into edit
mode is carried by a rendered-row arrow.

Meanwhile the earlier shared commit routine, the shared removal flow, the
limit predicate and the input-reset helper are still declared in the component
file, and at least one exported helper in the library's utility module no
longer has any production caller.

## Diagnosis

The input-handling orchestration was gradually inlined into the hot paths.
Copied sub-method implementations now sit at the wrong abstraction level: an
input handler owns autocomplete candidate resolution, duplicated-suppression
and state-reset detail; rendered JSX attributes own removal choreography. Each
copy reshapes the same underlying flow differently (guard branches instead of
early exits, per-entry continues), so gate rules can no longer be changed in one
place, and the retained originals are dead weight that misleads readers about
which code path is live.

## Scope and boundary

Focus on the tag-input subsystem: the component that owns keyboard input,
pasted input, suggestion selection, tag editing and rendered row events, and
the suggestion-list component that renders the dropdown rows. Out of scope:
public prop and callback contracts, default class names, module-level utilities
that remain genuinely shared, drag-and-drop plumbing, the portal/ref-forwarding
wrapper, and the memoization/scroll internals of the suggestion list beyond how
its rows are assembled.

## Desired outcome

Re-establish a decomposition the team can reason about:

- each user entry point (keyboard commit and removal, paste, suggestion click)
  should coordinate with well-scoped operations instead of owning copied
  pipeline bodies, and there should be one owner of the commit rules
  (validation, editing/limit gates, duplicate suppression, autocomplete
  resolution, dispatch, state and input reset);
- the removal flow, including focus restoration and screen-reader
  announcements, should have one implementation covering both the keyboard
  path and the row controls;
- the rendered rows' event wiring should delegate to named handlers rather
  than carrying copied bodies inside JSX attributes;
- the suggestion rows' assembly (custom-renderer dispatch, label highlighting,
  active-row emphasis) should live in helpers rather than inline in the
  render pass;
- anything left with no production caller after the refactor should be
  removed, so the module does not keep unreferenced declarations behind.

Apply the same reasoning everywhere in the subsystem; do not stop after the
first or most visible occurrence.

## Behavioral requirements

Behavior must be preserved exactly:

- gate semantics: duplicate suppression, tag-limit enforcement, editing-vs-new
  covariance, empty candidates;
- consumer callbacks: addition/update/delete, input focus/blur/change,
  suggestion hover/click, filter override — same events, same payloads, same
  timing;
- the deprecation and migration warnings consumers rely on must still fire on
  the same triggers;
- pasted content: splitting on configured delimiters, trimming, deduplication,
  and per-entry commit rules;
- removal behavior: the exact screen-reader announcement wording and focus
  targets, including moving focus to the previous row's control, the first
  row's control, or back to the input;
- edit mode: prefill on tag click, input focus, blur-time input clearing and
  state reset;
- suggestion rendering: custom renderers, the render gate and its override,
  and highlighted labels with escaping.

The public API, prop names, and rendered class names must not change. Run the
existing test suite as the behavioral reference; it must pass without
modification.
