# Refactor request: symbol knowledge should live with the symbol data

## Where this comes from

I maintain this barcode library. Most of my work over the past few releases
has been inside the paths that actually produce symbols for the Data Matrix,
QR and EAN encoding families — capacity planning for large symbols, the
error-correction pass, the EAN-13 digit and parity handling. Somewhere along
the way we started taking the same shortcut every time, and it has added up
into a pattern I no longer want to live with:

Whenever one of those passes needed a fact that is *about the symbol being
produced* — how wide the printed matrix is, how much payload a block or the
data region can carry, which bar pattern a digit contributes at a given
position — we usually just recomputed it on the spot from the raw fields of
the small record type that describes that kind of symbol, instead of asking
the record that already exists to describe exactly those facts. Some of those
little record types have carried the symbology's accounting (region
geometry, block budgets, version layout tables, the digit pattern tables)
since the library was written. A couple of newer helper types got added that
hold no data of their own at all but exist to churn somebody else's raw
fields into numbers or patterns, and some older hot passes now re-derive the
records' arithmetic inline, special cases included.

Nobody notices this kind of drift day to day because nothing about the
output changed. But the consumers now state the symbol's rules better than
the symbol types do, and the same facts are decided in more than one place
whenever someone reads those paths.

## What I would like done

Work through the symbol-producing paths of the Data Matrix, QR and EAN
encoders and put that knowledge back with the data it describes: facts about
a symbol should be computed by the record type that owns them (or behind it),
not re-stated by each consumer iterating over its raw fields. The concrete
arrangement is up to your judgment — I care about the placement of the
behavior, not about a particular shape. Where any pass-through scaffolding or
helper becomes unnecessary after the move, get rid of it rather than leaving
it behind.

Please find all places following this pattern in those paths and fix them
all, not just the most obvious example. I have not noticed the same thing in
the shared bottom-level utilities or the other symbologies, so I don't
expect changes outside those three encoding families.

## Behavior and compatibility requirements

- Everything must keep compiling with the normal Go build, and the full
  existing test suite must pass, unmodified.
- The barcodes produced for the same inputs must be exactly the same as
  before, bit for bit: rendered matrix layout, data region capacity and
  padding, error-correction codewords, digit bar patterns — none of it may
  change. One caution from someone who has been bitten by this before: the
  largest square Data Matrix profile does not split its blocks evenly, so
  beware of "cleaning up" block budget arithmetic into an average.
- The public API of the module stays as it is: the exported `Encode` /
  `EncodeWithColor` surfaces, the exported types, and every package's
  existing external behavior. Internal types and their methods are yours to
  reorganize freely; just do not add anything newly exported.
