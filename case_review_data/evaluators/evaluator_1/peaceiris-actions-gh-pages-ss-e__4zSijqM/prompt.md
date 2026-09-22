# Refactor task: consolidate how this GitHub Pages action consumes its workflow inputs

Repository: `peaceiris/actions-gh-pages` (a TypeScript GitHub Action that publishes static
site assets to GitHub Pages), pinned at commit `09d8f313dcc84179c9f28dbbbe1f06f3ca174008`.

## Context and observed problem

This Action exposes nineteen workflow inputs (credentials, publish target, destination
subdirectory, commit identity, commit/tag messages, Jekyll/CNAME handling, exclusions,
and boolean toggles like `keep_files`, `force_orphan`, and `allow_empty_commit`). The
intended architecture is a single input-handling layer: one production module reads the
workflow inputs once, applies the parsing policies (boolean-style interpretation,
defaults, and the documented mutual exclusion between the Jekyll toggles), and produces
the typed inputs object that the rest of the pipeline consumes.

That contract has eroded across the publishing pipeline. Several subsystems no longer
trust the typed object for their core decisions and instead read raw workflow inputs
again at their point of use, each re-implementing whatever parsing convention it needs:

- the top-level deployment orchestration re-reads the three credential inputs for its
  fork-skip decision and re-derives the Jekyll/nojekyll decision with its own inline
  boolean interpretation;
- the credential-setup subsystem re-reads the same three credentials to choose the auth
  transport (SSH deploy key, generated GitHub token, personal token), and the SSH key
  path re-reads the raw deploy-key material it writes;
- the repository-preparation subsystem re-resolves the publish directory, destination
  subdirectory (including its relative-path guard), publish branch, force-orphan and
  keep-files toggles, and the exclusion list from the workflow inputs;
- the commit, push, and tag phases re-read the empty-commit allowance, the publish
  branch and force-orphan toggle, and the tag name and tag message — including a
  locally re-implemented default for the tag message — while some of these functions
  keep accepting (but ignoring) the corresponding arguments.

Where a boolean-style input is interpreted, the parsing convention is duplicated inline
or via a private helper instead of being owned once. The same input is therefore
interpreted independently in multiple pipeline phases.

## Why this must change

Every semantic change to an input — its parsing convention, its default, or its
meaning — currently has to be found and repeated in each subsystem that re-reads it,
and previous behavior fixes have been applied in some phases but not others. The
maintenance requirement is the standard single-responsibility outcome for this
erosion: input reading and input parsing policy must be owned by exactly one place,
and every subsystem must consume that result (the typed inputs contract) instead of
re-reading the workflow inputs.

## What to do

1. Investigate the TypeScript sources and find every place where a subsystem reads a
   workflow input directly or re-implements an input's parsing/default policy, rather
   than consuming the typed inputs object produced by the input-handling layer. The
   fragments are spread across the orchestration, credential, repository-preparation,
   and commit/tag/push phases — cover the whole publishing pipeline, not just the first
   site you find.
2. Return those subsystems to consuming the typed inputs contract: the values they act
   on must come from what the input-handling layer produces (function parameters or
   the typed object), so that the declared behavior of each input is defined in
   exactly one place. Function signatures in use by callers and tests must remain
   compatible with their current declarations, and parameters a function accepts today
   must keep being honored by its implementation.
3. Remove the leftover artifacts of the bypass: the duplicated local parsing helpers
   and the unused-parameter suppressions introduced by the scattered reads must not
   survive a complete repair.

## Compatibility boundary (behavior must not change)

- The workflow input surface is fixed: no input name, default, or documented semantic
  in the action manifest may change, and the input-handling layer must continue to
  produce the same typed object it produces today.
- Deploy behavior must be identical for every input configuration, including: the
  fork with no credentials skipping the deployment; the Jekyll mutual-exclusion error
  and the `.nojekyll` handling; the destination subdirectory relative-path guard;
  the auth-method precedence (deploy key, then generated GitHub token, then personal
  token) and the "not found" failure; the commit identity defaults and validation;
  the commit message forms including the external-repository variant; empty-commit,
  force-push, and orphan-branch behavior; the tag default message; and exclusion
  handling during asset copy.
- All existing tests must pass without modification; the exported functions you touch
  must keep accepting the arguments they accept today so that callers and tests keep
  working.

## How you will know you are finished

A single semantic change to any one input (for example, changing how a boolean-style
input is interpreted, or changing the default tag message) will require editing only
the input-handling layer, and no pipeline subsystem outside that layer reads workflow
inputs on its own any more.
