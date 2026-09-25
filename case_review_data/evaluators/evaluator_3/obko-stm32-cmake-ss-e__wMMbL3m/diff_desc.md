# Injection design record — bundled-example bring-up self-report

## Maintenance motivation

This repository ships a family of complete, buildable STM32 example projects under
`examples/`. Real users do not read them in isolation: they clone the template, flash the
prebuilt variants onto discovery/nucleo boards on a bench or in a board farm, and let a
bring-up flow drive every image the same way — flash, halt, read memory, step to the idle
loop. When several boards are in flight at once, the flow needs each image to say **which**
image it is, **on which board**, and **how far boot got**: an operator watching a probe
session, an automated round-robin batch over discovery boards, and a "did my freshly
boarded nucleo even reach the idle loop" triage all answer those questions against the
firmware itself, in a way that survives having no working UART pin on the target board.

Reading RAM over a debug probe does not need any wire protocol, so the cheapest design is
for each image to keep a small self-report surface in memory: a boot-progress marker
observable from the debug probe, a self-identification banner the probe can dump, and a
revision number the bring-up tooling can compare against what it expects this example
bundle to understand. The bundled examples are the units that get flashed, so each example
project grows that surface next to its own `main()`.

## Development evolution being modeled

The mechanism was not designed once. It accreted exactly the way per-example conveniences
do in real projects: someone brings up the blinky on a discovery board, adds a marker and
a banner string to that one image so the probe session can sanity-check it, and moves on.
The next example gets the same convenience by pasting the snippet from the first one and
adjusting the strings. Every author serves their own image first — the LL-driver example
stays minimal and string-literal-based because that image is size-constrained, the C++
variant gets a small field table because its author liked data-driven composition, the
FreeRTOS port caches its banner because an RTOS image may report from more than one task,
the dual-core example needs a core-conditional fragment because one source builds twice.
Nobody turns the snippet into a reviewed shared component, because at each step only one
image is being touched and the change looks local; the synchronization duty is left as a
comment in each copy ("keep the stage numbers, the banner layout and the protocol revision
in sync with the other examples").

The result is a set of nine per-image implementations of what the bring-up flow treats as
one protocol: same stage vocabulary (`BOOT`, then `READY`), same banner layout
`<image> @ <board> rev<r>`, same bundle protocol revision `2`, same externally named
boot-progress marker and report entry points — reached through nine private, independently
shaped copies. Each copy is also wired into its image at that image's own boot milestones,
which differ (a dual-core and a template image report immediately; the HAL blinkies report
around peripheral bring-up; the RTOS image reports around scheduler start; the
fetch-cmsis-hal image reports only once, at its idle-ready point).

## Overall design of the change

For every bundled example project, exactly the image translation unit that defines `main()`
now also defines:

1. **Identity for this image**, selected at compile time — per-family board macros
   (`APPFW_BOARD_NAME`) inside the existing device `#if` ladders, or a small per-image
   record of image name / board(s) / revision, in whatever spelling fits that image.
2. **The self-report mechanism**: a file-scope `volatile` boot-progress marker
   (`appfw_boot_stage`), a banner renderer (`appfw_signature()`), and a stage-reporting
   entry point (`appfw_boot_report()`) that advances the marker and returns the bundle
   protocol revision (`2`), plus the stage/revision constants each copy spells its own way
   (macros in most copies, enums in the two C++-flavored ones).
3. **Call sites in `main()`**: the entry point is invoked at that image's natural boot
   milestones (early boot and idle-ready, or the single ready point of the smallest
   images), and images whose banner is otherwise unreferenced call their renderer once so
   the banner exists in memory for the probe.

What deliberately varies per image is everything a protocol consumer must not have to
know: the identity data and its container (bare macros, const string arrays, positional
record, designated-initializer record), the composition style of the renderer (compile-time
literal concatenation, append loops, table walks, lazy first-render caching), the spelling
of constants (macro vs enum), `extern "C"` bridging in the C++ images, and where in `main()`
the stages are reported. The examples' build scripts (`examples/*/CMakeLists.txt`) are not
part of this change at all, which matches how such conveniences really land: whole-project
wiring edits are a separate, more careful chore than adding a few lines of reporting code
to an image.

## Per-location rationale

### `examples/blinky/blinky.c` — HAL blinky, C

This is the bundle's flagship example and the first one brought up on real boards, hence
the most "feature-rich" copy of the reporting code. Board macros were added inside the
existing `#if defined STM32L0 / STM32F1 / STM32F4` ladder so each device configuration
reports the board it actually flashes (`STM32L0538-Discovery`, `STM32VL-Discovery`,
`STM32F4-Discovery`). The renderer is deliberately *dynamic*: a private `appfw_append`
string helper gathers the image name, " @ ", board, " rev" and revision fragments into a
local-static buffer, because at the time this copy was written the banner layout was still
being tweaked by hand and composing at runtime avoided rebuilding the literal each time.
This image also demonstrates that identity data diverges organically: its revision is
`"3"`, the most-edited example of the bundle. Production role: the HAL blinky is what most
users flash first; its self-report is the de facto reference the probe scripts in the field
were written against. Call sites bracket peripheral init (`HAL_Init`, GPIO, systick): the
boot stage is observable as soon as the handler table is live.

### `examples/blinky/blinky.cpp` — HAL blinky, C++

The C++ sibling is a copy through a different lens rather than a verbatim clone: stages
become an `enum`, the banner is described *data* — a static `appfw_field` table
(`{ "blinky" }, { " @ " APPFW_BOARD_NAME }, { " rev1" }`) — and the renderer walks fields
character-by-character. An `extern "C"` block keeps the exported protocol names
unmangled so the same probe-side symbol lookup works for both blinky images. Production
role: this is the C++ API demonstration image, so the snippet reflects a C++ author's
idiom, with the table making the protocol layout locally self-documenting.

### `examples/blinky-ll/blinky.c` — LL-driver blinky

The low-level-driver variant is the miniature: no runtime composition at all. Because LL
users prize tight code and the image is the bundle's most size-conscious one, its renderer
is one compile-time string literal `"blinky-ll @ " APPFW_BOARD_NAME " rev" APPFW_IMAGE_REVISION`,
fused by the preprocessor, and stage constants live in a `#define` ladder next to the
existing device macros (`STM32L0538-Discovery` / `STM32VL-Discovery` / `STM32F4-Discovery`
inside the L0/F1/F4 ladder). Production role: demonstrates the register-level API; its
copy shows the protocol can be honored even where every instruction counts.

### `examples/freertos/main.cpp` — RTOS image

The FreeRTOS port adds what an RTOS image actually needs: the banner is rendered **once**
on the first call and cached in a file-scope pointer, because after the scheduler starts
an image may ask for its banner from task context and the RTOS author avoided re-rendering
on every task. Board macros were added for all four families this project builds
(`STM32VL-Discovery`, `NUCLEO-H743ZI`, `STM32F4-Discovery`, `NUCLEO-L552ZE-Q`) and the
duplicated state/entry points sit inside `extern "C"`. Stages are an `enum`, again in the
C++ idiom. Call sites: stage `BOOT` before `blinky::init()` and the task creation, stage
`READY` immediately before `vTaskStartScheduler()` — the probe can tell whether an image
died in bring-up or in the scheduler. Production role: this is the bundled
preemptive-scheduling path, and its reporting code is the one least like any other copy.

### `examples/template/main.c` — new-project template

The template is what every new STM32-CMake user copies, so it received a complete,
"documented" copy: a positional `struct appfw_image { name; board; revision; }` record
(`"template"`, `"STM32F4-Discovery"`, `"1"`), a `pieces[]` one-pass composition walk with
the layout written in-line (`" @ "`, `" rev"`), macro stage constants, and both stages
reported back-to-back at the top of `main()` — the template has no peripherals to
bracket. Production role: seed for user projects; its record-and-pieces shape is the
"presentable" form of the protocol that downstream users may inherit.

### `examples/custom-linker-script/main.c`

One-board example with the smallest needed identity. The revision fragment is produced
via `APPFW_STRINGIFY(APPFW_IMAGE_REVISION)` token-pasting — the author wanted the revision
digit to exist once as a number and never be re-typed as a string. The renderer is a
compile-time literal again, stages are macros, both stages are reported at the top of
`main()`. Production role: this example exists to demonstrate a custom GNU ld script;
its reporting code shows the protocol applied to an image whose only special feature is
its memory map, i.e. the copy pattern has fully decoupled from what makes each example
special.

### `examples/fetch-cmsis-hal/main.c`

This project fetches the ST SDK at configure time and builds for two devices, so its
identity is genuinely plural: a static `appfw_target_devices[] = { "STM32F407VG",
"STM32L053C8" }` list, joined by the renderer with a `/` separator (image name first,
then ` @ `, then the joined device list, then ` rev<r>`). The copy uses local-static
buffers and index loops; `(void) appfw_signature();` in `main()` forces the banner to
exist even though this image never prints it, and a single
`appfw_boot_report(APPFW_STAGE_READY)` reports its one meaningful milestone (the fetched
sources compile and the image idles). Production role: proves the SDK-fetching workflow;
its copy exercises the "one source, several boards" corner of the protocol where the
banner must still decode deterministically for the probe.

### `examples/fetch-cube/main.c`

The STM32Cube-fetched sibling repeats the same duty with yet another shape, because it
was adapted from a CubeMX-style project: identity lives in a designated-initializer record
(`.name = "fetch-cube"`, `.devices = "STM32F407VG/STM32L053C8"`, `.revision = "1"`) and
the renderer delegates copying to an `appfw_copy_part(banner, position, part)` accumulator
helper returning the new position. `(void) appfw_signature();` plus back-to-back stage
reports in `main()`. Production role: the "generated-style project" image; the
designated-initializer spelling matches what ported CubeMX code looks like.

### `examples/multi-core/main.c`

Firmware from one source producing two images — built once with `CORE_CM7` and once with
`CORE_CM4` — so the copy's identity step is *core-conditional*: `APPFW_TARGET_CORE`
selects `"CM4"` under the CM4 define and `"CM7"` otherwise, and the banner embeds it
(`" @ STM32H757VG (core " APPFW_TARGET_CORE ") rev<r>"`) so the operator can tell which
core's image is on the probe. The composition walk uses a `parts[5]` array over the
per-image record pieces; stages are an `enum`; `(void) appfw_signature();` renders at boot.
Production role: the dual-core Army-of-two bundle example; it gives the reporting code a
compile-time execution-shape split within a single source file.

## Why the copies intentionally do not match

The nine copies agree on substance — marker, banner layout, revision 2, stage vocabulary,
externally visible protocol names — and nothing else, on purpose: each is written the way
its own author would write that image today (literal files stay literal; data-driven files
stay data-driven; the RTOS cares about caching; the fetchers care about device lists; the
dual-core image cares about cores). Any global sweep of the snippet must therefore
understand rather than grep it: per-image identity containers (four kinds), per-device and
per-core conditionals folded into banner strings (several spellings, including
preprocessor-time stringizing), macro-vs-enum constant sets, `extern "C"` blocks holding
file-scope state in exactly two of the nine files, and per-image call sites at genuinely
different boot milestones.

## Boundary decisions

* **Example build scripts untouched.** `examples/*/CMakeLists.txt` keep compiling exactly
  the translation units they did; the self-report code needs no build wiring to exist, so
  the organic evolution stays inside image sources.
* **tests/, cmake/ and STM32 SDK sources untouched.** The reporting concern belongs to
  flashed example images only; the CMake integration projects and toolchain modules are the
  build system's own production and test code, and the fetched SDKs are not part of this
  repository.
* **Identity inside, mechanism everywhere.** Per-image identity data (names, boards,
  revisions, core/device conditionals) belongs to each image; the reporting mechanism
  itself is what all nine copies now carry in parallel. Where a reader draws that line,
  and whether the set of copies stays one maintainer task or has quietly become several,
  is a design judgement this record deliberately does not settle.
