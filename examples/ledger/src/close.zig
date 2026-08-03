const std = @import("std");
const entry = @import("entry.zig");

pub fn close(self: *entry.Entry, allocator: std.mem.Allocator) void {
    allocator.free(self.label);
}
