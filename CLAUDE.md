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

Changing the emitter changes `fixtures/expected`. Run `just regenerate` and
read the diff. That diff is the review.
