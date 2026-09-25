# Maintenance request

Every time someone wants to nudge how a short link name gets matched — accept another kind of trailing punctuation, change how dashes are handled, make the case-folding consistent — we end up opening a frustrating number of files. It feels like the rule for "turn this short name into the form we match on" ought to live in one place, the same way it did when there was a single helper everyone went through — but over a few feature additions it got inlined into each new caller.

I first ran into this in `db.go`; please start there and follow the related call path.

Please consolidate the repeated decision or policy behind one clear owner so the next change can be made in one place.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
