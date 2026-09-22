# Injection design record

## Maintenance motivation

trurl processes every command line as a stream of URLs, and its per-URL work is
dominated by a handful of loops: query pairs are extracted for every URL,
component values are fetched for every URL that gets printed, and each URL can
be rewritten by `--replace`/`--set`. The motivating complaint that the diff
models is the one maintainers actually raise about this code base as URL
throughput grows: the per-pair and per-component call chains run a deep nest of
small helpers (decode-then-re-encode a pair, decode only a pair, store a pair,
re-encode, resolve a component name, fetch a part with composed flags, escape a
string) once per pair or per component, and readings of profiles attribute a
cost to the call depth itself. A maintainer under that belief takes the
obvious shortcut: paste the callee bodies where the calls happen, keep the loop,
and delete nothing else. That decision is exactly what this change records.

## Development evolution being modeled

The change models a performance-driven maintenance edit, not a refactor. The
imagined developer kept the helper functions in place (cold paths still call
them), expanded the hot paths by pasting helper bodies at their former call
sites, adapted each pasted block to local variable names, and split some blocks
across guard labels and inner scopes to keep the strict build quiet. Where the
pasted logic contained a recursive tail, it was unrolled so the copy could sit
inside the loop body. The result looks like the kind of edit that survives
review because the tree still builds with `-Werror -Wall -Wshadow` and every
existing test still passes, while the code now performs its own sub-methods'
work in place at several levels of nesting. Nothing about the edit is annotated
as temporary; the commit message implied by the comments is "keep call levels
out of the per-pair/per-component loops".

## Overall design

The edit touches the query-pair pipeline (pair collection during URL ingestion,
pair update for `--replace`) and the per-URL output pipeline (default full-URL
printing, JSON emission) plus the `--set` component mutation loop embedded in
the per-URL driver. Four mainstream functions grow massively. The pasted
fragments are textual copies of the helper bodies: identical statement order and
control flow, adjusted only for local names, for the enclosing label/scope
structure, and where a recursive tail had to become an in-place repeat. Because
the helpers stay defined and still serve their other callers, the build retains
every symbol; because each pasted block keeps the original guard conditions,
runtime behavior is bit-for-bit what the helper calls produced. The diff stays
in one translation unit, which is where all URL processing in this project
lives; no test, documentation or build files are involved.

## Cluster 1: query-pair extraction routine

The pair-collection routine walks the raw query string pair by pair. Before the
change it resolved each pair through a chain of helpers: normalize the pair
(URL-decode it, then URL-encode it back with first-'=' splitting), decode the
same pair again for a decoded side-copy, and store both in the pair arrays under
a capacity check, with a final decoding step that turns '+' into spaces before
an unescape call. The change pastes all of this into the loop body. Each
pasted fragment retains its original structure: the decode-then-encode block
keeps its left/right/raw-pointer variables and its modified-detection
comparison; the decoded-copy block keeps its unescape call, length handling and
null-byte conversion for display; the storage block keeps the slot checks,
pointer/length stores, capacity warning and paired frees; the '+'-scan keeps
the same do/while over a memchr. Local names differ from the helpers' parameter
names, and error exits jump to a shared cleanup label inside the loop so no
pasted block leaks intermediate buffers. This cluster was chosen because it is
the deepest and most-often-iterated chain in the pipeline (five routines, each
called once per pair), and it is the natural first target of a
call-depth-avoidance edit. Its production role is the ingestion side: every
URL's pairs and their decoded copies come out of this loop.

## Cluster 2: query update routine for `--replace`

The content-replacement routine rewrites pair arrays that already exist. Before
the change it normalized update keys/values with the same
decode-then-re-encode helper pair the ingestion path uses (through a mid-level
convenience wrapper) and then did its own slot surgery. The change pastes the
normalized-copy construction and its decoding step directly into the key match
loop. The pasted block is written with its own z-prefixed locals so no outer
variable is clobbered at the `-Wshadow` level, keeps the same unescape/escape
calls and the same modified-detection comparison, and ends at the routine's
existing error exits. The decoded-variant and storage helpers keep being used by
their other callers; only the normalized-copy construction is repeated here.
This site was chosen because it is the pipeline's write-back half of the same
responsibility, so the paste feels locally justified ("the same normalize step
runs in the update loop, why call through three levels"), while it lifts
responsibility the routine did not own into it. Its production role is the
`--replace` output path.

## Cluster 3: per-URL component mutation loop (`--set` handling)

The per-URL driver loop that applies `--set` assignments used to hand each
`name=value` assignment to a single-component setter, which in turn resolved the
name through a component-name scan. Both are pasted into the loop: the setter
body with its conditional pre-check, its encode/conditional flag composition,
its IPv6-host special case, its `curl_url_set` call and its diagnostics for
unknown components and invalid syntax; and the name-resolution scan with its
length-and-prefix comparison. The setter copy keeps its original control flow
but writes to locally-threaded result variables instead of returning them, so
the enclosing loop can branch on them; the recursive-in-spirit retry of the
setter does not exist here, only the single-assignment body does. This cluster
was chosen because string-to-component resolution plus the setter body is a
two-level chain called once per assignment and once per URL, and inlining it
inside the driver loop compounds the per-URL failure and warning behavior with
name-resolution details the driver should not know. Its production role is the
`--set` write path driven per URL.

## Cluster 4: default full-URL fetch in the per-URL output path

When no JSON/formatted output is requested, the per-URL loop prints the final
URL, fetching it through a flag-composing part-fetch helper (default-port,
keep-port, encoding-preference, puny-code conversion, non-support-scheme
handling, plus a retry that disables IDN conversion on bad-hostname). The
change pastes the flag composition and the retry into the output branch,
keeping the helper's own parameter postures alive as locals (a local modifiers
variable frozen at the value the call used, a local part identity, a local
pointer-to-result), unrolling the helper's recursive retry into a direct second
fetch inside the branch. This site was chosen because it is the plainest
hot-path argument in the file - "we print one URL, one fetch, no layers" - while
the pasted flags expression plus retry is precisely the option-sensitive
behavior the helper exists to own. Its production role is the default output
path exercised by every non-JSON invocation.

## Cluster 5: JSON per-component fetch loop

The JSON output routine fetches each URL component through the same
flag-composing part-fetch helper. The change pastes the same flag composition
and retry into the component loop here, with the same local postures and the
same unrolled retry. The loop's '+'-promotion handling and its decoded-copy
handling are untouched; only the fetch grows. This cluster was chosen because
repeating the same paste at a second call site is itself a realistic evolution
artifact: the developer who pasted once for the default output path has the
text in their editor and pastes it again for the next path that bothers them,
even though the loop runs per component rather than per URL. Its production
role is the JSON component-parts emission.

## Cluster 6: JSON query-params emission

The params array emission used to push pair keys and values through the JSON
string escaper (quote, backslash, control characters). The change pastes the
escaping case dispatch twice - once around the pair key, once around the pair
value - inside the params loop, dropping the escaper's lowercase parameter at
both copies because neither call site ever wanted it, and threading the escape
through the same stdout writes the rounded routine used. The full-URL and
component-part emission keep their direct escaper calls, so the routine no
longer has a single consistent way to write a JSON string. This cluster was
chosen because output duplication is the maintenance trap this smell class
lives in: two pasted variants of an escaping policy in one output function
guarantee they drift. Its production role is the JSON query-params emission.

## Why these sites and not others

The chosen sites cover the pipeline end to end (ingestion, update, write, two
distinct output paths) so the pasted bodies carry genuinely different semantic
work rather than variations of one fragment. Helpers with a single caller, the
statistical/error-reporting printers, and leaf transforms (path canonicalization,
sort, trim) were left alone: pasting them would have orphaned the last caller of
a helper and produced exactly the dead-function breakage a `-Wall -Werror` build
catches, so their inlining had no plausible review-worthy shape and would have
read as construction noise rather than maintenance history.
