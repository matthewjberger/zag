//! Prints the declarations and layout of a Zig module, as resolved by the
//! compiler rather than guessed. The hand-built fact tables are checked
//! against this, so a table that disagrees with the program it claims to
//! describe fails rather than porting something that was never there.
//!
//! The report is built at comptime and delivered through `@compileError`, so
//! this is only ever analysed and never linked or run. Linking would drag in
//! a platform's libc and its SDK, which is a way to fail that has nothing to
//! do with what the compiler resolved.
//!
//! Reflection reaches declarations and layout. It does not reach the call
//! graph, the memory operations, or the field assignments, which is the part
//! still supplied by hand and the part that needs the compiler's semantic
//! analysis rather than its type information.

const std = @import("std");
const target = @import("target");

fn describeFunction(comptime name: []const u8, comptime Type: type) []const u8 {
    const info = @typeInfo(Type).@"fn";
    var out: []const u8 = std.fmt.comptimePrint("fn {s} params={d}\n", .{ name, info.params.len });
    for (info.params, 0..) |parameter, index| {
        const kind = if (parameter.type) |resolved| @typeName(resolved) else "anytype";
        out = out ++ std.fmt.comptimePrint("param {s}.{d} type={s}\n", .{ name, index, kind });
    }
    return out;
}

fn describeStruct(comptime name: []const u8, comptime Type: type) []const u8 {
    const info = @typeInfo(Type).@"struct";
    const layout = switch (info.layout) {
        .@"extern" => "extern",
        .@"packed" => "packed",
        .auto => "auto",
    };
    var out: []const u8 = std.fmt.comptimePrint(
        "struct {s} layout={s} size={d} align={d} fields={d}\n",
        .{ name, layout, @sizeOf(Type), @alignOf(Type), info.fields.len },
    );
    for (info.fields) |field| {
        out = out ++ std.fmt.comptimePrint(
            "field {s}.{s} type={s} size={d} offset={d}\n",
            .{ name, field.name, @typeName(field.type), @sizeOf(field.type), @offsetOf(Type, field.name) },
        );
    }
    for (info.decls) |declaration| {
        const value = @field(Type, declaration.name);
        if (@typeInfo(@TypeOf(value)) == .@"fn") {
            out = out ++ describeFunction(name ++ "." ++ declaration.name, @TypeOf(value));
        }
    }
    return out;
}

fn describeModule() []const u8 {
    var out: []const u8 = "\n";
    for (@typeInfo(target).@"struct".decls) |declaration| {
        const value = @field(target, declaration.name);
        const Value = @TypeOf(value);
        if (Value == type) {
            if (@typeInfo(value) == .@"struct") {
                out = out ++ describeStruct(declaration.name, value);
            }
        } else if (@typeInfo(Value) == .@"fn") {
            out = out ++ describeFunction(declaration.name, Value);
        }
    }
    return out;
}

comptime {
    @setEvalBranchQuota(200000);
    @compileError(describeModule());
}
