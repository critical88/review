# URL processing: the query and output paths got hard to follow

I maintain trurl, and recently the parts of the tool I touch most - the
query-string pair handling and the per-URL output code - became genuinely hard
to work in. Following what happens to a URL now means stepping through very
long routines that build things in place at every level: stretches that
normalize and decode query pairs twice over (a normalized copy and a decoded
copy), tend to pair storage, resolve component names, compute URL-part fetch
options from the tool's option state, and hand-roll the JSON string escaping for
the query-params array - all of it inline inside loops that also have their own
bookkeeping.

What bothers me is that this code base used to have a clear shape: small
single-purpose routines did exactly one step each - one decoded a pair, one
normalized it, one stored it, one mutated a single URL component, one fetched a
URL part with the right option flags, one escaped JSON strings - and the bigger
functions orchestrating the pipeline called them. Those small routines still
exist and are still correct; several of the big functions just stopped calling
them on their mainstream paths, at more than one place and at more than one
level of nesting. When I changed one of the option handling details last week I
had to find every inlined copy of the same logic by hand, and I don't trust that
I found them all. I want that clean layering back before we build more features
on top, because right now every decoding quirk and option sensitivity is
duplicated in place, and any of those copies can silently drift from the
others.

Please restore the delegation structure: the repeated in-place processing
should go back to living in small, well-scoped functions called from the places
that need them (using the existing helpers where they fit, or shaping new
single-purpose helpers where a slightly different boundary is the better
abstraction), and the duplicated inline blocks should be removed along the
whole pipeline - query pair handling during both parsing and command-line
update, the component mutation done per URL, and the output paths - not just
the first spot you find. The big functions should be left orchestrating: each
of their steps should read as one call whose name says what it does. Keep the
existing small routines working for the callers they still have.

Nothing observable should change:

* `make trurl` builds clean with the project's warning setup, and
  `make trurl && python3 test.py` passes with unchanged results.
* Command-line behavior is identical. A bare `+` in a query keeps being read as
  a space on the decode side while `%2B` survives as a literal `+`, empty
  query-pair pieces keep their slots, and `--replace`, `--replace-append`,
  `--sort-query`, `--qtrim`, `--set`, `--get` and `--json` all keep their
  current outputs - JSON output byte for byte.
* Decoded query values that contain a null byte keep being converted only in
  the display forms where that conversion already happens.
* The behavior where a URL-part fetch that fails because of a bad puny-code
  hostname conversion is retried once without the conversion option - and stays
  disabled afterwards for the rest of the run - is intentional; keep it intact.
* The default output, including the effect of options like default-port
  handling and keep-port, must not change for any URL.
