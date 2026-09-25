# Maintenance request

I ran into this while trying to make a small change to how excluded links are decided. I expected one place that answers "is this link ignored?", and instead I found two: the exclusion-policy configuration that has always owned the decision, and — line for line, reading the very same settings directly off the configuration — the link-check pipeline itself. Once I saw that, I kept looking, and it goes further. The pipeline also evaluates remap rules by hand: it opens the remap rule table's internal storage, walks the rules, and does the matching and rewriting itself — the table's internals had to be made readable outside its module for that to even compile.

I first ran into this in `lychee-lib/src/checker/mail.rs`; please start there and follow the related call path.

Please separate the unrelated responsibilities that have accumulated here, keeping the central object focused on genuine coordination.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
