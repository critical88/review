# Maintenance request

I maintain a small CPU sampling profiler. It works like this: a signal handler interrupts profiled threads, captures each thread's backtrace, records the thread's name/id and a timestamp, and stores the sample raw — symbolication has to happen later, outside the handler. Lately every change I make in this area hurts, and last week's flamegraph tweak convinced me the layout has drifted somewhere wrong.

I first ran into this while working around `resolve_in_perfmap` in `src/frames.rs`; please start there and follow the related call path.

Please move the misplaced behavior closer to the data or component that owns it, leaving the surrounding coordinator focused on its own work.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
