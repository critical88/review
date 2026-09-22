# Injection design record — boombuler/barcode (feature envy case)

## Maintenance motivation

This repository is a pure-Go barcode library: every symbology (Data Matrix, QR,
EAN, Code128, PDF417, Aztec, ...) hides behind the small `Encode` /
`EncodeWithColor` surface, and everything below that surface is internal. That
internal layer is exactly where feature work happens. Over the last few years
of such work the library has repeatedly gained symbology-level capabilities
that are framed as *properties of the symbol being produced* — how many
codewords a block may carry, how wide the printed matrix is, which bar
pattern a digit contributes at a given position — while the data defining
those properties has always lived inside a small, long-lived struct per
symbology (`dmCodeSize`, `versionInfo`, the EAN digit records, and so on).

The realistic failure mode in this environment is not a bad rewrite of those
structs; it is feature iteration on the *production paths* that consume them.
When someone needs "the capacity budget for this symbol", the lowest-friction
move in a small internal package is to recompute it at the point of use from
the raw table fields, because that is where the new code is being written and
it avoids touching a shared struct that other call sites already read from.
Reviews see green output and identical encodings and the change lands. The
result is a production path that now knows a data holder's accounting rules
better than its own state — and that nobody has to notice on any given day,
because nothing observable changed.

## Modeled development evolution

The three change clusters below model three separate maintenance iterations
that were each reasonable on their own and then happened to land in the same
tree. Iteration one works on Data Matrix error correction for larger
multi-block symbols; iteration two works on the QR render and capacity path
while trying to give the render stage a single planning call; iteration three
works on EAN-13 encoding while restructuring how checksum and parity
information is consulted. They are related only by the *shape of the
shortcut*: in each iteration behavior that describes a data holder got
written next to its callers instead of consulted from (or given to) the
holder.

## Per-cluster record

### 1. Data Matrix error-correction pass (`datamatrix/errorcorrection.go`)

**What changed.** `calcECC` — the routine that takes the encoded data and
appends error-correction codewords for every block — no longer asks the
symbol profile for its budgets. It now reads six raw fields of the
`dmCodeSize` record it receives as a parameter (rows, columns, both region
counts, the ECC count and the block count) and re-derives, inside its own
block loop, the per-block data budget and the per-block ECC budget,
including the 144×144 special case copied out of the profile's own accessor
(more codewords in the first eight blocks than in the remaining two).

**Why this site.** Error correction is the heaviest consumer of the size
profile in the package: it repeats the profile's budget arithmetic once per
block. The data holder's whole purpose is to encapsulate that accounting,
and it still does so for the layout pass — which keeps using the accessors —
so the two consumers of the same numbers now disagree structurally
about where those numbers live. The special case is what makes the
duplication real rather than cosmetic: the caller re-states a
profile-specific rule that the profile already states about itself.

**Production role.** Every encoded Data Matrix symbol flows through this
pass; if the re-derived budget strays from the profile's accounting (most
plausibly in the 144×144 branch, the only size whose two block groups
differ), the ECC codewords are laid out for the wrong block sizes while
still padding out to exactly the same total.

### 2. QR frame planning type (`qr/versioninfo.go`, `qr/encoder.go`)

**What changed.** A new internal type `symbolPlanner` was added to the
version-table file. It holds no state. Its single method, `frameMetrics`,
takes a `*versionInfo` and returns two numbers — the module width of the
final matrix, and the byte capacity of the data region — computed from five
raw fields of the version record. The render routine and the padding
routine (`addPaddingAndTerminator`), which previously called the two
existing `versionInfo` budget methods, now construct a planner and take both
numbers from it.

**Why this site.** The render pass and the padding routine needed two
numbers that the version record already knows how to produce; giving the
version record one combined planning method would have meant shaping that
stable table type around the needs of two specific callers. Adding a
standalone planner next to the caller was the path that required no
negotiation with it. The new method passes the full block layout of the
version through raw fields to get there, and the type contributes no data of
its own to the calculation. Two stages of the QR pipeline — capacity padding
and rendering (which builds all eight mask candidates before choosing one) —
now take their geometry from the planner while the rest of the package
(`drawFinderPatterns`, `findSmallestVersionInfo`, the alignment-pattern
placement) still consults the version record directly.

**Production role.** These two numbers decide, respectively, how much
payload a code can carry for its version and how many modules wide the
rendered matrix is; both consumers run on every encode.

### 3. EAN-13 digit plan (`ean/encoder.go`)

**What changed.** EAN-13 digits do not all render the same way: the second
through seventh digits are drawn in the odd or even variant depending on
the lead digit's parity pattern. A small helper type `digitPlan` was added
to hold that parity pattern — copied from the first digit's record at the
start of encoding — together with a constructor `newDigitPlan` and a
method `barsAt` that returns, for a content position, which bar pattern of
the digit record to emit. The encoder now builds a plan from the lead digit
and asks it, per digit position, for that digit's pattern. The pattern
records themselves (`encodedNumber`: odd variant, even variant, right-hand
pattern, checksum pattern) are unchanged, and EAN-8 encoding is untouched.

**Why this site.** The variant-selection rule is stated entirely in terms of
the digit record's own fields — the plan contributes only the parity
sequence, which is itself a copy of one of those fields — so the rule lives
naturally with the record. Splitting it out into a helper that receives the
record read-only on every call was the easiest way to give the loop body a
name without deciding which type should own digit variants. It also froze
an implicit interface: a fresh plan per code, constructed from the first
digit but consulted for all twelve.

**Production role.** This is the core of EAN-13 encoding; every left-hand
digit's bar pattern is chosen through this path, and the checksum digit
computed in the public entry point is what seeds the parity row.

## Overall design note

The three clusters intentionally take different implementation shapes
(inline arithmetic in an existing hot path; a stateless planning type wired
into two callers; a stateful per-code helper with a constructor), because
that is how these shortcuts actually accumulate in real code, and because
the surrounding packages keep their existing internal division of labor as
genuine context: the Reed–Solomon encoder in `utils` consults the single
field it needs, the QR block interleave stays the loop over the split
codeword slices it always was, and the other symbologies' encoders keep
consulting their tables through their established entry points. All
computed output of the library is unchanged by this diff.
