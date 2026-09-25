# Injection design record — record-backed features of the Jarvis CLI

Repository: https://github.com/sukeesh/Jarvis (pinned at
`0c62c730de3af69d0105d0bbc7c65feee25c6f67`). This document records the
design of the case diff: the maintenance situation it models, why each
location has the shape it has, and what production role each piece serves.

## The maintenance motivation being modeled

Jarvis is a personal assistant whose features persist user data through a
single tiny key-value module (`packages/memory/memory.py`, class `Memory`)
that dumps one JSON file. Every record-backed feature — tasks, reminders,
todo entries, tags, routines — historically opened its own `Memory`
instance, pulled its collection out, reshaped it, and pushed it back. That
is a lot of repeated plumbing per feature: read-or-init, add-versus-update
on first write, whole-file dump, and per-feature id counters.

A maintainer addressing that repetition would naturally extract a small
record-store library next to the memory module: one holder class per
collection, owning the key names, the counter keys and the load/save
mechanics, so the feature code stops re-implementing persistence. This case
models exactly that well-motivated maintenance session — the extraction
step, in the state a developer realistically leaves behind when the session
moves the input/output plumbing but not the record work itself.

## The development evolution the diff represents

The session reconstructed here, step by step:

1. A new module `packages/memory/record_store.py` is introduced with a
   `RecordStore` base and one subclass per collection, each replicating
   exactly the persistence form the corresponding feature already used
   (isolated memory file vs. shared memory file, real list vs. JSON-string
   envelope, counter or no counter).
2. Each record-backed feature is rewired across its flows: instead of
   opening a `Memory` and reading/writing the collection through the
   jarvis API, the feature now instantiates its store and calls
   `load()`/`save()` on it.
3. The record manipulation itself — constructing records, allocating ids,
   locating entries by position, rebuilding whole collections, ordering
   them — is not relocated. The features keep doing that work; they simply
   do it directly on the new store objects' fields instead of on values
   obtained from the memory module.

The result is the classic halfway point of a real extraction: persistence
is centralized, but the new holders are passive data shells (load and save
only) while every record decision still executes in the feature classes,
one reach across the new boundary each time. Nothing about this state is
artificial to the codebase — every flow reads as the same code it was
before, with the memory plumbing swapped for a store object.

## Overall injection design

Four production files participate:

- `jarviscli/packages/memory/record_store.py` (new, ~154 lines): the
  extracted persistence library. Base class plus `TaskRecordStore`,
  `TagRecordStore`, `JsonRecordStore` with `TodoRecordStore` and
  `RemindRecordStore`, and `RoutineRecordStore`.
- `jarviscli/plugins/tasks.py` (rewired, 8 methods touched, 1 retired):
  the task feature operates directly on its store's collection.
- `jarviscli/plugins/reminder.py` (rewired in 5 methods across 3 classes):
  tag creation/removal, the shared todo/reminder removal flow, and the
  todo/reminder creation flows operate directly on their stores.
- `jarviscli/plugins/routine.py` (rewired, 2 module functions): routine
  creation and deletion operate directly on the routine store's mapping.

Untouched on purpose: the jarvis API (`JarvisAPI.add_data/update_data/
get_data`), the memory module, the reminder menu/formatting helpers and
`@plugin` interaction wrappers, routine listing/execution, and every other
plugin. The jarvis API remains the only writer of the shared memory file.

## Why the store module has this shape

Each subclass mirrors one genuine storage form, byte-for-byte compatible
with what the feature stored before — that is what makes the session
realistic rather than a redesign:

- `TaskRecordStore` keeps the isolated memory file the task feature always
  used (its own JSON file and its own collection key), including the
  read-as-empty-list fallback for first use, and keeps being constructed
  fresh per operation, exactly as the old per-operation `Memory` usage was.
- `TagRecordStore` covers the real-list collection stored straight into the
  shared memory file, with its own `reminder_tags` key and counter key.
- `JsonRecordStore` is the base for todo and reminder entries, whose
  collections are stored as JSON strings in the shared memory file, and
  whose two subclasses deliberately share one counter
  (`todo_next_id`): continuing ids across todo and reminder collections is
  original behavior a solver may not disturb.
- `RoutineRecordStore` covers the `routines` mapping from routine name to
  command list.

The base class carries the small mechanics common to all of them: a
`load()` that fills `items` (and the id counter where one exists) and a
`save()` that writes the collection back through the same add-versus-update
route the features used. A per-collection `had_invalid_text` flag preserves
the existing decode-error signaling the reminder flows rely on.

## Task feature (`plugins/tasks.py`)

- `load_tasks`: replacement of the legacy constructor-time `Memory` use; it
  sets the store field and loads it. Production role: the entry point that
  guarantees later flows see a loaded collection.
- `list_all`: the all-tasks view. Its condition and print formatting are
  original; the collection access is now direct positional reads into the
  store's collection plus a size probe. Chosen because this flow already
  had the strongest read pattern (index/size/format loop).
- `add_task`: creates an entry by appending a bare record to the store's
  collection and triggering the store's save. Role: task creation.
- `choose_task`: a size probe against the store's collection decides
  whether the selection prompt runs. Role: the interactive pick guard.
- `update_task`: locates the chosen record through the collection by
  position, builds a replacement collection with a per-record rename
  condition, and overwrites the store's collection with the result. Role:
  renaming a task.
- `add_priority_to_task`: same locate-and-rebuild shape with the record
  layout changing (the `"priority"` field and its High/Medium options).
  Role: marking a task's priority.
- `delete_task`: filter-then-overwrite removal of the chosen task. Role:
  task removal.
- `display_sorted`: sizes the collection, copies it, and applies the
  consumer's ordering strategies (by name, or by priority in reverse)
  before printing. The original short-circuit for tiny collections and the
  nested ordering helpers are kept as they were. Role: the sorted view.
- `update_tasks` (retired): with the store now owning save, the old
  whole-collection rewrite helper lost every caller and was dropped from
  the class.

## Reminder feature (`plugins/reminder.py`)

- `TagBase.add_tag` (tag creation): the tag record is constructed in the
  feature — id drawn from the store's counter with the first-use
  zero-initialization, name attached — appended to the store's collection,
  the counter advanced, and the store's save triggered. Role: adding a tag.
- `TagBase.remove` (tag teardown, including cross-referenced cleanup of
  reminders that carried the tag): sizes the collection, prints a numbered
  menu from position-indexed reads, supports removing everything, and for
  selective removal rebuilds the surviving-collection in the feature before
  writing it back into the store. Role: tag lifecycle end.
- `RemindTodoBase.remove` (the removal flow shared by todo and reminder
  collections): consults the store's decode-error flag, probes the
  collection size, iterates entries for cleanup and menu construction,
  supports selective removal, and rebuilds the surviving collection. Role:
  the shared teardown lifecycle.
- `TodoBase.add` and `RemindBase.add` (creation flows for todo and reminder
  entries): construct the dated record in the feature — id from the shared
  counter, plus the message/date/progress fields — append it to the store's
  collection, advance the shared counter, and save. Role: entry creation.
- The older jarvis-API helpers on these classes (`load_tags`, `save_tags`,
  the `get_next_id` pair) are left in place as the session left them: the
  flows that still use them keep using them, and the rewritten flows simply
  no longer call them. Which of them remain meaningful after ownership is
  settled is a decision the follow-up work gets to make — the state left
  here does not presuppose it.

## Routine feature (`plugins/routine.py`)

- `create_routine`: makes the fresh routine store, ensures its mapping
  exists (the original none-means-empty behavior), and inserts the command
  list under the routine's name. Role: defining a routine.
- `delete_routines`: membership-tests the store's mapping and pops the
  named routine, keeping the original "Invalid routine name" reply. Role:
  routine teardown.
- Routine listing and execution keep their existing jarvis-API reads; the
  `routines` key constant stays because those flows use it.

## Roles the locations play, in summary

Across the four files the same relation appears in the distinct lifecycle
phases a record collection has: creation (task/tag/todo/reminder/routine
additions), inspection (listing, sorting, menus), single-record alteration
(rename, priority), teardown (deletions, the everything-paths), and
bootstrap (empty-collection handling). The holders participate as the
persistence point of each phase; the features participate as the places the
record decisions were executed. This record describes the designed state;
whether that state counts as well-formed or as harboring one specific
design problem, and whether any piece of the diff is unrelated to it, is
for later review to determine.
