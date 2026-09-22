# Cleanup in `learn-python`: the audio example behind the packages lesson

While updating the modules/packages lesson material for the current interpreter-only
notebooks, I finally read through the whole mock audio package that the packages
lesson demonstrates (the pluggable sound example under the module sources in
`src/modules`), and it still drags along the old Python-2 support machinery we
stopped shipping years ago:

* the package and its subpackages still carry import-time branches keyed on the
  runtime switches from that era — the ones that were pinned off once the
  exercise notebooks dropped the older interpreters — including one documented
  two-arm decision whose older arm can no longer win on anything we support;
* some of the individual reader/effect functions hold inlined Python-2 return
  paths that only those retired switches could ever have selected;
* one of the readers had its mock return (the one the current lesson text shows)
  put in front of the old form-chunk decoding logic, which is still sitting in
  the same function below it;
* under all of it sits a small helper module of Python-2-era readers, buffers
  and stream wrappers, plus the switch module itself, and outside the branches
  above nothing references them anymore.

None of that can run on any interpreter the lesson supports today, but students
still read this package as their worked example of how to structure one, and the
lesson tests import it directly. I'd like the package cleaned up so it contains
only what the current lesson actually exercises: retire the leftover Python-2-era
compatibility layer everywhere it lingers — the guarded branches and their
inlined variants, the leftover tail, the helper machinery, and the import-time
aliases and registrations they fed — without changing how the demonstration
behaves today.

Notes on what must stay true:

* The package's observable behavior must not change: the effect and reader
  functions keep returning their mock strings, and the import styles the lesson
  relies on (importing the package, its subpackages, their member modules, and
  the functions directly) keep working exactly as before.
* The full pytest suite must stay green; please don't modify tests or lesson texts.
* Keep the work inside the audio example package under the module sources; other
  lessons shouldn't be touched.
* Please treat it as a package-wide cleanup rather than spot fixes — the layer
  shows up in different shapes in different files, so check the package roots and
  every leaf module, and make sure nothing of the old layer survives anywhere
  (including leftovers that only the old layer ever used, such as modules or
  imported names whose consumers are all gone).
