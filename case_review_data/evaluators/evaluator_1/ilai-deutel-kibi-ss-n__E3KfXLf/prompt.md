# The line-number gutter is everywhere

I keep running into the same thing with the line-number gutter — the numbers
on the left of every row, with the dark grey vertical bar next to them.

## What I keep hitting

Every time I need to touch how the gutter behaves, the work lands in a bunch
of modules that otherwise have nothing to do with each other. Last time:

- I adjusted when the gutter is allowed to show up (it has to back off in
  narrow terminals), and the relevant decision came from the configuration
  handling;
- I then had to recompute how many columns the text actually gets, and the
  window-size handling had its own say in that;
- and the drawing itself — aligning the number, drawing the bar — turned out
  to be a third spot, over with the escape-sequence helpers.

And honestly I'm never sure I found every site.

Meanwhile, the rows themselves — where the width of this thing naturally comes
from — are modeled in one tidy place, and there's even a tiny value type
describing the gutter width defined there. It's a bit odd: the shape of the
data lives in one module, but every decision *about* the gutter lives
somewhere else. Any conceptual change (a smarter rule for narrow windows, a
different padding scheme) means remembering all of those spots and editing
them all consistently.

## What I'd like

Give the gutter's logic one home — with the row data model if that's the
natural place, or wherever it genuinely belongs — so the rest of the editor
just uses it from there.

I don't want the behavior to change one bit: the same conditions for showing
the numbers, the same alignment, bar and color, the same columns left for
text. Config files keep working the same way (the `show_line_numbers`
preference and everything else in `config.ini`), nothing public should break,
and the test suite should of course still pass.
