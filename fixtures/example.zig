// The Zig this fixture stands for. Nothing reads this file: it is not a
// runnable project, so `zag read` is never pointed at it, and
// zag-facts/src/fixture.rs carries the tables it would yield. Every example
// under examples/ is read by the frontend and checked against its hand-built
// tables, so this is the one place the two are still kept in step by hand.
//
// It exists because every ownership class the analysis can reach appears below
// exactly once, which no real program does.

const std = @import("std");

pub const Buffer = struct {
    data: []const u8,
    length: u32,

    // data is allocated here and freed one call away from deinit, so deciding
    // it is owned needs the call graph, not this function.
    pub fn init(allocator: std.mem.Allocator, bytes: []const u8) !Buffer {
        return .{
            .data = try allocator.dupe(u8, bytes),
            .length = @intCast(bytes.len),
        };
    }

    pub fn deinit(self: *Buffer, allocator: std.mem.Allocator) void {
        release(self, allocator);
    }
};

fn release(self: *Buffer, allocator: std.mem.Allocator) void {
    allocator.free(self.data);
}

// The only caller that pins init's allocator to a concrete one.
pub fn makeBuffer(bytes: []const u8) !Buffer {
    return Buffer.init(std.heap.c_allocator, bytes);
}

pub const Header = extern struct {
    magic: u32,
    version: u16,
    flags: u16,
};

pub const Node = struct {
    label: []const u8,
    children: []const u32,
};

pub fn parseNode(arena: std.mem.Allocator, text: []const u8) !Node {
    return .{
        .label = try arena.dupe(u8, text),
        .children = &.{},
    };
}

// The only caller that pins parseNode's allocator, and it is an arena, so
// label is arena owned rather than heap owned.
pub fn parseTree(text: []const u8) !Node {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    return parseNode(arena.allocator(), text);
}

pub const View = struct {
    bytes: []const u8,
};

pub fn makeView(bytes: []const u8) View {
    return .{ .bytes = bytes };
}

// Nothing allocates, assigns, or frees this, so the analysis has nothing to
// go on and has to say so.
pub const Cache = struct {
    entries: []const u8,
};
