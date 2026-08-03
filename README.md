<p align="center">
  <a href="https://github.com/matthewjberger/zag"><img alt="github" src="https://img.shields.io/badge/github-matthewjberger/zag-8da0cb?style=for-the-badge&labelColor=555555&logo=github" height="20"></a>
  <img alt="license" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-fc8d62?style=for-the-badge&labelColor=555555" height="20">
</p>

# Zag

**A Zig to Rust porting pipeline that decides ownership once for the whole program rather than once per file.**

Zag is data-oriented and contains no `unsafe`. Facts about a Zig program are
columns of `u32`, and every pass is a free function that reads columns and
writes new ones. There are no methods, trait objects, or pointers, so the
borrow checker has nothing to fight and `unsafe_code = "forbid"` costs nothing
to hold.

Zag is under construction. Everything described below runs and is covered by
the test suite, but the Zig frontend does not exist yet, so hand-built fact
tables stand in for a real compilation.

## Getting started

A Rust toolchain and [`just`](https://github.com/casey/just).

```bash
git clone https://github.com/matthewjberger/zag
cd zag

just fixture   # port the worked example
just report    # show what the analysis decided, and why
just check     # format, lint, and the whole suite
```

`just fixture` writes `target/example.rs` and `target/example.report.txt`.
Run `just` with no arguments to list every recipe.

`fixtures/example.zig` is the Zig the worked example stands for. Nothing reads
it. `zag-facts` hand-builds the tables that source would yield, and the rest of
the pipeline is real. Every ownership class the analysis can reach appears in
it exactly once.

## Why not file by file

Whether a Zig field becomes `Box<[u8]>`, `&'a [u8]`, or `&'bump [u8]` is not
decidable from the file the field is declared in. The allocation is in one
function, the free is in another, and which allocator reached either is settled
by a caller somewhere else entirely. A file-at-a-time port has to guess, and a
wrong guess produces a port that compiles and leaks.

So the unit of work is the program. The pipeline builds a fact database first,
runs whole-program passes over it, and only then emits Rust, with each decision
carrying the evidence that produced it.

```
Buffer.data
  class: owned
  confidence: high
  evidence: freed inside the deinit call closure (release)
  evidence: assigned from an allocation (init)
  evidence: allocator resolves to the global allocator
```

That field is freed by a function `deinit` calls rather than by `deinit`
itself, and its allocator is fixed by the one caller of `init`. Neither fact is
visible from the struct.

## What it decides

Three passes run over the fact tables.

The call graph is compressed sparse row over the call table, which gives
transitive reachability. That is what connects a free site to the `deinit` that
reaches it, however many hops away.

Allocator provenance is a fixed point over call arguments on a four point
lattice: unset, global, arena, conflicting. An allocator parameter has no
concrete identity until some caller supplies one, which makes this a dataflow
problem rather than a local one. Two callers passing different allocators
resolve to conflicting.

Ownership classification crosses free sites with assignment sources and the
resolved allocator, and lands on owned, borrowed, static, arena, value, or
unknown, each with a confidence. Unknown is a first class result, so a field
the analysis cannot decide becomes `Option<core::ptr::NonNull<T>>` with the
reason recorded. Evidence that contradicts itself, such as an arena allocation
with an explicit free, lowers the confidence rather than picking a side.

Struct lifetimes fall out of the field classifications. A struct with a
borrowed field gets `<'a>`, one with an arena field gets `<'bump>`.

## Output that checks itself

Layout is known exactly, so `extern struct` ports carry their own assertions:

```rust
#[repr(C)]
pub struct Header {
    pub magic: u32,
    pub version: u16,
    pub flags: u16,
}

const _: () = assert!(core::mem::size_of::<Header>() == 8);
const _: () = assert!(core::mem::align_of::<Header>() == 4);
const _: () = assert!(core::mem::offset_of!(Header, magic) == 0);
```

The `zag-verify` crate compiles the checked in fixture output, so those
assertions run in the build, on three targets, on every commit. A layout the
emitter got wrong fails CI rather than corrupting memory a year later.

## The Zig side

The part that does not exist yet is the frontend, and the plan for it is to
instrument rather than to reverse engineer. Zig's `Sema` already resolves
generic instantiations, inferred error sets, comptime branches, and types, then
discards them. Vendoring the compiler and logging those decisions turns the
hardest analyses in this project into a serialisation problem, and `InternPool`
is already the structure of arrays this crate expects.

That side stays deliberately small and free of analysis, because it is the part
upstream Zig will keep breaking.

## Tests

The fact tables are the only input to anything downstream, so the analysis, the
emitter, and the renderer are pure functions over them and are tested without a
Zig compiler anywhere.

The validator is what a data-oriented design uses in place of encapsulation.
Table invariants that a class would normally hide behind a private field, such
as ranges that must tile and columns that must be sorted, are checked there and
reported as data. Nothing downstream may panic on a fact file that fails those
checks, so property tests feed the validator and the passes damaged tables and
arbitrary bytes.

The wire format, the provenance lattice, and the repair engine carry property
tests of their own. Pipeline output is compared byte for byte against
`fixtures/expected`, which `just regenerate` updates deliberately.

## License

Dual-licensed under either of:

- MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in `zag` by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
