# Injection design record — RPISEC/MBE lab drivers

## Maintenance motivation

The RPISEC/MBE tree is maintained as course material: every semester the staff
adjust the interactive lab drivers — prompts, banners, timeout armor, and the
support code around them. Two facts of that maintenance shape this case:

1. `utils.h` exists precisely so drivers do not hand-roll input handling. It
   provides the shared "read an unsigned number off stdin, then eat the rest
   of the line" routine (`get_unum`) plus the buffering and timeout helpers,
   and the drivers were originally written to call it at every numeric prompt,
   and to delegate each menu command to a same-file worker function.
2. The lab drivers are also the code staff hacks on late at night before an
   image cut, because a lab that misbehaves in a student session is a lab that
   gets fixed in place, quickly, right there in `main`.

The evolution modeled here is the classic one: under deadline, several drivers
received "works-now" edits that replaced calls by pasted bodies. When a worker
function stood in the way (its return plumbing awkward inside a branch, its
prompt text off by a newline, its recursion annoying to single-step), the
caller simply swallowed the body. When the shared input routine's shape was
needed at a prompt, it was re-typed by hand into fresh locals instead of being
called. Each edit was behavior-preserving and locally reasonable; none came
with a cleanup pass, so the next semester inherited drivers that had grown
from thin dispatch loops into monolithic workspace functions. That is the
normal drift this case reproduces across the interactive drivers.

## Overall design

The edit set reads as one developer's hasty inlining pass across the
interactive console drivers of the lab03–lab09 families — the components a
maintainer actually revises between semesters. Every touched location absorbs
work into the driver function from one of two natural sources:

- **same-file worker functions** whose bodies got pasted into the caller's
  dispatch region (with the originals either deleted outright, moved into the
  caller wholesale, or left standing unused), and
- **the shared input idiom from `utils.h`**, spelled out by hand at prompt
  sites in `flush; scanf; line-drain` order with driver-local scratch
  variables.

The two sources interlock: absorbing a worker body usually also means
re-typing the prompt reads that the worker used to make, since the body is
now being executed where the caller's locals live. Sites were chosen to cover
different lab components and genuinely different workflows (storage shells,
a marketplace account flow, an authentication session, an OTP message
service, a self-describing book browser, a vector CLI, a storage-lockbox
service), and the concrete implementation was deliberately varied per file —
retained versus deleted originals, different drain-loop spellings, different
scoping disciplines for the scratch locals — so that the sweep looks like
organic drift accumulating over time rather than one repeated mechanical edit.

Transcription was done in operation order with the original prompt strings,
comparisons, and control decisions intact, so each driver still performs the
same observable session for the same input. No exercise's intended unsafe
pattern (missing bounds checks, format-string usage, deliberate stack quirks)
was hardened or altered: the drivers still teach exactly what they taught.

## Per-location design

### 1. `src/lab03/lab3A.c` — first-generation storage shell

**What changed.** The `store_number` and `read_number` worker functions were
removed from the file, and their bodies were spliced into `main`'s
`store`/`read` command branches behind main-scope locals: `store_val`,
`store_slot`, `read_slot`, and a line-drain character `line_ch`. The three
numeric prompt reads the workers used to make through the shared routine are
now spelled out by hand in the `fflush(stdout); scanf("%u", &var);` +
`while(1){ line_ch = getchar(); … }` shape under a per-site comment
explaining the stdin treatment. The workers' early `return 1` on a reserved
slot became a branch-local `res = 1` with the save moved into an `else`.

**Why this site and shape.** lab3A is the canonical first console of the
course — the smallest driver that still has the full dispatch/worker split
and pure shared-input prompting, so it is the most natural place for a hurried
"make the command handlers self-contained" edit to delete the workers
wholesale. Deleting them (rather than leaving them standing) models a
maintainer confident the splice was final.

**Production role.** The student's intro interactive console: store/read/
quit over an unsigned array with the quend-reserved slot rule
(`index % 3 == 0`, `input >> 24 == 0xb7`).

### 2. `src/lab05/lab5A.c` — second-generation storage shell

**What changed.** Same treatment as lab3A: `store_number` and
`read_number` were removed and their bodies spliced into `main`'s command
branches with locals `chosen`, `slot`, and `drainc`. The hand-typed input
treatment is spelled in the `do { drainc = getchar(); } while(drainc != '\n'
&& drainc != EOF)` variant, with the worker's `int` index semantics kept
faithful (an explicit `(unsigned int)` cast on the check the original made
through its typed locals).

**Why this site and shape.** lab5A is the successor shell of the same family
with the stricter slot rules, and showing the identical drift in a sibling
component — spelled by a different hand with a different drain-loop idiom —
is what makes the pattern look like a live bad habit of the staff rather
than a one-file accident.

**Production role.** The second storage exercise: same session shape with
the doom-reserved-slot variant and range check.

### 3. `src/lab06/lab6A.c` — l337-Bay marketplace

**What changed.** The `setup_account` body (name/description collection and
the description-stitching writes) was pasted into the choice-`1` branch, and
the `make_listing` prompt chain into the choice-`2` branch. A main-level
scratch buffer `acct_tmp` carries the intermediate description text. The
original `setup_account` and `make_listing` definitions were left standing
in the file, now unused.

**Why this site and shape.** The storefront is the most interactive of the
early labs — two multi-prompt workflows per session — so "temporarily"
pasting them into the branch to watch a full session without leaving `main`
is exactly the inplace fix a maintainer makes. This file deliberately keeps
the originals: an abandoned refactor that ran out of time (or its author's
nerve) leaves dead workers behind, which is the other half of the realistic
housing pattern next to the deletions in the storage shells.

**Production role.** The marketplace exercise: account creation feeding the
bidding/listing display the student inspects.

### 4. `src/lab06/lab6B.c` — FALK OS login

**What changed.** The `login_prompt` function was removed; its per-attempt
loop now lives directly in `main` right after the banner, with locals
`entry_pw`, `entry_user`, `readbuff`, an `attempts` counter, and `auth_rc`.
The candidate password's preparation — the two XOR derivation loops that the
file's `hash_pass` implements — is re-typed inline before the `memcmp`
against the secret, and the failed-attempt notice and the
too-many-attempts refusal (banner art, failure exit) moved inline with it.
`hash_pass` itself was left untouched and is still called by `main` to
prepare the real secret before the loop.

**Why this site and shape.** The login flow is the one driver where the
inlined body crosses a real confidentiality boundary, so the edit models the
maintainer wanting the attempt accounting visible locally. The interesting
design decision here is the *mixed* housing: the workflow function is
deleted, but the derivation helper it used is duplicated while still live
and called for the secret — the least consistent, most believable end state
of a rushed pass.

**Production role.** The authentication exercise reading the `/home/lab6A/
.pass` fixture, hashing candidate credentials, and rewarding exactly the
right pair.

### 5. `src/lab07/lab7A.c` — OTP secure-message service

**What changed.** `create_message` was removed; the whole creation workflow
now sits inside `main`'s choice-`1` branch behind locals `pad_ix`,
`create_rc`, `line_ch`, `msg_len_val`, and `new_msg`: the slot scan, the
allocation and zeroing, the callback wiring, the `rand()` pad fill, the
length prompt, the zero-length and clamp rules, the raw read, and the pad
application written out as an inline `for` loop instead of a call to the
file's pad applier. The data-length prompt's shared read routine is spelled
out by hand like the storage-shell prompts. The other menu options (edit,
destroy, print, quit) keep their worker functions and the shared menu read.

**Why this site and shape.** The message service has the clearest
one-command-one-workflow structure, and create is its longest workflow, so
it is the natural absorption target. Writing the pad application inline (as
well as the input read) shows the caller absorbing a compute helper and a
utility in the same pass; keeping the rest of the menu intact keeps the
file readable as "most of the service still delegates, one branch stopped
doing so".

**Production role.** The OTP creation exercise: fresh random pads, length
rules, and the print callback that later options rely on.

### 6. `src/lab08/lab8A.c` — Beta-Book-Browser

**What changed.** The recursive book-request flow was pasted into `main`
but rewritten while pasting: the recursion became a `while(1)` loop with a
fresh per-iteration stack buffer, the per-request branching became an
inline `strcmp` chain (`A`, `F`, the zero byte, the 1337 refusal printed and
exited), and the page-397 word check (global address computation from the
loop's buffer, oversized read, XOR'd cookie integrity check) was spliced in
immediately after the loop with two new comments. The original
`selectABook` and `findSomeWords` definitions still stand in the file,
unused.

**Why this site and shape.** This is the deliberate idiom-change of the
case: recursion is the control flow a debugger makes most painful, and
flattening it into an inline session loop is a *plausible renovation* — not
a copy-paste. It also concentrates the driver's genuinely security-relevant
behavior (the terminal-1337 refusal and the untrusted buffer
`printf`) inside the pasted region, which is where a rushed edit does the
most narrative work. Keeping both originals dead models the
"refactor-search" abandoned halfway.

**Production role.** The format-string/stack-quirk exercise the students
exploit; its observable transcript (prompts, book shelve-out, word check)
is unchanged.

### 7. `src/lab08/lab8B.c` — vector math CLI

**What changed.** `enterData` and `sumVectors` were removed; the menu cases
now carry their workflows in scoped blocks with locals `selc` and `vend`
(case `1`) and direct access to the module-level addend/sum vectors. Inside
the entry block, the `vectorSel` body is re-typed (a `getchar()` skip loop
and a selection switch mapping to `&v1`/`&v2`/`&v3`, with the bad-selection
refusal), followed by the "don't enter into the sum" rule and the nine
`scanf` reads. Case `2` inlines the addend zero-check with its crestfallen
notice and the nine summation writes. `vectorSel` itself remains defined and
is still used by the print case.

**Why this site and shape.** This driver's workers are exactly the
"convenience wrapper around global state" kind a maintainer declares
pointless once the cases are being edited anyway — the stated-aloud
justification ("the cases can see `v1`, `v2`, `v3` directly") writes the
commit message for the deletion itself. Leaving `vectorSel` alive on the
print path while duplicating its body on the entry path keeps a live
original served to one caller and pasted into another, so no single
per-file habit explains all sites.

**Production role.** The vector exercise: entry, summation with addend
validation, and the favorites/printer paths left delegating.

### 8. `src/lab09/lab9A.cpp` — clark's item storage lockboxes (C++)

**What changed.** All four command handlers — `do_new_set`, `do_add_item`,
`do_find_item`, `do_del_set` — were removed and absorbed into `main`'s
switch as per-case-scoped blocks. Two main-level scratch variables
(`in_val`, `line_ch`) are explicitly re-zeroed at each of the seven prompt
sites, where the shared read routine is spelled out by hand; the handlers'
early `return`s became `break`s, and their exact status messages ("Which
lockbox do you want?: ", "No more room!", "Item Found", the lockbox print,
"Invalid ID!") moved into the switches untouched. The `HashSet` template,
the hash functor, `print_menu`, and the class machinery are untouched, as
are edit-relevant behaviors like the size-checked accessor.

**Why this site and shape.** The lockbox service is the broadest single
absorption in the case — four whole workflows plus their prompt plumbing in
one switch — and it is the C++ half of the sweep, showing the same drift in
a driver with class-backed storage. The per-case block scoping and the
re-zeroed shared locals are the "each case is self-contained, it's fine"
rationalization: the switch *looks* more organized while the workflow logic
itself has nowhere to live but inline. This provides the deepest nesting of
the case: workflow inside case blocks inside the dispatch loop inside
`main`.

**Production role.** The final lab's storage service: four-session command
surface over heap-allocated lockboxes.

## Deliberate structural variation

The sweep intentionally does not use one repeated edit shape, because
organic drift never does:

- **Housing of the swallowed originals alternates per file**: deleted
  outright (lab3A, lab5A workers; lab6B `login_prompt`; lab7A
  `create_message`; lab8B `enterData`/`sumVectors`; lab9A all four
  handlers), left standing unused (lab6A `setup_account`/`make_listing`;
  lab8A `selectABook`/`findSomeWords`), and still-live-but-duplicated
  (lab6B `hash_pass`; lab8B `vectorSel`).
- **The input idiom is spelled two different ways** — `while(1)`-break
  drains (lab3A, lab7A, lab9A) versus `do…while` drains (lab5A) — and the
  scratch locals are main-wide in some drivers (lab3A, lab5A, lab6A, lab6B)
  versus per-site block-scoped in others (lab8B cases, lab9A cases).
- **Idiom changes accompany the absorption** where a maintainer's hand
  would actually re-shape things: recursion flattened to an inline loop
  (lab8A), the pad applier written out as a loop at the call site (lab7A),
  worker early-returns turned into branch-local result/`break` flow
  (everywhere an absorbed worker had them).
- **Language mix**: seven gnu89 C drivers and one C++ driver, so the drift
  crosses the tree's two built source languages within one narrative.
- **Localization burden is deliberately uneven**: in some files the pasted
  body is textually obvious (lab6A, lab8A: the untouched original sits in
  the same file), while in others the original is gone (lab9A, lab7A), so
  the swallowed workflow is only reconstructible from the driver's own
  dispatch semantics and the shared-utility idioms around it.

## Rationale audit trail

- **Scope choice.** All 41 programs the recorded build contract compiles
  were surveyed for the seam this case needs: a driver that dispatches to
  same-file workers or prompts through the shared input utilities. The
  interactive lab drivers in the lab03–lab09 families are the only place
  both seams co-occur; the lecture programs are single-purpose
  demonstrations with no dispatch/worker split to swallow, lab02/lab04 and
  the second/third variants of the touched labs are too thin or would force
  content-free padding, `lab7C` and the heap-use-after-free lecture driver
  are already very large drivers in the clean tree whose size predates any
  modeled edit, and material outside the recorded build contract
  (`cpp_lec02`, `lab01`, `lab10`, the project harnesses, `print_frees`)
  could not participate without entangling the case in a separate defect or
  in files nobody builds.
- **Coverage chosen over repetition.** The eight selected sites span
  storage shells, storefront account flow, authentication, an OTP service,
  a browser with a self-describing menu, a vector CLI, and heap-backed
  lockbox storage — genuinely different workflows — instead of eight copies
  of the same "paste a worker into a menu branch" edit. The depth of the
  absorption also varies: prompt-only plumbing sites (lab3A, lab5A) up
  through whole-workflow splices (lab7A, lab9A), with lab6B and lab8A
  deliberately mixing both kinds in one file.
- **Behavior preservation by construction.** Each pasted body was
  transcribed with the original operation order, prompt strings, status
  text, comparisons, and exit paths, with only the syntactic accommodation
  a new lexical home requires (locals renamed to the caller's style, early
  returns to branch results, recursion to a session loop). Session-visible
  output, exit codes, and the exercises' intended unsafe patterns were
  treated as course material and left exactly as shipped.
