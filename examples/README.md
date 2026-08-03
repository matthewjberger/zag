# Examples

Each directory is a Zig program that builds and runs on its own, and each one
exists to drive a different answer out of the analysis. Together they are the
integration layer: the unit tests cover each pass in isolation, and these check
that a whole program goes in one end and a port comes out the other.

## Running the Zig

Zig 0.15.2 or newer. From any example directory:

```bash
cd examples/wordcount
zig build run
```

`zig build` alone puts the binary in `zig-out/bin`.

## Porting one

The Zig frontend does not exist yet, so the fact tables each program would
produce are built by hand in `crates/zag-facts/src/examples/`, and the rest of
the pipeline is real.

Those tables are not taken on trust. `tools/reflect` imports an example and
asks the compiler what it resolved, and the tests compare the answer against
the tables: every struct, its layout, every field in order with its offset,
and every public function with its parameter count. A table that drifts from
the program it claims to describe fails there. Run it by hand with
`just reflect netpacket`.

What reflection cannot reach is the dataflow, which call allocates what and
which function frees it. That part is still hand supplied, and it is exactly
the part that needs the compiler's semantic analysis rather than its type
information.

```bash
zag examples                                          # what is available
zag facts --example wordcount --output wordcount.facts
zag emit --facts wordcount.facts --source port.rs --report port.report.txt
```

`just examples` runs that for every example and rewrites the `expected`
directories. `cargo test` compares against them, so a change to the emitter
shows up as a diff you have to look at.

Read the report before the port. [../docs/PORTING.md](../docs/PORTING.md) is
what each line means and what to do about it.

## What each one is for

`wordcount` is the flagship case for owned memory. `Counts.text` is allocated
from the global allocator and freed by `release`, which `deinit` calls rather
than doing the work itself. Deciding that the field is owned therefore needs
the call graph, and no rule that reads one function at a time gets it right.
The port is `Box<[u8]>` and the Zig `deinit` disappears into `Drop`.

`tokenizer` splits three lifetimes apart in one struct. `Document.source`
belongs to the caller and comes back as `&'a [u8]`, `Document.tokens` and
`Token.text` come from an arena and come back as `&'bump`, and
`Document.separators` is a literal and comes back as `&'static`. It is also the
example that makes a struct name carry the lifetimes it declares, since
`tokens` is a slice of `Token` and `Token` has a `'bump` of its own.

`netpacket` is the layout case. `Header` is an `extern struct` whose size,
alignment, and field offsets the port has to preserve, so the emitted Rust
carries assertions for all of them and `zag-verify` compiles it. The Zig
asserts the same numbers at comptime, so a layout that moves fails on both
sides rather than one. Its `payload` is owned and freed directly in `deinit`,
which is the case the call graph does not have to work for.

`conflict` is the one that produces a finding rather than a port. `makeCache`
takes an allocator, one caller hands it the heap and another hands it an arena,
so the field has no single owner and the report says
`allocator does not resolve to one allocator`. The emitted type is
`Option<core::ptr::NonNull<[u8]>>`, which compiles and owns nothing, so the
port cannot be finished until a person decides. That is the intended outcome.

## Adding one

Add the directory with `build.zig`, `build.zig.zon`, and `src/main.zig`, and
check it runs. Add a module under `crates/zag-facts/src/examples/` with the
tables that program would produce, and register it in `examples.rs`. Run
`just reflect <name>` and make the tables match what the compiler says, then
`just examples` and read what comes out. The tests fail while a directory has
no tables, tables have no directory, or the two disagree about a declaration.

Zig lays an `auto` struct out in whatever order it likes, so field offsets are
worth reading off `just reflect` rather than assuming. `netpacket` puts its
header second in memory and first in declaration order.
