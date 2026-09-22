# Please clean up the built-in-leaf plumbing in the deep utilities

Hi — a maintainer request, mostly born out of frustration.

Over the last cycle we merged a batch of small PRs that made the deep rewriting
utilities (`DeepPartial` and the rest of the "deep" types in `lib/`) configurable in
how they treat `Date`, `Error` and `RegExp` objects during traversal: instead of a
fixed per-utility decision about which of those built-in families count as traversal
leaves, each utility can now switch each family individually. Our users asked for
this — e.g. serializers that must never rewrite error fields, or API-response types
that want dates descended into as well. `DeepReadonly` had already offered this kind
of configurability via its override record.

The trouble is what the rollout left behind. Each utility grew its own set of
standalone boolean type parameters for the three families, its own private helper that
combines them into the traversal's leaf set, and — this is the part that hurts —
every recursive reference inside every utility now passes those parameters down one by
one, in each branch (maps, sets, promises, the array/tuple splits, the object mappers,
the iterator handling).

I tried to prepare a small demonstration for the docs — "here is how you keep Errors
unwrapped in each deep utility" — and had to write it nine different ways, because the
switch names and even the meaning of `true` differ from file to file. I then tried to
prototype what adding one more built-in family would look like, and gave up after the
umpteenth branch. Whatever design intent existed has been smeared out; the three
families form one decision in the user's mind, but in the code they are loose switches
that repeat in step after step of every recursion. `DeepReadonly`'s internal layers
were also reworked into this loose-flag style, even though its public override record
survived.

**What we'd like:** please restore some sanity by giving this built-in-leaf decision
one coherent representation in the deep rewriting family — the three Date/Error/RegExp
choices should be declared and passed as a unit, with each utility's default
configuration expressed in that same form, rather than as per-family switches
repeated at every recursion site of every utility. Follow whatever shape fits this
library best; we don't want to prescribe the exact type.

**What must not change:** every type the library computes today must come out exactly
the same — each utility must keep descending into exactly the built-in families it
descends into today, and keep stopping at the ones it stops at today. All existing
usage styles must keep compiling without new arguments and without different results —
plain usages, composite utilities built on these types, and `DeepReadonly`'s
object-style override record, which users depend on and which must stay the supported
way to configure readonly traversal. Public type names stay as they are.

**Where this lives:** the deep rewriting family under `lib/` — the utilities that
recursively rewrite a type's structure. The settings used by the path utilities
(recursion depth and array-index accessors) are unrelated to this and should be left
alone.
