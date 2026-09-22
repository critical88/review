# Injection design record — actions-gh-pages workflow-input scatter

Repository: `peaceiris/actions-gh-pages` at commit `09d8f313dcc84179c9f28dbbbe1f06f3ca174008`
(TypeScript GitHub Action that publishes static site assets to GitHub Pages; 19 workflow
inputs declared in the action manifest: credentials, publish target, commit identity,
tagging, and Jekyll/CNAME handling).

## Maintenance motivation

This action grew from a small "copy a directory to a branch" utility into a
pipeline with fork handling, three credential transports, external-repository
support, exclusions, and release tagging. Every one of those features is
configured by workflow inputs. The repository's intended design keeps that
configuration story in one place: a dedicated input module parses all workflow
inputs into a typed inputs object and the pipeline consumes that object. The
typed contract is what makes the 19 declared inputs auditable — an input's
name, parsing rule, and default live in exactly one source file plus the
action manifest.

The change modeled here is a common decay of that contract during feature
work. A contributor touching one pipeline phase wants the value of one or two
inputs *right there*; the typed object is already in scope, but re-deriving the
value directly from the workflow input at the point of use feels like the
smaller, safer edit — no contract change, no threading a new field, no
re-touching the input module the last reviewer complained about. Each phase
that grew this way (`deploy_key` handling for enterprise users, orphan-branch
publishing, release tagging, fork heuristics) left behind a locally reasonable
read of the raw inputs. The result is the classic maintenance problem: the
input story is now written in many places, and every semantic change to one
input — a parsing fix, a default change, a rename — has to be found and
repeated per subsystem, per phase, or the phases silently disagree.

## Development evolution being modeled

The pattern is a half-finished "direct read" migration, not a removal of the
input layer. The input module still exists and still produces the typed object;
the orchestration still builds it and still passes it to subsystems (and even
still passes many per-input fields for logging or auxiliary uses). What
changed is that, phase by phase, several subsystems stopped *consuming* those
fields for their core decisions and started re-reading the raw workflow inputs
themselves — each with its own local idea of the parsing conventions and
defaults. That mixture (contract passed around but bypassed for decisions) is
what a real evolution of this codebase produces, because no single PR would
ever delete a working input layer; features simply route around it.

## Overall injection design

The edit distributes the *deployment-configuration reading responsibility*
across the pipeline phases that need it. Scope was chosen by surveying every
production module for where workflow-input-derived values participate in a
decision, and then implementing the bypass only where it can be done without
changing any observable behavior, because the raw-input read and the typed
field come from the same workflow invocation. The material fragments land in
three modules covering distinct pipeline responsibilities and lifecycle
phases:

- the top-level orchestration (`src/main.ts`),
- the credential/SSH-auth setup (`src/set-tokens.ts`),
- the repository preparation and commit/tag/push pipeline (`src/git-utils.ts`).

Each fragment is small (one to a few reads plus, in some phases, a local
copy of the boolean parsing convention) and is locally defensible with the
kind of comment its author would write. All exported signatures stay
unchanged so callers and tests keep compiling; in two functions the legacy
parameters stay declared but unused, suppressed the way this repository
already suppresses targeted analyzer findings elsewhere.

## Per-location rationale

### 1. `src/main.ts` — `run()`: fork-guard credentials and Jekyll re-check

What changed: the fork-decision block re-reads the three credential inputs
with direct reads, and the publish-branch preparation block re-derives the
Jekyll enablement decision with two direct reads plus an inline copy of the
boolean parsing convention, instead of using the corresponding fields of the
typed object it already holds.

Why this location: `run()` is the orchestrator and the natural first stop of
the decay. It already possesses the typed object, which makes the bypass
invisible to a reader skimming for missing data flow — the values still look
"available", just re-obtained. The fork heuristic (skip publishing on forks
without credentials) and the `.nojekyll` decision are exactly the kind of
late-arriving features that were bolted on here.

Why this shape: the credentials are read into locals and forwarded to the
fork helper unchanged, so the guard keeps its tested semantics; the Jekyll
pair is re-derived with a copy of the repository's boolean parsing expression
because the author "wanted the branch preparation to see the workflow exactly
as it runs". Production role: input-driven gatekeeping of a deployment
(whether it runs at all, and whether GitHub's Jekyll processing stays
enabled).

### 2. `src/set-tokens.ts` — `setTokens()`: credential-method dispatch

What changed: the auth-method selection re-reads the three credential inputs
directly for its chain of decisions and passes the re-read token to the
HTTPS token builder, while still passing the typed fields for the external
repository and publish branch parameters.

Why this location: the dispatch is the highest-stakes consumer of input
semantics — it decides which transport (SSH deploy key, generated GitHub
token, or personal token) authenticates the push. A contributor hardening
enterprise/fork support plausibly "distrusted" the pre-built snapshot here.

Why this shape: three sibling reads mirroring the dispatch order (deploy key
first, then generated token, then personal token), one branch each. The
mixed style — re-reads for the decision, typed fields for auxiliary data —
is deliberate: it is what a partial migration actually looks like, and it
makes the reader ask which source is authoritative. Production role:
chooses the push credential and wires it into the remote URL pipeline.

### 3. `src/set-tokens.ts` — `setSSHKey()`: deploy-key material

What changed: the SSH key file write takes the raw deploy-key input directly
instead of the key material field of the typed object.

Why this location and shape: the deploy-key path is the least-exercised
production path (only genuine SSH setups), so it is where a lone-maintainer
shortcut survives longest. A one-line substitution where a secret is
materialized reads as "be certain we write exactly what the workflow gave
us". Production role: writes the private key file that authenticates the
SSH transport.

### 4. `src/git-utils.ts` — module-level coercion helper

What changed: a module-local arrow function copies the repository's
boolean parsing convention for boolean-style inputs.

Why here: `src/git-utils.ts` is the module that consumes the most boolean
options (force orphan, keep files, allow empty commit). Rather than importing
an internal helper from the input module (which no such export currently
permits), the author re-declared the three-line convention locally. The copy
is deliberately a near-verbatim duplicate of the expression used by the input
module and by the orchestration fragment, so the same parsing policy now
exists in several production locations. Production role: local
interpretation of boolean-style workflow inputs.

### 5. `src/git-utils.ts` — `setRepo()`: repository preparation options

What changed: six preparation decisions — publish directory and its
workspace-relative join, destination subdirectory (including the relative-path
check), publish branch for clone and orphan creation, force-orphan
selection, keep-existing-files selection, and the exclusion list — are each
re-resolved from direct reads with the local coercion helper for the
booleans.

Why this location: `setRepo()` is the preparation phase that touches the most
inputs in one function, which is why it carries the largest cluster. Its
comment claims preparation should "match the checkout" rather than a "stale"
argument snapshot — precisely the rationalization that hides a bypass from
review.

Why this shape: fresh locals feeding the existing control flow unchanged, so
the clone/orphan/first-deployment paths keep identical observable behavior,
including the tested destination-path rejection. Production role: turns
target-directory configuration into the actual clone, orphan branch, and
asset copy that the deployment pushes.

### 6. `src/git-utils.ts` — `commit()`, `push()`, `pushTag()`: publication options

What changed: the commit phase re-reads the empty-commit allowance; the push
phase re-reads the publish branch and force-orphan selection; the tag phase
re-reads the tag name and tag message and re-derives the default
`Deployment <tag>` message locally. `push()` and `pushTag()` retain their
legacy parameters but no longer use them (with targeted suppressions, in line
with how this repository already annotates such cases); `commit()` keeps its
parameter but consults the raw input instead.

Why these locations: the versioning trio (`commit`, `push`, `pushTag`) are
sibling lifecycle phases of publication. Modeling one phase without the
others would leave the concept intact in its adjacent phases; covering all
three reproduces the real spread of a release-tooling change (adding tag
defaults, tightening force-push rules) that had to be implemented "in each
phase where it applies".

Why this shape: each function reads only the options it consumes
operationally, so the fragments stay small and the functions remain
signatures-compatible. The deliberately different handling of legacy
parameters (kept-unused vs kept-and-shadowed) varies the repair work per
site: each function needs its own decision about how to consume its
declarations again.

## Structural variation across fragments

The fragments intentionally differ in role and shape rather than repeating one
syntactic edit: string-availability checks (tokens, tag name), boolean-policy
re-derivations (jekyll pair, three booleans via the copied helper, one inline
copy), path/branch resolutions (publish dir, destination dir, branch clone,
push target), secret material handling (deploy key), dispatch-branching on
re-read credentials, default-value re-derivation (tag message), and three
distinct treatments of legacy parameters. This mirrors how an organically
grown scatter actually looks: same underlying responsibility, different local
vocabulary per phase.

## Scope decisions and exclusions

Explored but excluded fragments, with reasons:

- Parameter-asserted helpers in `src/utils.ts` (`skipOnFork`, `addNoJekyll`,
  `addCNAME`, directory helpers) and the argument-tested builders in
  `src/git-utils.ts` (`copyAssets`, `deleteExcludedAssets`, identity and
  message helpers) plus `src/set-tokens.ts` (`getPublishRepo`,
  `setGithubToken`, `setPersonalToken`): their runtime values are pinned by
  direct-argument tests (tests pass explicit credentials, booleans, paths,
  and message parts), so replacing parameter consumption with direct reads
  would alter tested behavior instead of preserving it. Their tests are the
  reason they stay contract-driven — a boundary worth preserving rather than
  working around.
- The input module itself: leaving it intact is what makes the bypass a
  *bypass*; deleting or weakening it would destroy the observed coexistence
  of the contract and its scattered readers.
- A scatter of the GitHub server-URL resolution was explored and dropped:
  only the credential setup naturally consumes it, so spreading it further
  would require fabricating consumers.
- A scatter of the credential-precedence chain was explored and dropped: the
  two clean derivations already sit close together and are tested, so a
  threshold rule keyed on them would have to split hairs between healthy and
  unhealthy near-duplicates.
- `src/index.ts` and `src/interfaces.ts` carry no input-driven decisions.

## Saturation

Remaining candidate sites would either repeat a role already covered
(availability checks, boolean policy, path/branch resolution, secret
material, dispatch, default derivation), land in behavior-pinned
direct-argument functions, or require invented consumers with no natural
reason to touch input values. The task therefore stops at the pipeline phases
that genuinely own input-derived decisions.
