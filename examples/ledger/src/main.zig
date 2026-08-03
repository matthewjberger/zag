const std = @import("std");
const close = @import("close.zig");
const store = @import("store.zig");

pub fn main() !void {
    var opened = try store.open(std.heap.page_allocator, "rent", 1200);
    defer close.close(&opened, std.heap.page_allocator);
    std.debug.print("{s} {d}\n", .{ opened.label, opened.amount });
}
