const std = @import("std");
const entry = @import("entry.zig");

pub fn open(allocator: std.mem.Allocator, label: []const u8, amount: u32) !entry.Entry {
    return .{
        .label = try allocator.dupe(u8, label),
        .amount = amount,
    };
}

/// Names a type another file declares, which is what the port has to spell as
/// a path rather than as a bare name.
pub fn total(first: *const entry.Entry, second: *const entry.Entry) u32 {
    return first.amount + second.amount;
}
