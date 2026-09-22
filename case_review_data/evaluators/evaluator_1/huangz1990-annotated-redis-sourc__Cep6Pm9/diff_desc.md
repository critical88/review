# Injection design record — hot-path inlining campaign in the command layer

## Maintenance motivation

Early Redis development history repeatedly trades call-graph purity for raw
throughput on the hottest command paths. Around the 2.4/2.6 era, commits and
mailing-list discussions ("avoid function call overhead in the INCR path",
"speed up the expire path") describe maintainers manually unfolding helper
calls inside the busiest command implementations and keeping the generic
helpers around for less important callers. This repository is the annotated
Chinese-commentary edition of Redis 2.9.7, which preserves that development
style: command implementations live in dedicated per-type modules and sit on
top of a deep shared pipeline — key lookup with lazy expiry, add/overwrite
key assignment, object parse/encode conversions, the reply buffer protocol,
and AOF/replica propagation.

The modeled campaign is exactly the kind of performance sweep a maintainer of
this codebase would run: take the shared generic handlers that every command
in a family funnels through, and unfold their helper calls directly into the
handler body so that the hot path performs no extra jumps. The edit is
believable precisely because each of these generics is the single point every
command of its family passes through, so shaving per-call overhead there has
the highest visible payoff — and because the shared helpers stay in the tree,
still used by other modules, so nothing looks deleted.

## Development evolution being modeled

The change models a short but concentrated optimization phase that grew by
accretion across one subsystem:

1. The string family came first, because it is the busiest: the shared write
   generic behind SET/SETNX/SETEX/PSETEX is unfolded, then the shared read
   generic behind GET/GETSET, and then the numeric mutation generic behind
   INCR/DECR/INCRBY/DECRBY as the maintainer chased the same pattern through
   the file.
2. The expiry family followed, since EXPIRE/PEXPIRE/EXPIREAT/PEXPIREAT and
   TTL/PTTL route through two generics in the database module that sit right
   on top of the same lookup and expiry machinery the string work had already
   touched.
3. Finally the hash field counter HINCRBY, a known hot path in real workloads
   (counters in hashes), imported the same style from its busier siblings.

Each step of that sequence leaves the previous state compilable and
functionally identical: this is a performance refactor, not a feature change,
so behavior, reply bytes, statistics and memory ownership are carried over
statement by statement. Together the six edits make one reviewable developer
task: "fold the helper calls out of the generic command implementations in the
string, expiry and hash layers, preserving behavior exactly."

## Overall design

Six shared command handlers across three command modules take over the bodies
of the helpers below them, mixing 2 to 4 call levels of the pipeline into one
function per site:

* the string write generic (used by SET/SETNX/SETEX/PSETEX),
* the string read generic (used by GET and GETSET),
* the string numeric generic (used by INCR/DECR/INCRBY/DECRBY),
* the expiry mutation generic (used by EXPIRE/PEXPIRE/EXPIREAT/PEXPIREAT),
* the TTL query generic (used by TTL/PTTL),
* the hash field increment (HINCRBY).

The unfolded fragments are drawn from more than one lower module: the shared
key/dictionary layer, the shared object parse/encode layer, and the reply
output layer. Nothing else in the tree is touched; the helpers whose bodies
are copied remain in place for their other callers, which is historically
faithful (unfolded copies coexisting with the original functions was the
normal end state of these optimizations).

Because the copied bodies reference internal functions whose prototypes are
declared only inside their owning modules (the reply-string writer living in
the networking layer, and two slots bookkeeping helpers living in the
database layer), each command module gains a small group of forward
declarations next to its includes. The string and hash modules also pick up
the character-class include their absorbed parse code needs; the database
module already had that include. These header edits are the ordinary residue
of such a campaign rather than a separate concern.

Comments inside the rewritten handlers follow the style of the unfolding
maintainer: short step markers naming the region being walked through
(parameter parse, conditional-create check, assignment path). Some markers
name a step that no longer has a function boundary attached to it, which is
what naturally happens when the steps stop being calls.

## Per-site rationale

### String write generic (`src/t_string.c`, shared by the SET family)

This is the campaign opener because it is the single most executed code path
in the server. The rewrite absorbs the layers the handler previously knew
only as calls: the validity window for the optional timestamp argument
(integer parse, leading-blank rejection, range check, unit conversion, the
two error messages specific to SETEX/PSETEX), the conditional-create check
that NX/XX semantics ride on (including the lazy expiry gate and the key
lookup plus reference-cache touch it performs), the assign path with its
fresh-add versus overwrite split, the removal of any stale expiry record, the
touched-key notification, and the expiry-record write on the SETEX route.

The guard structure differs from the layers below it deliberately: where the
shared expiry helper asks a question and returns a boolean that is then
tested, the unfolded body flattens the question into one compound condition so
that the miss case falls straight through to the reply. Reply production
likewise mixes: the shared OK/cone replies are still used as calls, while the
object work that produces them is stitched in place. The production role is
unchanged: one function that creates or updates a string key with optional
NX/EX semantics and optional expiry.

### String read generic (`src/t_string.c`, shared by GET/GETSET)

The deepest absorbed stack in the campaign lives here, because reading a
value crosses every layer at once: the lazy expiry gate, the propagation of a
logical delete to the AOF and replicas, the key lookup itself with its
hit/miss statistics and LRU clock refresh, and then the reply protocol
machinery that formats a bulk reply around an arbitrary value — including the
output-buffer fast path, the overflow path into the client's object list with
its last-object reuse accounting, the small-integer optimization, the decode
fallback for non-string encodings, and the bulk header arithmetic.

This site was chosen to carry the deepest stack precisely because the
original call chain here runs four levels deep; a maintainer unfolding for
speed stops being credible before this point. Variables inherited from the
copied layers are re-scoped (the buffer path uses locally named flags for the
ready and buffered states), and the reply construction alternates between the
shared header shortcut and a hand-built one, mirroring how the original layers
behave rather than re-inventing them. The miss reply and the type-error reply
keep their original shared objects.

### String numeric generic (`src/t_string.c`, shared by INCR/DECR family)

The counter path is the classic documented speed complaint in this codebase,
so the campaign would be unpersuasive without it. The rewrite takes over the
write-side lookup (including its expiry gate and reference-cache touch), the
type check with its shared wrongtype reply, the long-long parse for both
encodings (including the zero fallback for missing values and the shared range
error), the overflow guard kept exactly as written, the counter-object
construction with its small-integer cache split, and the fresh-add versus
overwrite assignment.

The final reply keeps calling the shared colon/integer/CRLF pieces as calls —
a performance-minded maintainer folds until the remaining calls are
single-line protocol tokens, and this line is drawn here on purpose so the
site does not become a stylistic caricature of itself. The overflow test is
left as the original two-branch expression even though the surrounding code
was partly flattened; keeping recognizable lines like this is what makes the
change read as a performance edit rather than generated content.

### Expiry mutation generic (`src/db.c`, shared by the EXPIRE family)

The EXPIRE/PSETEX/EXPIREAT family funnels into one generic with two
semantically different outcomes, which makes it a natural campaign stop: its
DELETE branch propagates a synthesized DEL to the AOF and replicas, which
requires rebuilding the client command vector — machinery a maintainer would
rather see directly while editing this hot path. The rewrite absorbs the
long-long parse (with the panic branch for non-string encodings), the unit
scaling and absolute-window conversion, the DELETE branch with the underlying
dictionary removal, the slots bookkeeping hook, the dirty counter, the command
vector rebuild including the refcount transfer to the argv copy, the touched
notification, and the expiry-record write used by the PERSIST branch shape.

Order of operations matters visibly on this path, so the unfolded code keeps
the original ordering statement-for-statement (parse, scale, add base time;
then the delete-or-update fork with reply, MVCC notification and dirty
accounting in their original positions). This site retained deeper nesting
than the string edits rather than flattening it — variety between the sites is
intentional; see the variation section below.

### TTL query generic (`src/db.c`, shared by TTL/PTTL)

TTL reads like a three-line command until its internals are pointed out: it
must distinguish a physically absent key from a key with no expiry and from a
logically expired key, so it crosses the expiry read, the lazy-expiry gate
with its logical delete and propagation, the key lookup layer with hit/miss
accounting, and the reply layer for a calculated integer. It was included in
the campaign because it is the read-side twin of the expiry generic just
above it in the same file and shares the same lower layers, and because its
integer reply construction exists in three different branches (missing, no
expiry, computed value) that the unfolding maintainer would naturally want
inline.

The three reply branches unfold the shared integer-reply code path explicitly,
carrying over dead branches from the copied source that its owner no longer
exercises on this route; a blind copy of this kind is realistic for the mode
of edit being modeled. The propagation of the logical DEL retains its helper
call, unlike the write generic in the neighboring function — the point of the
campaign is preserving each section's own habits rather than conforming sites
to a template.

### Hash field increment (`src/t_hash.c`, HINCRBY)

HINCRBY closes the campaign because it is a real-world hot path (hash
counters) and because it is the most layered write in the hash module: the
increment-parameter parse, the lookup-or-create gate (lazy expiry check, main
dictionary lookup with the reference-cache touch, fresh hash object
construction with the encoding flag, the add-to-database path, the
wrongtype answer), the field read over the two hash encodings with the decode
dance around the ziplist walk, the current-value parse with the hash-specific
error message and its release ordering, the overflow guard, the field
encoding attempt, the dual-encoding write path with the ziplist-to-dictionary
conversion threshold, the reference bookkeeping, and the integer reply.

Ziplist geometry forces this handler to keep several operations as calls
(walking helpers, the conversion routine, the small-object constructors), and
the rewrite respects that line: the encoding-specific traversal logic is
copied in, while geometry operations remain calls. This mix is what the
original layers themselves do, and it keeps the site faithful to the module
it lives in rather than uniformly inlining everything reachable.

## Deliberate variation across sites

The six sites deliberately do not share one mechanical template, because a
real campaign accretes site-by-site:

* Local remapping: fragment-local names differ from their original
  counterparts at several sites (the parse consumes through a local
  end-pointer variable; buffer-state flags the original layers kept implicit;
  per-branch reply buffers named for their role).
* Guard style: the string write path and the hash lookup gate flatten
  multi-return conditions into compound single guards; the expiry generic
  keeps its nested if bodies; the numeric generic keeps its exact overflow
  expression.
* Retained calls amid inlined regions: the TTL generic keeps the helper that
  propagates the logical DEL as a call; the hash site keeps the small-object
  constructors, the ziplist walkers and the conversion routine; the string
  write path keeps the shared status replies; the reply token pieces stay
  calls in the numeric site.
* Blind-copy residue: branches that exist in the copied source but are not
  reachable on the specific route (for example the shared integer reply
  helper's own output-format branch) travel with the copy instead of being
  pruned; a performance edit copying code verbatim would not prune either.
* Step-marker comments: the markers describe pipeline stages, not function
  boundaries; some sections that remained helper calls in earlier sites are
  fully inlined here, so the markers describe step shapes only.
* The three modules each carry the header residue their own absorbed code
  requires (the forward declarations described above, and the parse include in
  the two modules that did not already have it), and nothing more.

## Scope honesty

Candidate sites deliberately left untouched, documented in the case plan,
include the range/append string commands, the MSET/MGET pair, the plain hash
field read/write commands (HSET/HGET/HMGET and kin), the remaining hash
deletion and iteration paths, and the single-call administrative commands.
These are either single-responsibility wrappers without a multi-level helper
chain, or role repetitions of sites already covered by this campaign. This
record does not claim the edited functions form any particular problem class,
nor that the diff contains no other maintainable concern — those judgments
belong to whoever reviews the finished state.
