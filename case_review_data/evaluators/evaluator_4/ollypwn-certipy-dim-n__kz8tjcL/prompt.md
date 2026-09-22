# Repeated helper code folded into the main flows

Over the last rounds of debugging, several of Certipy's main flows grew very
long functions, and we are paying for it now.

While chasing a confusing enrollment failure, a broken NTLM handshake, and a
template-reporting mismatch, we kept copying the bodies of the small step
helpers *into* the flows that call them, because single-stepping one function
was easier than hopping between the flow and the library modules. The helpers
were never removed -- other callers still import and use them -- so today the
same logic lives twice inside the package: once in the flow, once in the
still-working helper it was copied from.

Symptoms we see right now:

* Reading any of the affected flows is hard: intervals of one-off routing and
  logging are interleaved with long stretches that read exactly like the body
  of an existing library function.
* When the PFX packaging policy changed recently, the packaging logic had to
  be edited in more than one place, and one of the copies was nearly missed.
* The conversion CLI contains the same PEM/DER load-and-parse fallback chains
  as the helpers it was copied from, with local variable names bound from the
  option values first so the copied text matches the original parameter names.
* The HTTP NTLM retry path and the enrollment request assembly each contain
  hand-copied protocol message construction, so every future fix to a shared
  behavior (message layout, wrapper selection, property extraction) has to be
  remembered and applied in two places.

We want the flows back to orchestrating instead of re-implementing: every
affected flow should locate the still-existing helper that a copied stretch
duplicates and call it, adapting arguments where the copy had re-bound them.
The shared helpers should stay where they are for their other callers, but no
flow should carry a second copy of a live helper's body. Where a stretch was
copied with small intermixed flow-specific edits, those flow-specific edits
belong back in the flow around the restored call, not inside a copy.

Behavior must be preserved exactly:

* CLI option handling for certificate, private key, PFX, and export
  conversions is unchanged, including PEM/DER/PFX acceptance and the PFX
  encryption policy (unprotected container without a password, the legacy
  compatible settings with one).
* Enrollment request assembly is unchanged: the CSR construction, the CMC
  wrapper variants (renewal, on-behalf-of, key archival), request attribute
  handling, and issued-certificate processing behave exactly as now.
* Template-derived report fields and their parsing behave exactly as now.
* The NTLM Type 1 / Type 3 messages, including the channel-binding handling,
  are byte-for-byte what they are today.
* There are already offline tests covering these paths; they must stay green,
  and no new test or behavior change belongs in this cleanup.

Please investigate broadly rather than stopping at the first long function:
this pattern was introduced the same way in several unrelated parts of the
tool, and we want every flow that absorbed helper bodies put back on the
helpers.
