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

`zag read --zig <file>` takes the root file of a program, follows `@import`
from it, and builds fact tables from every file it reaches, which is what
`just read` runs. Each example also carries tables built by hand in
`crates/zag-facts/src/examples/`, from before the frontend existed. The tests
require both routes to produce the same port, so the hand-built tables are the
oracle the frontend is held to.

Two tools do the reading, and the tests hold the hand-built tables to what they
find as well.

`crates/zag/tools/reflect` imports an example and asks the compiler what it resolved:
every struct, its layout, every field in order with its offset, and every
public function with its parameter count. It is analysed and never linked, so
the report arrives through `@compileError` and no platform's libc is involved.
Run it with `just reflect netpacket`.

`crates/zag/tools/extract` parses an example with the compiler's own parser and reports
the dataflow: which function calls which, which call allocates or frees, and
what each struct literal puts in each field. It sees private declarations,
which reflection cannot, so the function check runs both ways. Run it with
`just extract conflict`.

Between them, every function, parameter, call, memory operation, and field
assignment in the tables is checked against the program. What is left is the
part that needs types to decide: `crates/zag/tools/extract` recognises `x.dupe(...)` as an
allocation because of how it is spelled, not because it resolved `x` to an
allocator, so a table naming the wrong allocator still passes. Resolving that
is what the Sema frontend is for.

```bash
just names            # what can be ported
just port wordcount   # port it into target/ and print the Rust and the report
just dump wordcount   # print its fact tables as text
```

The two steps underneath, for anything that needs them separately:

```bash
zag facts --example wordcount --output wordcount.facts
zag emit --facts wordcount.facts --source port.rs --report port.report.txt
```

`just examples` runs that for every example and rewrites the `expected`
directories, which is the only recipe that touches them. `cargo test` compares
against those files and then hands each freshly generated port to `rustc` with
constants evaluated, so a change to the emitter shows up either as a diff you
have to look at or as a port that stopped building.

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

`shapes` covers the containers that are not structs. A Zig `enum` and a Zig
`error` set both port to a Rust enum whose variants carry nothing, a
`union(enum)` ports to one whose variants carry a payload, and a `void` payload
carries nothing so the port carries nothing either. Variants come back in
Pascal case, because a port that keeps the Zig spelling is a port the compiler
complains about.

`telemetry` covers the two types that are not one thing. `Frame.channels` is a
`[4]u32` and stays a fixed array rather than becoming a slice, so its length
survives the port. `Frame.label` is a `?[]const u8` set to `null`, and the
ownership wrapper goes inside the option rather than around it, which is why it
comes back as `Option<&'static [u8]>` and not an option of a reference to an
option. A field the Zig only ever sets to null is still a field the constructor
has to fill, so `null` is one of the expressions the port can write.

`ledger` is four files rather than one, and it is the case no file at a time
approach can get right. `Entry` is declared in `entry.zig`, its `label` is
allocated in `store.zig`, and that same field is freed in `close.zig`. No file
on its own says who owns the label, and reading them together is what makes it
`Box<[u8]>`. Each file becomes a Rust module, because a Zig file is a struct
and that is the same shape, and a type named across a module boundary is
spelled with the path to the module that declares it. `store.total` is also the
example of a body that comes across whole rather than as a `todo!()`.

`conflict` is the one that produces a finding rather than a port. `makeCache`
takes an allocator, one caller hands it the heap and another hands it an arena,
so the field has no single owner and the report says
`allocator does not resolve to one allocator` and then names both callers under
`allocator conflicts:`. The emitted type is
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
