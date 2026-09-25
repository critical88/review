# Injection design record — toast presentation option inheritance

## Maintenance motivation

Sonner lets every value that shapes how a toast looks be supplied at several
layers: the toast record itself, the `toastOptions` a `<Toaster>` is configured
with, the `<Toaster>` component's own props, and, for updates of an existing
toast, whatever the toast already carries. Over the life of the project that
inheritance grew by accretion - each option got its precedence where it
happened to be needed, spelled with whatever operator felt natural at the
time. Recent maintenance had two recurring irritations:

1. **Clarity in the toast component.** The JSX attributes rendering a toast
   carried inline expressions of growing subtlety - tri-state booleans that
   mean "unset = do what the toaster does", styling opt-outs that combine
   three inputs, an icon chain with a configured-set step in the middle.
   Reviewers asked repeatedly for these to be spelled out instead of being
   decoded in place.
2. **Update stability in the store.** Updating an existing toast (which is
   what promise flows and repeated `toast()` calls with a reused id do)
   replaced the whole record, so an update that restated only its message
   dropped presentation fields the toast had been given earlier. Subscribers
   and `toast.getHistory()` saw a stripped-down copy of the toast.

The change series below models how a codebase with those two irritations
actually evolves: independent waves, each landing in the module its author was
working in, with nobody chartered to look across the tree.

## Modeled development evolution

Four ordinary waves, in no particular order of blame:

- **Wave A - skin helpers.** While clarifying what `data-styled`,
  `data-rich-colors` and `data-invert` really mean, the three resolution
  expressions were lifted out of the toast component into a small
  presentation module. Each helper names its precedence in prose so the
  attribute expressions stop being a puzzle.
- **Wave B - icon resolution beside the assets.** During work on built-in
  icons, the icon chain was co-located with the type-to-asset mapping it ends
  in. The reasoning was purely local: icon selection reads the built-in
  assets, so it belongs next to them.
- **Wave C - wiring helper for `toastOptions`.** While touching the `<Toaster>` prop
  wiring, the two options that layer per-toaster configuration over component
  props (`duration`, `closeButton`) got a helper that does the layering in
  one place - and it was filed next to the option declarations it reads.
- **Wave D - store update preservation.** The update-stripping bug was fixed
  inside the store, which is where the record is rebuilt. The fix enumerated
  which stored presentation fields carry over when an update does not
  restate them.

Each wave is the kind of change a real maintainer merges without hesitation:
localized, well-commented, and improving the code it touched. The compound
effect only becomes visible from outside any single file.

## Overall design of the changes

Every moved or added piece keeps the runtime semantics of the value it
computes exactly - the same sources, in the same priority order, with the same
falsy and nullish behavior, including the corner where an explicit `null`
icon must not fall back to the stored one. The re-pointed expressions in the
toast component receive the same inputs as before; the store fix adds
carry-over only for fields an update leaves unspecified. The public surface -
the exported `Toaster`, `toast`, `useSonner` and the shared types - is
untouched, as are positioning, swipe handling, timers, theming, history
limits and the stylesheet.

Individual additions were deliberately given different structural homes and
shapes rather than a uniform pattern, because real maintenance work rarely
repeats one template five times. The homes chosen are, concretely: a fresh
module for extracted resolvers, co-location beside the asset mapping, an
exported helper beside the type declarations, a private helper inside the
store class's module, and - left in place on purpose - the fragments that the
waves did not get around to converting inside the toast component itself.

## Per-location record

### `src/appearance.ts` (new module)

**What changed.** Three module-level resolvers were introduced. Each takes the
toast record plus the toaster-level counterpart of one option and returns the
value that wins: `invert` (toast-set or toaster-set, `||`), `richColors`
(toast-set or toaster-default, `??`), and `styled` (built-in styling applies
unless the toast brings custom JSX, opts out, or the toaster is configured
unstyled). doc comments above each resolver explain its precedence in prose.

**Why this site and shape.** A fresh module was the natural landing spot for
extraction work whose author was thinking about "the skin of a toast" as a
concept: the helper bodies are pure (record + configuration in, value out),
so they read as a unit and the presentation vocabulary gets a filename.
Arrow-function consts with one `return` mirror the terse helper style already
used elsewhere in the package.

**Production role.** These resolvers feed three data attributes on every
rendered toast. The toast render path calls them at the same spots where the
inline expressions used to be.

### `src/assets.tsx`

**What changed.** The icon chain (an icon the toast carries itself, then the
per-type icon configured on the toaster, then the built-in asset for the
toast's type) was lifted next to the built-in mapping it ends in. The type
import gained the icon-set type and the toast record type the helper uses.

**Why this site and shape.** The module already owns the type-to-icon mapping
and re-exports the components' icons; the author of the icon work considered
that the only module that needs to know both the fallback chain and the
built-in assets. The chain stayed a single expression with optional chaining,
as before - only its home moved.

**Production role.** Icon selection for non-loading toasts. One earlier inline
expression in the component now delegates to it.

### `src/types.ts`

**What changed.** An exported wiring helper that layers per-toaster
`toastOptions` over the `<Toaster>` component's own props for the two options
where that layering exists (`duration`, `closeButton`), placed directly below
the declaration of the options interface it reads from.

**Why this site and shape.** The declarations of `ToastOptions` and
`ToasterProps` are the shared vocabulary for those options; the author wanted
the layering rule visible exactly where the options are declared. The
parameters are typed against the component-props interface rather than giving
the options interface a second public life. The helper is exported so the
component can import it.

**Production role.** Runs once per mounted `<Toaster>`; the component body
calls it and hands its two results to every rendered toast.

### `src/state.ts`

**What changed.** A private helper beside the store class that decides,
per presentation field, whether an update's value or the stored record's
value carries over: six fields (`richColors`, `invert`, `unstyled`,
`closeButton`, `classNames`, `icon`), with an explicit identity ternary for
`icon` because `icon: null` is meaningful and must win over the stored value.
The class method that rebuilds an existing toast record now computes this
carried-over presentation once and includes it in both the published and the
stored form of the updated record.

**Why this site and shape.** The bug was observed in the store's output, and
the record merge is the store's own logic, so the fix was shaped as
store-local knowledge: a module-private function of the stored record and the
incoming update, used where the merge happens. Making it public would have
advertised store internals nobody outside consumes; keeping it beside the
store keeps the merge self-contained. The `??` operator encodes "unspecified
in the update means keep", and the one ternary encodes the discipline the
plain operator could not express.

**Production role.** As soon as a toast is created with the id of a live
toast - which is how promise flows advance, how `toast()` updates by reuse,
and how StrictMode double effects replay - subscribers and `getHistory()`
keep seeing a toast with the same presentation it had, instead of a copy
stripped of everything the update did not restate.

### `src/index.tsx`

**What changed.** The toast component's imports re-pointed to the new homes
above, and the corresponding render-site expressions delegate to them: the
invert/rich-colors/styled data attributes, and the icon selection. In the
`<Toaster>` body, the prop layering for duration and close button is now
computed once through the wiring helper and passed to every rendered toast,
replacing the two per-toast attribute reads. Everything else about the
component and the store subscription paths is as before.

**What stayed behind.** The conversions the waves did not cover are still
here, spelled inline exactly as they always were: the per-toast duration
appears twice (once to seed the expiry ref before mounting, once to
recompute it), the close-button visibility memo, and the two description
button styles (toast-set cancel/action button styling over the toaster-level
configuration). This is the halfway state such wave-based work leaves behind:
the sites whose authors had already moved on remain in their original shape.

**Why this site and shape.** The component is the caller of everything above;
its edit is wiring only. Keeping the five remaining expressions inline was
not a decision anyone made - it is simply the arithmetic of partial cleanup
across separate changes.

## Production role of the whole surface

The changed surface decides, for every rendered toast: how long it lives,
whether it can be closed and with which button, whether it is colored,
inverted, or styled at all, which icon it shows, and which button styles and
classes it applies - and, when a toast is updated rather than newly created,
which of those it remember carrying.

## Deliberate variation, summarized

| home | shape | knowledge it holds |
| --- | --- | --- |
| new presentation module | pure exported resolvers with prose precedence | skin option precedence |
| asset module | expression co-located with the built-in mapping | icon chain |
| type-declaration module | exported wiring helper below the interface | toaster-level layering |
| store module | private merge helper beside the class | stored-vs-update carry-over |
| toast component | call-re-pointed sites plus leftover inline fragments | expiry, close button, button styles |

The fragments differ in operator (`||`, `??`, ternary, negated chain), in
kind (numbers, booleans, React nodes, style objects), and in the pair of
layers they join - each wave expressed its precedence the way its author
spelled it, which is the part of this record a reader will recognize from
their own codebase.
