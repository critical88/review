# Supporting another package manager shouldn't mean hunting through the whole CLI

## Where this came from

I spent the weekend adding `pnpm` support to my fork of typescript-starter,
and it went badly in a familiar way. Every time I thought I was done, it
turned out one more part of the CLI had its own private idea of which package
manager it was dealing with.

The non-interactive path had mapped the runner flag onto raw
`'yarn'`/`'npm'` strings (re-typed back into the runner option), so that had
to change. Then the interactive prompt turned out to give its runner
question raw string answer values with the same re-typing, so that had to
change too. Later I hit the dependency-install step, which decides what to
spawn from a small local registry keyed by runner names, and the project
generation stage, which holds its own per-runner registries for the
generated script override and for which lockfile a project must ignore — all
of them restating the same "which package manager was selected" knowledge in
slightly different notation. I've probably forgotten a spot or two; I only
found them by running everything repeatedly.

What struck me is that the CLI already defines its notion of the selected
package manager once — the option pipeline is typed against it end to end —
so none of those restatements needed to exist at all. They're leftovers from
various fixes and refactors that each solved their corner locally.

## What I'd like

Please consolidate package-manager handling in the CLI so the facts about
the supported package managers live in one place — with the runner identity's
own definition — and the rest of the CLI just consumes them. The goal is that
adding, renaming, or otherwise changing a supported package manager is a
small, local change in that one place, not a coordinated sweep across every
lifecycle stage. I don't care what the resulting internal shape looks like
(function, data table, enum-keyed records, or something better); I care that
there's one obvious place to look and one obvious place to edit.

I'd expect the rework to stay inside the CLI area of the codebase. The
library sources and their public export surface have nothing to do with
package managers and shouldn't need to change.

## What must not change

This was a behavior-preserving cleanup in my fork, and I need it to be here
too:

- The CLI keeps working exactly as before: same flags and defaults (npm
  remains the default runner; the yarn flag still selects yarn), same help
  text, same interactive questions with the same runner choice labels.
- Dependency installation still invokes the same package manager command
  with the same effective arguments.
- The generated projects come out identical, byte for byte, for both
  runners — the `package.json` content and scripts (including the
  yarn-specific script override) and the `.gitignore` entry for the lockfile
  the project ignores.
- The library's public export surface and its compiled build output are
  unchanged.
- The project's existing strict TypeScript build and its formatting,
  spelling, and import-ordering conventions still pass with the reworked
  code.
