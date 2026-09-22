# docuowl: document title and ID are being passed around everywhere

I was adding an optional `Locale` to document metadata and gave up halfway
through. Every place that touches a document's title or ID receives them as
two separate `string` values, and the pair has to be threaded through
function after function across the packages that parse frontmatter, build the
documentation tree, and build the search index. The tree nodes themselves no
longer hold the metadata record you'd expect — they keep two detached strings
and glue a record back together whenever a consumer asks for it, which at
least keeps the renderers compiling. The search side then pulls the same two
strings back out and re-implements the "ID falls back to the title" rule on
its own, so the rule now lives in more than one place.

That made the feature touch far too much code, and it reminded me of how the
pipeline looked before `fs` kept a metadata reference: identity data used to
move as one record and now it is disassembled everywhere.

Please clean this up properly. Something should own document identity once,
and the parts of the pipeline that parse, store, and index it should hand
that around as a unit instead of peeling it into bare strings at every
boundary — including the duplicated fallback logic the search side grew.
Make sure the behavior everyone observes stays exactly the same: frontmatter
parsing results, walk results and error messages, compound identifiers, the
search index, and the interface consumers outside these packages rely on.
The test suite should pass when you're done.
