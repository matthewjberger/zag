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
| `wordcount` | `owned`, where the free is a call away from `deinit` |
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
| `owned` | allocated from the global allocator, freed through the owner's `deinit` | `Box<[T]>` or `Box<T>` | that `Drop` now does what the Zig free did |
| `borrowed` | assigned from a parameter the caller keeps, never freed | `&'a [T]`, and the struct gains `<'a>` | that the caller really outlives the struct |
| `static` | assigned only from literals, never freed | `&'static [T]` | nothing |
| `arena` | allocated from an arena | `&'bump [T]`, and the struct gains `<'bump>` | that the arena outlives the struct |
| `unknown` | the evidence is missing or disagrees | `Option<core::ptr::NonNull<[T]>>` | all of it, this is the flag |

An `unknown` field compiles and carries no ownership. Leaving one in a finished
port means the port has a pointer nobody owns.

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

A `warning:` line above the fields means allocator provenance did not settle.
Every allocator below it is understated, so treat the whole report as
provisional until that is fixed.

## Types

The type a field gets is its Zig type crossed with its ownership class.

| Zig | class | Rust |
|---|---|---|
| `[]const u8` | `owned` | `Box<[u8]>` |
| `[]const u8` | `borrowed` | `&'a [u8]` |
| `[]const u8` | `static` | `&'static [u8]` |
| `[]const u8` | `arena` | `&'bump [u8]` |
| `[]const u8` | `unknown` | `Option<core::ptr::NonNull<[u8]>>` |
| `*T` | `owned` | `Box<T>` |
| `*T` | `borrowed` | `&'a T` |
| `u8`, `u16`, `u32`, `u64` | `value` | `u8`, `u16`, `u32`, `u64` |
| `bool` | `value` | `bool` |
| `void` | `value` | `()` |

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
| `@intCast(x)` | `T::try_from(x)?` |
| `@truncate(x)` | `x as T` |
| `@intFromEnum(e)` | `e as uN` |
| `@memcpy(dst, src)` | `dst.copy_from_slice(src)` |
| `@min(a, b)` | `a.min(b)` |
| `a +% b` | `a.wrapping_add(b)` |
| `a -\| b` | `a.saturating_sub(b)` |
| `std.mem.eql(u8, a, b)` | `a == b` |
| `std.math.maxInt(T)` | `T::MAX` |

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
