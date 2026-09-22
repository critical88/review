# Injection design record: package-runner knowledge in the typescript-starter CLI

- Repository: bitjson/typescript-starter (TypeScript), pinned revision `586cdb3029ab2c52e2f0893adafbbb017059e1c9`
- Injection surface: 4 production files, 43 insertions, 12 deletions (`smell.diff`)

## Maintenance motivation

typescript-starter scaffolds a new project from the user's own repository
history, then adapts the result for the package manager the user chose. npm and
yarn differ in small, boring ways: the command that installs dependencies, the
arguments that command wants, which lockfile the generated project must
ignore, and which `reset-hard` script the template already ships. The CLI has
to make a runner-specific decision at each of those points, and it has to know
which runner the user selected from two different front doors — a `--yarn`
flag for non-interactive use, and an inquirer list in interactive mode.

The realistic maintenance story being modeled is the arrival of a new package
manager (for example pnpm), or a rename/representation change to an existing
one. Preparing the tool for that change means an author must remember every
place the tool encodes a runner fact. At review time, "which places know about
runners?" has no single answer a maintainer can point at, because the
knowledge is not owned anywhere in particular — it is restated wherever a
stage happens to need it.

## The development evolution being modeled

The pinned revision is not the imagined starting point; the imagined starting
point is a hypothetical earlier state in which each stage consumed the
selected runner through the shared `Runner` enum only. The injection models a
series of small, individually defensible local edits of the kind real projects
accumulate:

1. an author touching the flag path found it more convenient to write the two
   identities directly as strings and bridge them to the option type with a
   cast, instead of referencing the enum;
2. a later author reworking the prompt valued inquirer's conventional
   `{ name, value }` shape as raw strings, adding the same kind of cast;
3. a data-driven-installs refactor replaced an enum-conditioned ternary in the
   install step with a small lookup table keyed by runner names;
4. the generation stage grew two per-runner registries (the scripts override
   and the lockfile swap), and its remaining runner check was tightened into
   a direct string comparison, consistent with the tables sitting next to it.

Each edit makes local sense; none changes behavior. Drift of exactly this
kind happens when reviewers ask "does it work?" and never ask "who owns this
knowledge?"

## Overall design

The option pipeline's type surface is untouched. `TypescriptStarterOptions`
still carries `runner: Runner`, and the enum remains defined and exported in
`src/cli/utils.ts`. What changes is how each lifecycle stage *produces* or
*consults* runner knowledge:

| Stage | Production role | Injection shape |
| --- | --- | --- |
| `src/cli/args.ts` (`checkArgs`) | non-interactive flag parsing | raw literals re-typed: `'yarn' as Runner` / `'npm' as Runner` |
| `src/cli/inquire.ts` (`inquire`) | interactive prompt | raw inquirer values re-typed: `'npm' as Runner`, `'yarn' as Runner` |
| `src/cli/tasks.ts` (module scope, used by `install`) | dependency-install dispatch | module-level table `{ npm: ['install'], yarn: [] }` consulted by indexed lookup |
| `src/cli/typescript-starter.ts` (module scope, used by `typescriptStarter`) | generated-project adaptation | module-level tables `runnerResetHardScripts` and `runnerIgnoredLockfile`, plus a raw `runner === 'yarn'` guard |

The design deliberately mixes four syntactic manifestations of the same
underlying hand-off — two literal-to-enum assertions at the input boundary, an
indexed dispatch table at the execution step, value registries plus a string
comparison at the generation step — because that is what organic drift looks
like: each site uses whatever expression the local author found natural.

## Per-location rationale

### `src/cli/args.ts` — non-interactive runner selection

`checkArgs` maps boolean flags to the `Runner`-typed options. The injected form
derives runner identity at the flag site instead of using the shared enum:

```ts
runner: cli.flags.yarn ? ('yarn' as Runner) : ('npm' as Runner),
```

This site was chosen because it is the first place runner identity is
*created*. A cast is the honest local shortcut an author writes when the flag
layer speaks strings and the options layer speaks enums; it keeps the diff at
one line and compiles cleanly under strict mode, which is why such a line
survives review. Role served: converts `--yarn` into the selected runner.

### `src/cli/inquire.ts` — interactive runner choice

The runner question offers `{ name, value }` pairs. The injected choices carry
raw inquirer values re-typed to `Runner`, with `name` and `value` kept
identical so the prompt renders and answers exactly as before:

```ts
choices: [
  { name: 'npm', value: 'npm' as Runner },
  { name: 'yarn', value: 'yarn' as Runner },
],
```

This site was chosen because it is the *second, independent* birthplace of
runner identity — interactive use never executes `checkArgs`' selection. The
same cast shape as `args.ts` is reused with slightly different surface syntax
(parenthesized there, bare here), reflecting that two authors wrote them apart
in time. Role served: presents the runner question and yields the answer the
options pipeline expects.

### `src/cli/tasks.ts` — dependency-install dispatch

The `install` factory previously branched on the enum. The injected state
separates per-runner command *data* from the call site using a module-level
table:

```ts
const runnerInstallArguments: { readonly [runner: string]: readonly string[] } = {
  npm: ['install'],
  yarn: [],
};
...
spawner(runner, runnerInstallArguments[runner], opts);
```

This shape was chosen because "make it data-driven" is a plausible pull
request in its own right: the spawn target comes from the runner value while
the arguments come from a local registry keyed by raw runner names. Execa
normalizes `('yarn', ['…'], opts)` and `('yarn', opts)` identically, so the
effective spawned command is unchanged. Role served: runs the selected package
manager inside the generated project.

### `src/cli/typescript-starter.ts` — generated-project adaptation

The generation function adapts two template files per runner. The injected
state hoists both adaptations into module-level registries and narrows the
remaining runner check to a raw comparison:

```ts
const runnerResetHardScripts: { readonly [runner: string]: string | undefined } = {
  npm: undefined,
  yarn: 'git clean -dfx && git reset --hard && yarn',
};
const runnerIgnoredLockfile: { readonly [runner: string]: string } = {
  npm: 'yarn.lock',
  yarn: 'package-lock.json',
};
```

with consumption sites `runnerResetHardScripts[runner]`, the `.gitignore`
entry swap `to: runnerIgnoredLockfile[runner]`, and the guard
`if ((runner as string) === 'yarn')`. The `as string` widening mirrors the
npm-branch `undefined` lookup: with a string-indexed table the guard must
compare the escaped string. This cluster was chosen because it is the output
boundary — where runner facts become concrete generated bytes — and because a
raw comparison alongside raw-keyed tables is the natural way a table refactor
actually gets written. Roles served: emits the per-runner `package.json`
scripts and `.gitignore` lockfile entry.

## Behavior notes

Every path through the CLI produces the same observable results as before:
same flags, defaults, help text and prompt labels; same effective install
commands; `package.json` scripts, `.gitignore` content and all other generated
files byte-identical per runner; same compiled library surface. Where syntax
had to change to keep those guarantees (e.g. `execa('yarn', [], opts)` versus
`execa('yarn', opts)`, optional-chaining on the `undefined` npm entry), the
equivalent form was verified to compile and to preserve the emitted value.
