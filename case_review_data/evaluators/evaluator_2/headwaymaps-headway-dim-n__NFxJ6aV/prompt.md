# Several functions have grown large and now repeat logic that already exists right next to them

I was on a maintenance pass over `headway` and kept tripping over the same kind of thing in three of our services. A handful of our larger functions got rewritten so they do, line for line, work we already keep in separate functions in the same file. The original functions are still there - and in some cases the code a few lines away still calls them for related cases - so the same logic is now sitting in two places. In two or three spots the duplication stacks: the new section itself runs the body of another helper all the way down, so one function carries what used to be three layers of separate work.

Where I ran into it:

- **The route endpoints.** Both the current (`v7`) and the older (`v6`) OSRM-compatible route APIs build the interior steps of a leg in a way that is far more than filling in a struct - it lays out the banner-instruction text, the maneuver-type/direction mapping, and the bearing computation by hand. The same building work is available as a function in the same file, and the first and last steps of a leg still use it, right next to the copied section. So we have the pipeline twice in each generation.
- **The step-by-step plan output (`v7`).** The transit plan's per-step loop produces each spoken instruction, the `Continue for ...` tail, the duration, and the geometry bearings itself. There's a per-step constructor in the same file that does exactly this work, along with the instruction and duration helpers it delegates to; the loop no longer calls it, but they're all still sitting there.
- **The GTFS tools.** One of our feed credential checks downloads and reads its zip inline, even though a sibling download routine that backs the feed-measurement path sits right beside it in the same file with the same machinery. The copy is annotated as kept in hand for a follow-up, but right now it does nothing the delegated call wouldn't.
- **The OTP router-config render.** The code that resolves a realtime feed's credential, and the code that names a stream's OTP updater, were pasted into the zone render pass. The module-level functions that defined the same logic are still present in the file.
- **The zoner's map page.** The endpoint that answers which feeds intersect a bounding box now builds its summary rows by hand and sorts them right after; the shared row builder our other endpoint uses is still right there, unused by this one.

What makes me nervous is that the copies don't match the originals any more: each one swapped a helper parameter for whatever was locally in scope (an `Option` here, a borrowed leg there), and each carries its own comment about why the inline form is the right one. So whoever changes one of the originals next will only touch one of the copies, and the two will silently diverge.

Please restore each of these so the logic has one home. Wherever a function embodies work that already exists as a separate function in its file, that function should own the work and the call site should delegate - or fold the two together so the logic lives once, not twice. Walk the three services and find every instance of this; the ones above are the obvious ones, but the same kind of mistake may sit in a module or two I didn't call out, and the work isn't done until they're all single-sourced.

Don't change the public shape of anything:

- the OSRM output, in both generations;
- the plan instructions and per-step durations (including the elapsed-time fallback and the treatment of zero-length legs);
- the redaction of any configured credential from verification errors;
- the OTP updater names and poll intervals, and the reasons recorded for skipped realtime feeds;
- the bbox listing's ordering and the fields it returns.

These all need to read exactly as they do today, and the whole repo's test suite should still pass.
