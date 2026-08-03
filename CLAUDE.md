# Zag

A whole-program Zig to Rust porting pipeline. Read `README.md` first for what
the passes decide and why the unit of work is the program rather than the file.

## Invariants

These are load-bearing. Breaking one is a design change, not a refactor.

- No unsafe. `unsafe_code = "forbid"` is set in the workspace lints and every
  crate inherits it. `crates/zag/tests/workspace.rs` fails if a crate stops
  inheriting, or if the keyword appears anywhere in the tree. Handles are `u32`
  indices, so nothing in the fact database is a pointer.
- Data-oriented, no object orientation. Tables are structure of arrays. Handles
  and rows carry no methods. Everything that computes is a free function taking
  `&Tables` and returning fresh columns. Marker derives (`Clone`, `Debug`,
  `PartialEq`) and `From` for `?` are fine. Nothing that computes may hang off
  a data type.
- Determinism. No hash map iteration in any output path, no timestamps in the
  wire format, columns written in a fixed canonical order. The fixture
  comparison in `crates/zag/tests/fixture.rs` is the guard.
- Honest analysis. `Unknown` and `Conflicting` are results the passes are
  expected to produce. A pass that cannot decide something must say so and
  record the evidence. Never widen a rule to make a fixture pass.
- Nothing panics on a corrupt fact file. Accessors are total, the validator
  uses checked arithmetic, the passes index sibling columns through `get`
  rather than assuming the validator ran, and property tests feed both damaged
  tables and arbitrary bytes. A row the passes cannot make sense of is dropped
  and the validator reports why.

## Layout

`zag-facts` owns the tables, the wire format, the validator, and the worked
example. `zag-analysis` is the three passes. `zag-render` owns the flat Rust
syntax tree and prints it. `zag-emit` lowers facts plus analysis into that tree
and into the review report. `zag-repair` turns compiler diagnostics into span
edits. `zag` is the driver. `zag-verify` exists only to compile the checked in
fixture output so its layout assertions run in the build.

Dependencies point one way. `zag-facts` and `zag-render` are leaves.

`examples/` holds Zig programs that build on their own, and
`zag-facts/src/examples/` holds the tables each one would produce. They are the
integration layer over unit tests that cover each pass alone. `zag-verify`
compiles every port they produce, so the layout assertions run in the build.

`tools/reflect` asks the compiler what an example declares and
`crates/zag/tests/reflection.rs` holds the tables to it: structs, layout, field
order, offsets, and public function arity. It is analysed, never linked, so it
works wherever zig can compile. Never hand-write an offset. Read it off
`just reflect <name>`, because Zig reorders an `auto` layout.

`tools/extract` parses an example and `crates/zag/tests/extraction.rs` holds
the tables to the dataflow: functions including private ones, parameter names,
call edges, memory operations, and field assignments. It reads syntax, so it
recognises `x.dupe(...)` by spelling rather than by resolving `x`. A table
naming the wrong allocator still passes, and closing that is the frontend's
job. It has to link, so it skips itself where zig cannot, and CI runs it.

## Working here

The validator replaces encapsulation. Any new table invariant (a range that
must tile, a column that must be sorted, a handle that must resolve) belongs in
`zag-facts/src/validate.rs` with a `Violation` variant and a test. It runs
before any pass touches the tables, so a missing check there shows up
downstream as a panic or as silently wrong output.

Adding a fact column means touching `tables.rs`, `wire.rs` (both directions, in
canonical order), and `validate.rs`. The round trip property test in
`zag-facts/tests/wire.rs` catches a column added to one direction only.

Nothing in a pass may scan a whole table per row. Cross-table lookups go
through an index built once. `build_call_graph` and `build_field_index` are
both counting sorts into compressed sparse row, and new ones should follow that
shape. The same rule is why the emitter uses `push_string` rather than
`intern`, which is quadratic and exists only for building small tables by hand.

Changing the emitter changes `fixtures/expected` and every `expected` directory
under `examples/`. Run `just regenerate` and read the diff. That diff is the
review.

A new example needs a directory that `zig build run` succeeds in, a module
under `zag-facts/src/examples/`, a line in `examples.rs`, and a paragraph in
`examples/README.md`. Tests fail while any of the four is missing.
