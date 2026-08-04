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

## What they surface

Cargo builds every port here, as the one file `zag emit` writes and as the
crate `zag build` lays out, and `zag-verify` builds the checked in ones again
on every build of this workspace. That is the claim the suite makes and the
reason the refusals below exist: a shape the passes cannot spell correctly is
left as `todo!()` rather than written wrong, so what comes out is a skeleton
somebody can start from rather than a file they have to fix before it builds.

These programs are what found the following, none of it visible from
`examples/`.

- A field's type came from the parser reading the source text, so
  `[stack_size]opcode.Value` lost its length and became the element type.
  Reflection had the compiler's answer, `[32]opcode.Value`, and was only being
  read for offsets. It now supplies the type as well.
- An error set named across a module boundary was spelled unqualified, so
  `Fault` compiled only in the file that declared it.
- A function returning a slice returned the slice body, which has no size.
  It now returns a reference, with the borrow inside the option where the
  return is optional, and a function with no reference argument to elide from
  is refused instead.
- Zig indexes with any integer and Rust indexes with `usize`, so the cast is
  now part of the translation.
- A `*T` parameter that was not the receiver came across as `&T` while the
  receiver became `&mut self`, so the same pointer was read two ways depending
  on where it sat in the signature. It takes `&mut` now, which leaves a body
  somebody can actually write.

Two shapes are refused rather than ported, because a body expression carries no
resolved type and both need one. `null` is `None` only once something says what
it is null of, and everything on the way out of the function would have to be
wrapped to match. `.len` is a length on a slice and a field access on anything
else, and Rust spells the first as a call. Both are cases the report names, and
both are the kind of gap that closes when the frontend can ask the compiler
what a body expression is rather than reading its spelling.
