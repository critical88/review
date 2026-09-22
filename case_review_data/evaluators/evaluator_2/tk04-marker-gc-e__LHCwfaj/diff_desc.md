# Injection design record — `tk04-marker-gc-e`

## Maintenance motivation

Marker's project workspace accumulates file-tree logic in the places that
need it: the recursive scan walk is inlined inside the zustand `fetchDir`
action and (a second, verbatim copy of it) inside the project workspace
screen's `getFiles`; metadata attachment lives in `src/utils/getFileMeta.ts`;
tree pruning lives in `src/utils/removePath.ts`; the sort comparator lives inside
the `Root` file-tree component; and create/rename/delete lifecycle steps are
spread across `File.tsx`, `Tree.tsx`, and the project screen's
`addFileHandler`. A maintainer hit by repeated touch-points across these
files has an obvious, tempting answer: "the file tree needs one owner". The
change recorded in `smell.diff` models what that pull looks like once it is
followed past its first reasonable step.

## Development evolution being modeled

This diff models the natural slope of a well-intended coordinator
introduction. Real projects rarely grow a god class in one commit; they grow
a "central workspace helper", pull a second capability into it because a
caller needs two things at once, then a third because unit-testing one place
is easier than threading props, and the class quietly becomes the only way
anything in the area gets done. The endpoint recorded here is where that
slope finishes: one service class with a singleton, a factory, shared
mutable sort state, and every file-tree concern implemented in its own body.

## Overall design

A new `WorkspaceService` class in `src/utils/workspace.ts` is introduced as
the project workspace's "coordinator":

- a process-wide `shared()` singleton plus a `forProject(projectDir)` factory,
  so callers stop passing directories around and bind to an instance instead;
- the recursive directory walk with hidden-entry and non-markdown filtering,
  and metadata attachment;
- the sort comparator with its own `activeSort` state field, so ordering is
  decided inside the service rather than in component scope;
- tree pruning, file creation, rename, and remove dispatch, so the context
  menus and forms delegate lifecycle steps to it;
- the type surface (`FileInfo`, `FileMeta`, `SystemTime`) is re-exported so
  importers have one canonical import site.

The older modules are reduced to thin ballast: `getFileMeta` and
`removePath` still exist but only forward to the service, because other
modules import them and the developer did not want to touch every consumer
yet.

## Per-location record

### `src/utils/workspace.ts` (new)

Holds the whole coordinator body: singleton/factory plumbing, `scanDir` +
private `processEntries` (the walk, copied verbatim from the store action,
including its known quirk of not awaiting the recursive descent),
`getEntryMeta` (what `getFileMeta` used to do), `compare`/`sortFn`/`sortTree`
(the comparator with its own mutable sort state instead of the component's
props closures), `removePath`, and `addFile`/`renameEntry`/`deleteEntry`
(the lifecycle). Written as one class because the developer wanted one
import and one call surface per screen; the result is that
sorting-related state and filesystem lifecycle mutate through the same
instance, and every distinct concern has the same reason-to-live.

### `src/utils/getFileMeta.ts`

The metadata helper becomes a forwarding wrapper to the service while
keeping `FileInfo` re-exported from the service, because `Root.tsx`,
`Tree.tsx`, and the store still import the type from here. Chosen since the
importer surface is the widest of the utility modules - keeping the name
stable avoids touching the type imports in components.

### `src/utils/removePath.ts`

The pruning helper likewise keeps its export shape and forwards to the
service. It stays as its own module since both `File.tsx` and `Tree.tsx`
import it, and re-pointing every importer was deferred.

### `src/store/appStore.ts`

`fetchDir`'s 25-line walk assembly (the `processEntries` closure plus the
"no project selected" guard) collapses to a single service call; the
`FileInfo` import moves to the service's canonical type surface; the direct
`readDir` import is no longer needed. This site was chosen because it is the
first duplicated copy of the walk and the most annoying to extend; keeping
the guard behavior identical was the priority.

### `src/components/Project/FileTree/Root.tsx`

Removes the component-local `compare`/`sortFn` pair and the sort effect body;
the effect now hands the tree section to the service and still copies the
result into local state for rendering. The component keeps the sort popover
UI unchanged. This site was chosen because the comparator is pure logic and
was previously two closures living in render scope.

### `src/components/Project/FileTree/File.tsx`

`rename()` delegates to the service (suffixing `.md`, computing the target
path and applying the rename), consuming `{path, name}` it returns to update
the currently-open file when the active entry was renamed, preserving the
existing refetch order. `deleteFile()` no longer calls `removeFile` +
`removePath` separately - the service returns the pruned tree. The menu UI
is untouched.

### `src/components/Project/FileTree/Tree.tsx`

`deleteFile()` mirrors `File.tsx` but for directories - the service decides
between `removeDir` with recursion and `removeFile`, and returns the pruned
tree, so the component keeps only the confirm-dialog concern.

### `src/components/Project/File.tsx` (project workspace screen)

The screen binds a `WorkspaceService.forProject(project.dir)` instance, so
`getFiles()` drops its `path` parameter and `readDir`/`join`/
`exists`/`createDir`/`writeTextFile` imports. `addFileHandler` delegates to
the service and then re-scans via the bound instance rather than
`getFiles(project!.dir)`. The store wiring, folder collapse state, and
rendering are untouched. This site was chosen as the second duplicated
copy of the walk and the only place that creates files.
