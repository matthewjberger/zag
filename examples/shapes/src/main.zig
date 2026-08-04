const std = @import("std");

/// A plain enum. Every variant carries nothing, which is the Rust shape too.
pub const Colour = enum {
    red,
    green,
    blue,
};

pub const Extent = struct {
    width: u32,
    height: u32,
};

/// A tagged union. Every variant carries a payload, which Rust spells the same
/// way rather than as a separate tag and union. A void payload carries
/// nothing, so the port carries nothing either.
pub const Shape = union(enum) {
    circle: f32,
    rectangle: Extent,
    empty: void,
};

/// An error set is a set of names, so it ports to an enum with no payloads.
pub const ParseError = error{
    Empty,
    TooLarge,
};

pub fn area(shape: Shape) f32 {
    return switch (shape) {
        .circle => |radius| 3.14159 * radius * radius,
        .rectangle => |extent| @floatFromInt(extent.width * extent.height),
        .empty => 0.0,
    };
}

/// A switch over an enum, which is a Rust match over the same enum. The arm
/// patterns need the type being switched on, and a parameter carries it.
pub fn shade(colour: Colour) u32 {
    return switch (colour) {
        .red => 1,
        .green => 2,
        .blue => 3,
    };
}

pub fn parse(text: []const u8) ParseError!Colour {
    if (text.len == 0) return ParseError.Empty;
    if (text.len > 16) return ParseError.TooLarge;
    if (text[0] == 'r') return .red;
    if (text[0] == 'g') return .green;
    return .blue;
}

pub fn main() !void {
    const colour = try parse("green");
    std.debug.print("{s} {d}\n", .{
        @tagName(colour),
        area(.{ .rectangle = .{ .width = 3, .height = 4 } }),
    });
}
