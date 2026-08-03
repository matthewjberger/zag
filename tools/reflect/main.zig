//! Prints the declarations and layout of a Zig module, as resolved by the
//! compiler rather than guessed. The hand-built fact tables are checked
//! against this, so a table that disagrees with the program it claims to
//! describe fails rather than porting something that was never there.
//!
//! Reflection reaches declarations and layout. It does not reach the call
//! graph, the memory operations, or the field assignments, which is the part
//! still supplied by hand and the part that needs the compiler's semantic
//! analysis rather than its type information.

const std = @import("std");
const target = @import("target");

fn printStruct(comptime name: []const u8, comptime Type: type) void {
    const info = @typeInfo(Type).@"struct";
    const layout = switch (info.layout) {
        .@"extern" => "extern",
        .@"packed" => "packed",
        .auto => "auto",
    };
    std.debug.print("struct {s} layout={s} size={d} align={d} fields={d}\n", .{
        name,
        layout,
        @sizeOf(Type),
        @alignOf(Type),
        info.fields.len,
    });
    inline for (info.fields) |field| {
        std.debug.print("field {s}.{s} type={s} size={d} offset={d}\n", .{
            name,
            field.name,
            @typeName(field.type),
            @sizeOf(field.type),
            @offsetOf(Type, field.name),
        });
    }
    inline for (info.decls) |declaration| {
        const value = @field(Type, declaration.name);
        if (@typeInfo(@TypeOf(value)) == .@"fn") {
            printFunction(name ++ "." ++ declaration.name, @TypeOf(value));
        }
    }
}

fn printFunction(comptime name: []const u8, comptime Type: type) void {
    const info = @typeInfo(Type).@"fn";
    std.debug.print("fn {s} params={d}\n", .{ name, info.params.len });
    inline for (info.params, 0..) |parameter, index| {
        const kind = if (parameter.type) |resolved| @typeName(resolved) else "anytype";
        std.debug.print("param {s}.{d} type={s}\n", .{ name, index, kind });
    }
}

pub fn main() void {
    @setEvalBranchQuota(20000);
    const module = @typeInfo(target).@"struct";
    inline for (module.decls) |declaration| {
        const value = @field(target, declaration.name);
        const Value = @TypeOf(value);
        if (Value == type) {
            switch (@typeInfo(value)) {
                .@"struct" => printStruct(declaration.name, value),
                else => {},
            }
        } else if (@typeInfo(Value) == .@"fn") {
            printFunction(declaration.name, Value);
        }
    }
}
