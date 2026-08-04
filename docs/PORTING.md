# Porting Zig to Rust

This is how to read what zag decided, and the rules for the parts it does not
decide.

zag settles the shape of a type. Which Rust type each field becomes, which
lifetimes a struct carries, and what layout the port has to preserve. It does
not write function bodies, so the second half of this document is the rules for
those.

Nothing here is advice. Each rule is a decision procedure with one answer.

## The procedure

1. Run the pipeline over the program. `just read <file>` does this for any Zig
   file, and `just port <name>` for the examples `just names` lists. Both write
   into `target/` and print the Rust and the report.
2. Read the report before reading the generated Rust. The report says what was
   decided for every field and on what evidence.
3. Settle every field marked `unknown`, and every field marked `low` or
   `medium`. Those are the fields where a port goes wrong silently.
4. Take the generated struct definitions as given and write the bodies against
   them. Changing a generated type means the evidence was wrong, so fix the
   fact that produced it rather than the output.
5. Keep the layout assertions. A wrong layout then fails the build instead of
   the program.

## Worked examples

`examples/` holds Zig programs that build and run on their own, one per outcome
this document describes. Each carries the port and the report it produces, so
every rule below can be read against a case that actually runs. `just port
tokenizer` ports one and prints what came out.

| example | what it shows |
|---|---|
| `wordcount` | `owned`, where two fields are freed a call away from `deinit` |
| `tokenizer` | `borrowed`, `arena`, and `static` separated inside one struct |
| `netpacket` | an `extern struct` and the layout assertions its port carries |
| `conflict` | an allocator two callers disagree about, which ports to nothing usable |

## Reading the report

Every field carries a class, a confidence, and the evidence behind both.

```
Buffer.data
  class: owned
  confidence: high
  evidence: freed inside the deinit call closure (release)
  evidence: assigned from an allocation (init)
  evidence: allocator resolves to the global allocator
```

### Classes

| class | what zag found | emitted Rust | what you check |
|---|---|---|---|
| `value` | the field is not a pointer or a slice | the scalar type | nothing |
| `owned` | allocated from the global allocator, freed through the owner's `deinit`, and never resized | `Box<[T]>` or `Box<T>` | that `Drop` now does what the Zig free did |
| `grown` | the same, and something reallocates it, so its length is not fixed | `Vec<T>` | that nothing relied on the address staying put |
| `borrowed` | assigned from a parameter the caller keeps, never freed | `&'a [T]`, and the struct gains `<'a>` | that the caller really outlives the struct |
| `static` | assigned only from literals, never freed | `&'static [T]` | nothing |
| `arena` | allocated from an arena | `&'bump [T]`, and the struct gains `<'bump>` | that the arena outlives the struct |
| `unknown` | the evidence is missing or disagrees | `Option<core::ptr::NonNull<[T]>>` | all of it, this is the flag |

An `unknown` field compiles and carries no ownership. Leaving one in a finished
port means the port has a pointer nobody owns.

`zag build` writes `unsafe_code = "deny"` into the manifest rather than
`forbid`, because reading one of those pointers needs an `unsafe` block and
`forbid` cannot be relaxed by an attribute. Settling the field is the answer,
and `#[allow(unsafe_code)]` on the one item that needs it is the escape while
you are getting there. The lint level of the ported program is yours to set.

### Confidence

| confidence | what to do |
|---|---|
| `high` | take the class as given |
| `medium` | read the evidence lines, one fact is missing or two disagree |
| `low` | decide by hand, zag had nothing usable |

### Evidence

| line | what it tells you |
|---|---|
| `freed inside the deinit call closure` | the free is reachable from the owner's `deinit`, however many calls away |
| `freed outside the deinit call closure` | something frees this field, and it is not the owner. Find out who owns it before porting |
| `assigned from an allocation` | the field points at memory this program allocated |
| `assigned from a parameter the caller retains` | the caller owns the memory |
| `assigned from a static literal` | the memory outlives everything |
| `assigned from an unrecognised source` | zag could not classify the assignment. Read the Zig |
| `allocator resolves to the global allocator` | ordinary heap, `Box` is correct |
| `allocator resolves to an arena` | the arena owns it, `Box` would double free |
| `allocator does not resolve to one allocator` | two callers disagree. One of them is a bug, or the function needs splitting |
| `no assignment to this field was found` | nothing in the program writes this field |
| `resized after allocation, so its length is not fixed` | a `realloc` reaches this field, which is why it is a `Vec` rather than a `Box<[T]>` |
| `the Zig asks for an alignment no port of this field can carry` | the field is `[]align(N) T` or `*align(N) T`. Decide what carries the alignment before anything else, because no ownership class can |

A `warning:` line above the fields means allocator provenance did not settle.
Every allocator below it is understated, so treat the whole report as
provisional until that is fixed.

### Allocator conflicts

`allocator does not resolve to one allocator` says a field has no single owner.
The `allocator conflicts:` section at the end says which callers disagreed:

```
allocator conflicts: 1
  makeCache takes allocator from callers that disagree
    from fromHeap: the global allocator
    from fromArena: an arena
```

Those two callers are the whole finding. Either split the function so each
allocator has its own, or hand the ownership decision to the caller and take a
`&mut` rather than allocating inside. There is no port of the original that is
right for both.

### Functions

The report ends with what became of every function.

| outcome | what to do |
|---|---|
| `ported, as the constructor` | nothing, the body is written |
| `ported, signature and body` | read it, then nothing. The whole function came across |
| `disappears, Drop frees what it freed` | delete the Zig `deinit`, `Box` already does its job |
| `ported, signature only, the body is still to write` | fill in the `todo!()`, the signature around it is settled |
| `still to write, what it returns did not resolve` | find the type the frontend could not read, usually a generic or a comptime result |
| `still to write, the error set it can fail with has no name` | give the Zig a named error set, or decide the Rust error type yourself |
| `still to write, what it returns borrows from an arena the port drops` | decide who owns the result, since the arena does not survive the port |
| `still to write, what it returns borrows and no parameter can carry the lifetime` | decide what the result borrows from and put it in the signature |
| `still to write, what it returns borrows and nothing was passed in to borrow from` | it returns a slice and takes no reference of its own. Return an owned value, or take the buffer it reads from as an argument |

Where an `init` produced no constructor, the line under it says which field
stopped it.

| line | what to do |
|---|---|
| `no constructor: nothing the port could read assigns <field>` | the Zig may well assign it, from a loop or a branch the frontend cannot follow. Write that part of the body yourself |
| `no constructor: <field> is set from <zig>, which the port cannot spell` | port that expression by hand. The Zig it could not read is quoted so you do not have to go looking |
| `no constructor: who owns <field> was not decided` | fix the ownership finding above first, because the constructor cannot say what it is handing over until you have |

A `deinit` only disappears when every field it frees is one the analysis proved
owned. A `deinit` that closes a handle, decrements a counter, or frees
something it does not own keeps its own outcome, because `Drop` does not cover
it. A helper the `deinit` calls disappears on the same grounds, since freeing
fields `Drop` now frees leaves it with nothing to do.

A signature is worth having on its own. It carries which parameters the port
borrows and which it takes by value, what the function returns, and which error
set it can fail with, and those are settled before anybody writes the body.
The allocator parameter is not among them, because the port allocates through
its own types.

Two things stop a signature being written, and both are the port refusing to
invent something. A `!T` infers its error set from the body, so the Zig named
no error type and neither can the port. A return type that carries a lifetime
needs that lifetime tied to a parameter, and an arena lifetime has no parameter
to tie to once the allocator is gone.

### Where it was written

Every function outcome ends with the file and line the Zig declares it on, and
so does the expression a constructor could not spell:

```
  Counts.init: still to write, the error set it can fail with has no name (main.zig:9)
    no constructor: words is set from words, which the port cannot spell (main.zig:24)
```

A location appears only when the tables recorded both a file and a line, since
a line on its own sends a reader nowhere. Layout facts carry none: the compiler
resolves a struct's size through reflection, which has no source location to
give, and nothing in the report complains about layout.

### Taking the port away with you

`zag emit` writes the whole program as one file, which is the form the checked
in output compares against. `zag build --into <directory>` writes the same
program as a crate: a manifest, a `src/lib.rs` declaring each module, and a
file per module. That is the form somebody keeps, and `just build-crate <name>`
runs it over an example and compiles what comes out.

A Zig package the crawl could not read is a crate the port needs and does not
have. Where a program names one, the port is a workspace: the program's own
crate, a crate standing in for each package, and the dependency already wired
between them. Filling those in, or pointing Cargo at the real ones, is work the
report named and the layout has now made a place for.

A missing `.zig` file is not a missing package and adds no crate.

### When the report is not enough

`zag dump` prints the fact tables as text, one row per line, in the order the
tables hold them. Reach for it when the report says something surprising and
the question becomes what the tables actually say rather than what the port
made of them. `just dump <example>` runs it over an example.

## Modules

`zag read` takes the root file of a program and follows `@import` from there,
so the tables cover every file rather than the one it was pointed at. That is
not a convenience: which allocator reached a field is decided by a caller that
may be three files away, and reading one file at a time cannot see it.

Each Zig file becomes a Rust module. A Zig file is a struct and `@import` gives
you that struct, so `store.Entry` is already a path and `store::Entry` is the
same path in Rust. The root file keeps the top level, because in Zig the root
file is the program's own namespace. A program that is one file therefore has
no module tree, which is not a special case: it has one namespace and so does
the port.

A type named from another module is spelled with the path to the module that
declares it, relative rather than rooted at the crate, so a port stays correct
when it is included somewhere else.

| report line | what to do |
|---|---|
| `modules: N` followed by a path per file | nothing, this is what was read |
| `unresolved import: <text>` | find what it names and point zag at it, because the analysis never saw those declarations |

## Artifacts

Pointed at a project directory or a `build.zig` rather than at one file, `zag
read` reads the build graph and starts from every file an artifact is rooted
at. That is the difference between porting the program you happened to name and
porting the repository, and it matters for the same reason modules do: two
executables that share a module share whatever that module owns.

There is no root file then, so nothing sits at the top level and every file is
a named module. Each executable becomes a binary under `src/bin`, named what
the build script named it, calling the `main` its root module ported. All of
them are one Cargo package, because they share every module they import and a
workspace would mean copying those modules into each one.

| report line | what to do |
|---|---|
| `executable <name>: <path>` | nothing, this is what the build script asked for |
| `library <name>: <path>` | nothing, though the port makes one library of everything rather than one per artifact |
| `test <name>: <path>` | port it as a test rather than a binary, which is a judgement the layout does not make for you |
| `<kind> <name>: the build script names a root source file the crawl could not open` | the script computes its root path rather than writing it, so find what it resolves to and read that file directly |

A build script that builds its artifact list in a loop, or names its root
source files through a variable, says nothing this can read by spelling. It
gets no artifacts rather than wrong ones, and `zag read` on the root file is
the way through.

An unresolved import is an import that named neither a file the crawl could
open nor a module the project declares. Every ownership decision below it in
the report was made without whatever that import brought in, so treat them the
same way you would treat a `warning:` line.

## Types

The type a field gets is its Zig type crossed with its ownership class.

| Zig | class | Rust |
|---|---|---|
| `[]const u8` | `owned` | `Box<[u8]>` |
| `[]const u8` | `borrowed` | `&'a [u8]` |
| `[]const u8` | `static` | `&'static [u8]` |
| `[]const u8` | `arena` | `&'bump [u8]` |
| `[]const u8` | `unknown` | `Option<core::ptr::NonNull<[u8]>>` |
| `[]u8` | any | as `[]const u8`. Zig's mutability is not carried into the field type |
| `*T`, `*const T` | `owned` | `Box<T>` |
| `*T`, `*const T` | `borrowed` | `&'a T`, never `&'a mut T` |
| `?*T` | `owned` | `Option<Box<T>>`, which is the same size as the Zig |
| `?[]const u8` | `static` | `Option<&'static [u8]>` |
| `?[]const u8` | `owned` | `Option<Box<[u8]>>` |
| `[4]u32` | `value` | `[u32; 4]` |
| `u8`, `u16`, `u32`, `u64` | `value` | `u8`, `u16`, `u32`, `u64` |
| `f32`, `f64` | `value` | `f32`, `f64` |
| `bool` | `value` | `bool` |
| `void` | `value` | `()` |
| an enum or union field | `value` | the ported enum |

A borrowed pointer never becomes `&mut`, whatever the Zig wrote. Zig's `*T` is
a mutable pointer and its `*const T` is the shared one, so the obvious mapping
is `&'a mut T`. It is the wrong one. Rust's `&mut` is `noalias` and Zig's `*T`
is not, so a field that this analysis calls borrowed may still be one of
several live pointers to the same object, and `&mut` would make that undefined
behaviour that compiles and passes its tests. The analysis answers who frees a
thing, not how many pointers reach it, so it cannot tell the two apart and
takes the shared reference every time.

That is a deliberate under-approximation and it will not compile where the Zig
did write through the pointer. Widening a field to `&mut` by hand is only safe
once you have established that nothing else holds the same address, and that is
a fact about the whole program which nothing here has checked.

A parameter is the other way round. `*T` there becomes `&mut T` and `*const T`
becomes `&T`, because the Zig said which one it meant and the port has no
reason to say otherwise. A field is a place the analysis had to guess about; a
parameter is a place the author wrote the answer down. The same aliasing
question applies, and the caller is the one who has to answer it, which is where
Rust would put it anyway.

Two shapes have no row because they carry no length: `[*]T` and `[*:0]u8`. A
many-item pointer is a raw pointer plus a length you have to find, and a
sentinel pointer is one whose length is a scan away. Both need a person.

A field with an alignment the Zig asked for, `[]align(16) T` or `*align(16) T`,
gets no class at all. `Box<[T]>` carries the alignment of `T` and nothing more,
and Rust puts alignment on the type rather than on the allocation, so every
class would write something that quietly relaxes the request, and a port that
does that is one alignment fault away from a crash on a platform that cares.
The report says which field and why, and the answer is a wrapper type carrying
`#[repr(align(N))]` rather than a different ownership class.

`allocator.alignedAlloc` is a separate reason for the same outcome: it is not
one of the spellings the frontend reads as an allocation, so the field it fills
has no allocation evidence and lands on `unknown` too.

`owned` becomes `Box<[T]>`, which is right only for a field whose length never
changes after it is assigned. A field a `realloc` reaches is `grown` instead
and becomes `Vec<T>`, on the evidence line that says so.

That is read by spelling, so it finds `allocator.realloc(self.data, n)` and
does not find a `std.ArrayList` stored in the field, or a resize that goes
through a helper taking the field as a parameter. Check an `owned` field for
both before you write against it: the boxed slice will compile and then refuse
to grow, and changing it afterwards is the one edit step 4 tells you not to
make blind.

`toOwnedSlice` shrinks to fit and `Vec` keeps its capacity, so swapping one for
the other changes the memory profile of anything hot.

An optional keeps its ownership wrapper inside itself. `?[]const u8` that is
owned is an optional box, not a box of an optional, and the class is decided by
what the option holds rather than by the option. An array keeps its length,
because a `[4]u32` that ports to a slice has lost something the Zig knew.

Zig has an integer of every width and Rust has five. A `u3` field widens to
`u8`, a `u12` to `u16`, and so on. Widening is safe for storage and changes
behaviour in one place: Zig traps when a `u3` overflows and the ported `u8`
does not. Check every widened field for arithmetic that relied on the trap.

Text is bytes. Paths, source, module names, environment variables, and anything
that came from a syscall or a socket stay `&[u8]` and `Vec<u8>`. Reach for
`String` only for literals you wrote yourself. Adding UTF-8 validation to a
port both costs throughput and rejects inputs the Zig accepted.

## Allocators

Provenance resolves each allocator parameter to one of four values.

| resolved | what it means for the port |
|---|---|
| `global` | delete the allocator parameter, `Box` and `Vec` use the global allocator |
| `arena` | keep the arena, thread it as `&'bump Bump`, and give the type a `'bump` |
| `conflicting` | two callers pass different allocators. Split the function or fix the caller before porting |
| `unset` | no caller supplies one. The function is dead, or its only callers are outside what was analysed |

An arena field freed individually is contradictory, so zag drops its confidence
to `medium` and says so. Arena memory is released by resetting the arena, so
either the Zig has a bug or the allocator was misidentified.

## Bodies the port reads

A function whose body is made of shapes the port can spell comes across whole,
and the report says `ported, signature and body`. Those shapes are names,
literals, field access, indexing, the arithmetic and comparison operators,
`and` and `or`, grouping, `try`, `if` with or without an else, local
declarations, assignment, `return`, `while`, `for` over one thing, calls to
functions the tables declare, and the builtins below.

A Zig `return` at the end of a body is a Rust trailing expression, because that
is what Rust writes and what its linter asks for. A `var` becomes `let mut` and
a `const` becomes `let`.

| Zig | Rust | why |
|---|---|---|
| `@min(a, b)`, `@max(a, b)` | `a.min(b)`, `a.max(b)` | the same thing, no type to invent |
| `@abs(a)` | `a.abs()` | the same |
| `@intCast(x)` | `x.try_into().unwrap()` | Rust infers the target, and the conversion is checked rather than silent |
| `a +% b`, `a -% b`, `a *% b` | `a.wrapping_add(b)` and so on | Rust has no wrapping operator |
| `a +\| b`, `a -\| b`, `a *\| b` | `a.saturating_add(b)` and so on | the same |
| `std.mem.eql(u8, a, b)` | `a == b` | slices compare by value in Rust |

`@truncate` and `@intFromEnum` are not among them, because both need a target
type that the syntax does not carry and the port will not invent one.

A call comes across when the callee is a function the tables declare, spelled
with the path that reaches it from where the call is written. The allocator
argument goes, because the ported signature does not take one. A call to
anything else, `std` included, is not written.

A `switch` becomes a `match`, and its arm patterns need the type being switched
on to say what a bare `.red` means. A parameter carries its declared type, so a
switch over one resolves; a switch over anything else does not, and is left
alone rather than guessed at. An `else` arm is `_`, and a union variant that
carries a payload binds its capture.

Everything else, a loop that captures or has an else, a method call
on a value, stops the whole body. The function still comes across as a
signature with `todo!()` in it, because a body with one hole in it looks
finished and is not. The report says which shape stopped it and where.

## Bodies the port writes for you

A struct whose Zig `init` sets every field to something the port can spell gets
a constructor, and nothing is written when even one field falls outside that
set. What it can spell is a literal, a parameter, a `len`, an `@intCast`, an
allocation, and a struct literal made of those.

The allocator parameter is gone from the signature. Where provenance resolved
it to the global allocator, `Box` is the allocator, so the port takes one fewer
argument than the Zig did. An arena stays, because the arena still has to come
from somewhere.

Everything below is the rest, which is still yours to write.

## Writing the bodies

### Ownership and cleanup

| Zig | Rust |
|---|---|
| `pub fn deinit(self: *T, allocator)` | `impl Drop for T`. Delete the body when it only frees owned fields, because `Box` and `Vec` already do that |
| `allocator.free(self.field)` | delete it, the field is a `Box` now |
| `defer x.deinit()` | delete it, `Drop` runs at scope exit |
| `errdefer alloc.free(x)` on a local | delete it, `?` drops the local on the error path |
| `errdefer` that rolls back a counter or unregisters a handle | a guard value whose `Drop` performs the rollback, disarmed on the success path |
| `allocator.dupe(u8, s)` | `Box::<[u8]>::from(s)` |
| `allocator.create(T)` and `destroy(p)` | `Box::new(..)` and dropping it |

Never expose `deinit` as a public method on the ported type. When a caller
needs to release early, take ownership: `fn close(self)`.

### Control flow

| Zig | Rust |
|---|---|
| `try x` | `x?` |
| `x catch \|e\| ...` | `.map_err(..)?`, or a `match` when the arms differ |
| `x catch return v` | `let Ok(x) = x else { return v; }` |
| `x catch v` | `x.unwrap_or(v)` |
| `x catch unreachable` | `.expect("why it cannot fail")` |
| `orelse v` | `.unwrap_or(v)` |
| `orelse return` | `.ok_or(..)?` |
| `if (x) \|y\|` | `if let Some(y) = x` |
| `while (it.next()) \|x\|` | `for x in it` |
| `switch` on a tagged union | `match` |
| `for (a, b) \|x, y\|` | `a.iter().zip(b)`, with a length assertion, because `zip` truncates where Zig asserts |

### Values

| Zig | Rust |
|---|---|
| `@intCast(x)` | `T::try_from(x).unwrap()` |
| `@truncate(x)` | `x as T` |
| `@intFromEnum(e)` | `e as uN` |
| `@memcpy(dst, src)` | `dst.copy_from_slice(src)` |
| `@min(a, b)` | `a.min(b)` |
| `a +% b` | `a.wrapping_add(b)` |
| `a -\| b` | `a.saturating_sub(b)` |
| `std.mem.eql(u8, a, b)` | `a == b` |
| `std.math.maxInt(T)` | `T::MAX` |

`@intCast` does not return an error in Zig. It traps in Debug and ReleaseSafe
and is undefined in ReleaseFast, so the port unwraps rather than propagating.
Mapping it to `?` would invent an error the Zig never had and change the error
set of whatever encloses it, which would mean a signature is not settled until
its body is written. Where the trap is the wrong behaviour for your program,
that is a decision to make deliberately and not one the port makes for you.

Bare `as` is for `@truncate` only. Every narrowing conversion goes through
`try_from`, because Zig traps where `as` silently wraps.

## Layout and the C boundary

An `extern struct` ports to `#[repr(C)]` and carries the assertions zag
generated from the layout Zig resolved:

```rust
const _: () = assert!(core::mem::size_of::<Header>() == 8);
const _: () = assert!(core::mem::align_of::<Header>() == 4);
const _: () = assert!(core::mem::offset_of!(Header, magic) == 0);
```

Keep every one. They cost nothing at runtime and they catch the class of defect
that otherwise surfaces as corruption months later. A failing assertion means
the field order or a field type changed during the port.

A `packed struct(uN)` whose fields are all `bool` becomes a `bitflags!` type.
Any other packed struct becomes `#[repr(transparent)] struct Foo(uN)` with
shift and mask accessors in the original field order. Do not reorder the
fields, and do not let a plain `#[repr(C)]` stand in, because the bit layout is
the contract.

## Locks and atomics

A Zig struct holding a `std.Thread.Mutex` beside the fields it guards is a
structural change rather than a type substitution. Rust wants the lock to own
what it protects, so the fields move inside a `Mutex<T>` and the struct keeps
one field where it had several. Nothing here decides that for you, and a port
that leaves the lock beside the data produces a type that is not `Send` and
gives no reason why.

The question it turns on, which fields are only touched while the lock is held,
is the same shape of reachability question the deinit closure answers, so this
is a gap rather than an impossibility.

`@atomicRmw`, `@atomicLoad`, `@atomicStore`, and `@cmpxchgWeak` have no rows in
the values table. Rust spells them as methods on `AtomicU32` and friends with
an explicit `Ordering`, and Zig writes the ordering at the call site, so the
translation is mechanical but the field type changes with it and nothing here
does that.

## Shared ownership

`freed outside the deinit call closure` usually means one owner that the
analysis could not see. Sometimes it means two, and there is no class for that:
a field genuinely shared between owners is `unknown` here, which is the right
answer in the sense that the port refuses to guess and the wrong one in the
sense that a reader is told nothing about what to do.

The rule against reaching for `Rc` is about intrusive structures, where a
back-pointer is a pointer and not an owner. It is not about data two owners
genuinely share. Where you have established that two owners really do share a
lifetime, `Rc` or `Arc` is the port, and the thing to avoid is reaching for it
because the borrow checker complained rather than because the program shares.

## What has no mechanical answer

Stop and decide by hand when you meet any of these. There is no rule that gets
them right.

`comptime` that inspects a type. `@typeInfo`, `@hasDecl`, and `@field` have no
Rust equivalent. A `@hasDecl` check is a trait bound. Field iteration that
implements equality or hashing is a derive. Field iteration that implements a
domain protocol is a trait with one impl per type.

`anytype`. Collect every type passed at every call site, take the union of the
methods the body calls on the parameter, and that union is the trait. When the
method sets diverge because a `comptime` branch went differently per type, the
union is wrong and the function needs splitting.

Sentinel-terminated slices. `[:0]const u8` has no Rust type. It becomes a
length-carrying wrapper over `*const u8` whose length excludes the terminator.

Intrusive structure. `@fieldParentPtr`, embedded list nodes, and back-pointers
survive the port as raw pointers with `core::mem::offset_of!`. Do not convert
them to `Rc` while porting.

Inferred error sets. Zig computes the union across the call graph. A per-crate
`thiserror` enum is right where the set is small and local. Where errors flow
through many layers, one program-wide `Copy` error type keeps the error name
stable, which matters when the name is observable.

## Before calling a file done

- No field is left `unknown`.
- No field is left `low` or `medium` without a written reason.
- Every `deinit` is gone, replaced by `Drop` or by a `close` that takes `self`.
- Every layout assertion the emitter produced is still present.
- Every widened integer has been checked for overflow behaviour the Zig relied
  on.
- No `String` or `&str` holds bytes that came from a file, a socket, or a
  syscall.
- Field order matches the Zig, so the two can be read side by side.
- No field points into another field of the same struct. Zig lets a struct hold
  its own address and Rust moves structs freely, so a self-reference that
  compiles is unsound the first time it moves. This is broader than the
  `@fieldParentPtr` case and it is the one that survives every test.
- Every owned field that is reallocated or grown is a `Vec`, not a `Box<[T]>`.
- Every `defer` that is not a plain free has been accounted for. `Drop` covers
  the frees; a `defer` that releases a lock, decrements a counter, or writes a
  line is doing something else and disappears silently if you let it.
- Every `align(` in the Zig has a matching alignment in the port.
- No borrowed pointer was widened to `&mut` without establishing that nothing
  else holds the same address.
