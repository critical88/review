# Project workspace file-tree responsibilities have collapsed into one place

Marker's project file tree has grown a single `WorkspaceService` class in
`src/utils/workspace.ts` that now owns almost every behavior the tree needs.
It presents as a deliberate coordinator: a process-wide singleton plus a
per-project factory, and the whole file lifecycle - recursive directory
scanning with metadata attachment, sort ordering for the rendered tree,
tree pruning after deletion, and the create/rename/remove steps -
implemented inside its own body, with module-shared mutable sort state
threading between its ordering members. Two of the older utility helpers
were reduced to pass-through delegations to it, keeping only their names so
existing imports keep resolving, and the rest of the file-tree layer now
routes its work through the service.

The result is that one class has many unrelated reasons to change: it walks
directories, talks over the Tauri interop, joins paths, decides comparator
ordering, prunes the in-memory tree, and performs create/rename/delete
mutations. Those concerns have different dependencies and evolve for
different reasons, so keeping them fused in one owner makes every
file-tree change land in the same body.

## What we want

Dissolve this arrangement. Trace where the service's behavior is consumed,
identify the distinct responsibilities it accumulated, and give each of them
a cohesive, focused home (dedicated modules, small owner units, or
equivalent structures - the exact shape is yours), so that no single body
concentrates the whole file-tree lifecycle:

- directory scanning and entry metadata handling;
- sort ordering for the tree;
- in-memory tree pruning;
- the create / rename / remove file lifecycle.

Everything that currently calls into the workspace service should instead
depend on the focused unit that owns the work it needs, so no call site
routes through one central intermediary. The utility helpers that became
pass-through delegations must not survive as empty forwarding shells:
either their names become real implementations again or their consumers are
rewired away - importing a helper must never mean a second hop into a
broader owner.

## Behavior that must stay stable

This is a restructuring task, not a feature change - external behavior,
rendered UI, and module contracts must be preserved:

- the exported `getFileMeta` metadata function and the default-exported
  `removePath` pruning function keep resolving for code that imports them
  today;
- directory scans keep skipping hidden (dot-prefixed) entries and
  non-`.md` files, keep walking nested directories, and keep attaching
  metadata to every kept entry; refreshing the file list when no project is
  selected still does nothing rather than erroring;
- sorting still happens in place (nested children included) with identical
  name and created/updated-date results in both directions, including the
  direction-inversion the date comparisons show today;
- creating a file keeps suffixing `.md`, keeps materializing the missing
  parent folder, and keeps writing an empty markdown file; renaming keeps
  the suffix and parent-directory behavior; deleting still removes
  directories recursively but files singly; and deleted entries disappear
  from the in-memory tree that is rendered;
- the currently open file still follows a rename: after renaming the
  selected file, the open editor target reflects the new path and
  suffix-corrected name;
- every file-tree interaction - choosing a sort order, renaming or deleting
  from the context menus, creating a file from the new-file form - keeps
  working exactly as before;
- `npm install` then `tsc --noEmit` must stay green.
