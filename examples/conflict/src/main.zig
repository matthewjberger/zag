const std = @import("std");

/// Nothing here says who owns entries. One caller of makeCache passes the heap
/// and another passes an arena, so the field has no single answer and the port
/// cannot be written without picking one. That disagreement is the finding.
pub const Cache = struct {
    entries: []const u8,
};

pub fn makeCache(allocator: std.mem.Allocator, bytes: []const u8) !Cache {
    return .{ .entries = try allocator.dupe(u8, bytes) };
}

pub fn fromHeap(bytes: []const u8) !Cache {
    return makeCache(std.heap.page_allocator, bytes);
}

pub fn fromArena(arena: std.mem.Allocator, bytes: []const u8) !Cache {
    return makeCache(arena, bytes);
}

pub fn main() !void {
    const heap = try fromHeap("alpha");
    // Correct only because this caller happens to know which allocator ran.
    defer std.heap.page_allocator.free(heap.entries);

    var arena_state = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena_state.deinit();
    const pooled = try fromArena(arena_state.allocator(), "beta");

    std.debug.print("{d} heap bytes, {d} arena bytes\n", .{
        heap.entries.len,
        pooled.entries.len,
    });
}
