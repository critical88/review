# Maintenance request

I sat down to add a new numeric-tone rendering option to this library and the change turned into a scavenger hunt before I even wrote any new behavior: figuring out where "how a syllable comes out for a given style" is decided sent me through several unrelated functions in several different files, and I still cannot say with confidence that I found all of it. another one, in its own finals file with the finals helper code and the j/q/x exception data, decides which styles get finals extraction and keeps the nasal-character special case; and the loop that collects and de-duplicates the rendered results takes its own cut for the first-letter form.

I first ran into this while working around `phoneticSymbol` in `phonetic_symbol.go`; please start there and follow the related call path.

Please consolidate the repeated decision or policy behind one clear owner so the next change can be made in one place.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
