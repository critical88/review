Subject: cleanup before the 9.x prep continues — staged rollout leftovers scattered around the codebase

While collating the changelog for the next release I finally walked through the leftovers of the feature-staging work we did over the last releases, and I kept coming across machinery that cannot actually do anything anymore:

- The multi-pass encoded-secret decoding work was supposed to gain a re-probe pass so keywords that only surface after a value is decoded would not be skipped in the next pass. That pass was parked behind a source-level toggle that has been pinned off the whole time, together with the helpers written for it.
- Around the change to how decoded segments are paired up, a backward-compatibility path was drafted for configs written against the old segment boundaries. It is controlled by a level constant that is deliberately held below the threshold at which it would kick in, and the pairing code sits in the codec layer below it.
- When the archive scanning depth option landed, a preview helper was drafted so callers could list what a deep scan would cover before opting into it. Only the actual archive walking made it in; the listing helper next to it never got a caller.
- During the multi-part (required) rule reporting work, small report output helpers were drafted and then never wired into the writers that sit right next to them.
- The remote-config extend support that is still marked as future work in the config code has a validation draft and a depth bound parked next to it, waiting for wiring that never happened.
- And when the hidden pre-commit-style `protect` command was being reduced toward the git scanning path, some exit-code compatibility constants and a mapping helper were kept around for wrapper scripts; the mapping never got called.

In other words: a fair amount of code that sits behind toggles that are literally constants in our source, or that lost its last caller when the surrounding feature shipped in a smaller shape. Every one of these regions reads like a supported path until someone actually traces the constants and callers — which several of us did while writing the release notes, and we would rather not have everyone repeat that exercise.

I'd like a full sweep of this residue. Please audit the detection engine (including its helper layer and the codec segment code below it), the file/archive source front end, the report writers, the configuration loading and extend code, and the command entry points, work out what can never execute at runtime or has no remaining caller, and remove all of it — including the leftover toggle and level constants that only exist to guard it.

Ground rules:

- Scanning results, report output, CLI flags, command behavior and exit behavior, and configuration handling (including the gitleaks.toml extend semantics) must be exactly what they are today. This is a removal, not a behavior change.
- Anything that still has a genuine caller from exercised code stays, including code that only tests exercise.
- The repository must keep building and the whole test suite must stay green.
- Don't leave fragments behind: if removing something orphans a constant, an import, or a comment block that narrates it, those go too. Likewise remove anything else in those areas that cannot ever run for the same staging reason — the list above is what we found, not necessarily everything that is there.
