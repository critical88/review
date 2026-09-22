# Refactor request: the `.sens` recording type has accreted the whole pipeline

## Where this comes from

We maintain the SensReader C++ library under `SensReader/c++`, the reference
reader/writer for `.sens` captures. Sensor recordings are represented by
`ml::SensorData` in `SensReader/c++/src/sensorData.h`, with nested helper types
holding the frame data (`RGBDFrame`), the calibration values
(`CalibrationData`), and the write-out sequence counter (`StringCounter`). The
demo consumer lives in `SensReader/c++/src/main.cpp`.

Over several maintenance pushes, implementation work that belongs to those
other owners has drifted into `ml::SensorData` itself, so the recording type is
now the place where several unrelated pipeline concerns are actually
implemented, while the types that own the state became thin pass-throughs or
plain callers. Concretely, the suspicion to investigate covers several
distinct responsibility areas — among them the per-frame compression,
decompression and buffer-release work; the construction of the intrinsic
calibration matrix; the zero-padded image/pose file naming; the preparation of
the output directory on Windows/Linux; and the frame-decoding demo example
that consumers were previously expected to write themselves.

## Maintenance observation

The everyday symptoms are: a one-line change to frame handling or to the
export naming ends up as a diff in the recording type rather than in the type
whose fields it manipulates; the nested helper types no longer implement their
own responsibilities but hand off to statics of the enclosing class; the
recording type mixes codec backends, raw heap-buffer bookkeeping, binary file
IO, formatted string assembly, numeric padding arithmetic, and platform
directory calls into one declaration; and the consumer program calls the
library type to run even its "how to decode" example.

## What we want

Restore a cohesive ownership structure in the reader pipeline:

- Each distinct responsibility should be implemented by a well-scoped owner —
  normally the type (or helper) that holds the state the responsibility
  operates on — rather than being concentrated in the recording type.
- The data-owning types in the reader header should implement their own
  behavior again instead of existing only to forward to the recording type.
- Pipeline support utilities that are not specific to a recording (e.g. the
  platform directory helpers) should live at an appropriate non-member scope.
- The consumer program should own its own example code.
- Remove whatever scaffolding remains behind once the responsibilities are
  re-homed; do not leave duplicated or dead implementations of the same
  behavior.

This is a restructuring across a scope you need to determine yourself: study
the reader pipeline in `SensReader/c++/src/sensorData.h` and its consumer in
`SensReader/c++/src/main.cpp`, identify every place where the recording type
carries the implementation of a responsibility that belongs elsewhere, and
repair all of them consistently — not only the first one you notice.

## What must not change

This is a pure refactoring; observable behavior and the external contract of
the library must be preserved:

- The library and the demo reader program must compile exactly as before, and
  the demo must decode a `.sens` recording the same way.
- `.sens` serialization must stay byte-compatible: a file written before this
  restructuring loads afterwards with identical frame data, timestamps,
  camera poses, calibration values, sensor name, and compression types (and
  vice versa).
- Compression and decompression must produce identical results for every
  configured format path, including guarded codec branches; allocation sizes
  and ownership (caller frees returned buffers) stay the same.
- The image/pose export must produce the same output directory state, the same
  zero-padded sequence file names, and the same file contents as before, with
  the same Windows/Linux directory behavior.
- `CalibrationData::makeIntrinsicMatrix(fx, fy, mx, my)` must return the same
  matrix.
- The public API of the reader types (member fields, enumerations, and the
  members callers rely on, including the add-frame/IMU entry points,
  save/load, and the decompression helpers) must remain source-compatible for
  existing callers, including the tools and examples that consume this header.
- Error/exception behavior (e.g. `MLIB_EXCEPTION` paths) must be retained.

## Deliverable

A source patch that restructures ownership as described, keeps the build
(`cd SensReader/c++ && make main`) working, and preserves every behavior
listed above.
