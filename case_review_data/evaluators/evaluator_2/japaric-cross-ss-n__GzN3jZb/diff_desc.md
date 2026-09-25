# Injection design record — scattered target-to-platform decisions in `cross`

## Maintenance motivation

`cross` runs the user's build inside a container image, so almost every
feature eventually needs the same fact: *given a Rust target (or a toolchain
host, or an engine report), which container platform does that imply?* The
repository answers this question centrally — the docker module's image code
defines the platform types together with the conversions that turn a target
triple into an `Architecture`/`Os`/variant pairing, including the error text
emitted for triples it cannot resolve.

The repository is also distributed in its usage matrix: hosts range from the
two mainstream architectures to powerpc, riscv and s390x CI fleets, and
formats range from rustup triples to strings printed by a container engine.
During normal evolution, several unrelated features landed in the same
window of time, and each one touched a *different* call path that needed this
platform fact. The shared conversions were of course available, but each
author was focused on making their own immediate matrix work, and the
mainstream platforms are exactly the ones where a locally written decode gives
identical results — so nothing obvious broke. The outcome of that evolution is
the situation modeled here: a series of small, locally reasonable-looking
decoders spread across the code paths that decide anything about a container
platform.

This diff is a record of that evolution. It models a few weeks of pull
requests in which the knowledge "how does a target string become a platform"
got repeatedly re-derived at the point of use instead of being delegated to
the one place that owns it.

## Overall design

The injection adds a local, hardcoded target-to-platform decode to six
production sites across four files, covering four distinct responsibilities
that decide or consume a container platform:

- toolchain handling (three methods of `QualifiedToolchain`: `custom`,
  `with_picked`, and `parse`);
- image selection from configured tables (`get_possible_image`);
- container engine introspection (`engine_info`);
- the top-level build pre-flight warning (`run`).

Two properties hold at every site:

1. **Local decode, shared fallback.** Each fragment recognizes only the
   component tokens its author cared about, and defers to the repository's
   shared target-to-platform conversion for everything else. Unusual hosts
   therefore still resolve (or fail) exactly as before — behavior is
   preserved for exotic platforms, and the fragments are visibly part of the
   successful-path logic rather than dead code.
2. **Behavior preservation.** Local decodes deliberately cover only the
   token/component combinations whose result is provably the same as what the
   shared conversion produces for that input. All decisions about unrecognized
   components — including malformed triples and error wording — stay with the
   shared conversion.

The supporting work in each file (imports, the platform type usages) is the
smallest possible glue for the fragment; the remainder of each function keeps
its original control flow and error handling.

## Per-location record

### `src/rustc.rs` — `QualifiedToolchain::custom` (custom toolchain qualification)

- **What changed:** The `rustc -Vv` branch of `custom()` now decodes the
  reported host string by hand. It converts the host into a `TargetTriple`,
  splits off the leading component to identify the architecture, walks the
  trailing components to find the OS position, augments arm with the
  appropriate variant, and assembles an `ImagePlatform` from the parts; any
  component it does not recognize sends the whole decision back to the shared
  conversion.
- **Why this site:** `custom()` handles non-rustup toolchains (bisected or
  locally built), where the only platform information available is the raw
  `rustc −Vv` host string. This is the natural first place an author writes a
  decode, because the input is a bare string rather than an existing typed
  value.
- **Why this shape:** An author optimizing for "make my CI hosts work" writes
  the cheapest decode that accepts the handful of hosts their fleet runs: an
  explicit first-component match for the architecture, a right-to-left scan
  for the OS field (mirroring how triples end in `-<sys>-<abi>`), and an
  `Option` fold over both so that partial decodes never produce a platform.
  The fallback-to-shared-conversion shape reflects that the author knew the
  shared machinery existed and used it as a catch-all rather than duplicating
  its error handling.
- **Production role:** Qualifying a custom toolchain's usable platform is a
  prerequisite for every build made with a custom toolchain.

### `src/rustc.rs` — `QualifiedToolchain::with_picked` (toolchain override resolution)

- **What changed:** When configuration picks a toolchain with an explicit
  host (`Some(host)`), `with_picked()` now derives the platform by matching
  the triple's components: a leading-component architecture match, a
  right-to-left OS scan including the android-before-linux subtlety, and
  `starts_with` prefixes for the arm variants, all combined in a three-way
  match over `(architecture token, decoded OS, optional variant)` that
  constructs `ImagePlatform` values inline for the claimed combinations, with
  the shared conversion handling everything else.
- **Why this site:** Picked toolchains arrive from `Cross.toml`/environment
  overrides, so this path reuses the general triple grammar but with a
  different provenance for the string, and different authors (here: the
  "dated hosted nightly" feature) hit it independently of `custom()`.
- **Why this shape:** The three-dimensional `(arch, os, variant)` tuple match
  is what a careful author writes to combine several half-independent decodes
  into a single decision point; it looks rigorous, which is exactly why it
  survives review. The or-pattern covering three OSes for
  aarch64/arm64 shows a slightly newer fragment than `custom()`'s: a second
  author, a later week, a bit more ambition.
- **Production role:** Every build that pins or dates a toolchain in its
  configuration goes through this resolution.

### `src/rustc.rs` — `QualifiedToolchain::parse` (fully-qualified toolchain parsing)

- **What changed:** The `Ok(_) | Err(_) if config.custom_toolchain()`
  branch of the fully-qualified parser was extended so the dated-hosted
  builds note also decodes the toolchain's host triple: architecture
  component recognition for the big-power/riscv hosts, a derived
  linux-host boolean built from the trailing components, and linux-only
  platform assembly for the recognized tokens, deferring to the shared
  conversion otherwise.
- **Why this site:** `parse()` receives toolchain strings from rustup
  metadata, not from `rustc -Vv`: it is the third distinct provenance of the
  same string grammar. When rustup began publishing dated builds for
  non-mainstream hosts, the natural patch was to recognize those hosts here.
- **Why this shape:** The natural shortest patch at this site is a smaller
  vocabulary over a boolean (is this a linux-hosted build?) rather than a full
  OS enumeration, because the dated rustup builds the author cared about are
  all linux systems. The `(arch, linux-host)` tuple makes the fragment
  visibly different in structure from the other two sites in the same file,
  while still re-implementing a slice of the same responsibility.
- **Production role:** Toolchain qualification for the default-path builds
  — the most common configuration in practice.

### `src/config.rs` — `get_possible_image` (configuration-driven image selection)

- **What changed:** Where a configured image's accompanying toolchain list
  (the `CROSS_IMAGE_TOOLCHAIN`-style variables, holding comma-separated
  target triples) is resolved, the code now derives each list entry's
  platform by decoding the configured target string locally: a
  leading-component architecture vocabulary, a right-to-left OS position
  scan with the android-before-linux ordering, arm-variant prefixes on the
  full string, and a three-way token match assembling `ImagePlatform`
  values, again with the shared conversion absorbing unrecognized entries.
- **Why this site:** This environment-configuration code is where
  user-written strings (rather than strings from rustc or rustup) become
  platforms: a fourth provenance for the same grammar, in a different module
  with a different surrounding structure (a closure applied per list entry),
  so the fragment is naturally written in closure-local form.
- **Why this shape:** A per-entry decode inside a mapping closure is the
  locally reasonable choice when the enclosing function is already a chain of
  `.map` steps and the author wants to avoid restructuring the chain. The
  closure-local form also means the fragment is easy to read as a small
  self-contained concern.
- **Production role:** Resolving the `CROSS_IMAGE`/`CROSS_IMAGE_TOOLCHAIN`
  style of configuration (per-build and per-target) into the platforms used
  for every image override a user configures.

### `src/docker/engine.rs` — `engine_info` (container engine introspection)

- **What changed:** The os/arch strings a container engine reports about
  itself (e.g. `linux/x86_64`) are now translated by two local matches —
  one turning the OS string into an `Os` value for the common words,
  the other turning the arch string into an `Architecture` for the common
  tokens — with both falling back to the platform types' own constructors
  for anything else.
- **Why this site:** This is the only path where the platform fact arrives
  from *outside* the Rust target ecosystem, in engine vocabulary. The author
  of the "warn when the engine can't run the image" feature had two
  independent strings and needed one decision, and mirrored the commonly
  reported words rather than routing through the target-oriented conversion
  entirely.
- **Why this shape:** Two parallel `match` expressions over independent
  strings (no triple splitting at all) is the shape this context suggests:
  the engine reports already-separated fields, so the author pattern-matched
  each field separately.
- **Production role:** Deciding whether the local engine can run the image
  cross is about to use, before spawning any container.

### `src/lib.rs` — `run` (top-level build pre-flight warning)

- **What changed:** The image/toolchain compatibility warning now derives the
  toolchain host's architecture itself: the leading component of the host
  triple is matched against a token list, and for tokens outside the list the
  already-stored host architecture value is used, after which the comparison
  proceeds as before.
- **Why this site:** The pre-flight check compares the image's platform with
  the toolchain host's platform. Here the platform value to compare against
  already exists in typed form, which makes this fragment the easiest one to
  write — and the most instructive: the author re-derived a fact that was
  already available, purely because reading it off the triple felt simpler
  than navigating to the stored field.
- **Why this shape:** A single leading-component match with a stored-value
  fallback (no OS decode at all) keeps the warning path small; only
  architectures, the field the comparison actually reads, are handled.
- **Production role:** The compatibility warning printed before every build
  that pins both an image and a toolchain.

## Deliberate structural variation

The fragments intentionally do not share an implementation style. Splitting
on the first component versus scanning components right-to-left;
`Option` folds versus tuple matches; full OS enumeration versus a derived
linux-host boolean versus no OS decode at all; or-patterned OS groups and
`expect` on the claim path; per-entry closure decodes; parallel independent
matches over two already-split strings. This variation models how scattered
responsibility usually accumulates in production - from several authors over
several moments rather than from one template - and it means the sites cannot
all be recognized by looking for one fixed code shape.

## False trails

The repository already contains code that pattern-matches against the same
vocabulary but is not part of the scattered-decoding group described above:

- the platform types' serde alias tables and name-casing in the docker
  module's image code — the central mapping implementation itself, which is
  precisely the designated owner rather than a duplicate;
- the `FromStr` implementation for `Toolchain` in the rustc module, which
  splits toolchain strings on `-` to extract channel/date/host —
  toolchain-string grammar, not platform decisions, although it handles the
  same host strings that the decoders above consume;
- the static table of pre-provided images keyed by already-constructed
  platform values in the docker module;
- token mentions in test scaffolding and in the image-listing subcommand that
  take platform values from the two preceding sources.

Reader beware: a hasty text search for architecture tokens will find these
as readily as the fragments described in the per-location record.

## Behavior notes

All fragments defer to the repository's shared conversion whenever a local
decode does not fully apply, which preserves results and error text for
malformed and unusual inputs alike. The surrounding functions keep their
original control flow; the glue that brings the platform vocabulary into each
file is limited to imports.
