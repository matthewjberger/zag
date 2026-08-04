# Corpus

Whole Zig projects, written as ordinary programs rather than as cases for the
analysis, read through their build scripts and ported end to end.

`examples/` is the other half of this. Each example is small enough to carry
hand-built fact tables, which is what holds the frontend to what the compiler
actually said. Nothing here is hand-built, so nothing here checks the frontend
against the compiler. What it checks is what the whole pipeline does with code
nobody bent to fit it.

The distinction matters because the examples are written by the person writing
the analysis, and they come out the shape the analysis expects. The most
ordinary construction in Zig, a `deinit` freeing two fields, was absent from
every example until a bug in reading it was found by accident.

```bash
cd corpus/lifegrid && zig build run   # each is a standalone project
just corpus                           # re-port them all and rewrite expected/
just corpus-zig                       # build and run all of them
```

`cargo test` compares against every `expected` directory, so a change to any
pass arrives as a diff. `just corpus` is how a deliberate one is landed.

## The projects

`config` parses an INI file across three modules. One module owns the copy of
the source text, another does the parsing, and the entries point into that copy
rather than owning anything themselves. It covers a `deinit` freeing two
fields, an error set shared between modules, `std.ArrayList` reaching a field,
and interior pointers, which are the thing the analysis has no way to see.

`machine` is a stack bytecode interpreter that allocates nothing. It covers an
enum with an explicit tag type, a tagged union, a fixed array field, a named
error set, a `switch` over every variant, and methods that take the receiver
both ways.

`lifegrid` runs Conway's life over two allocated buffers that trade places
every generation. It covers two owned fields freed by one `deinit`, an
allocation with no source to copy from, signed and unsigned index arithmetic
through `@intCast`, and nested `while` loops.

## What they currently surface

These are ordinary programs, so they reach shapes the pipeline gets wrong. The
output is checked in as it is rather than as it should be, because a defect
recorded in a file somebody reads is worth more than one nobody has written
down.

- A fixed array whose length is a named constant loses its length and becomes
  the element type. `stack: [stack_size]opcode.Value` ports as
  `super::opcode::Value`. The length is parsed as a literal, so an identifier
  falls through to the element.
- `null` in a function body ports as `null` rather than `None`, and a value
  returned into an optional is not wrapped in `Some`. The constructor path
  handles both and the body path does not.
- An error set named across a module boundary is spelled unqualified, so
  `Fault` should be `super::opcode::Fault`.
- A `*T` parameter that is not the receiver comes across as `&T`. The receiver
  itself becomes `&mut self` when the Zig said `*T`, so the same pointer is
  read two ways depending on where it sits in the signature.

None of these are visible from `examples/`, which is the argument for this
directory.
