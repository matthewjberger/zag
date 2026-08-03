const std = @import("std");

/// A frame carries a fixed number of channels, a name it owns, and a label
/// that may not be there at all.
pub const Frame = struct {
    channels: [4]u32,
    label: ?[]const u8,
    source: []const u8,

    pub fn init(allocator: std.mem.Allocator, channels: [4]u32, source: []const u8) !Frame {
        return .{
            .channels = channels,
            .label = null,
            .source = try allocator.dupe(u8, source),
        };
    }

    pub fn deinit(self: *Frame, allocator: std.mem.Allocator) void {
        allocator.free(self.source);
    }
};

pub fn main() !void {
    const channels = [4]u32{ 11, 22, 33, 44 };
    var frame = try Frame.init(std.heap.page_allocator, channels, "sensor-a");
    defer frame.deinit(std.heap.page_allocator);
    std.debug.print("source {s} first {d} label {any}\n", .{
        frame.source,
        frame.channels[0],
        frame.label,
    });
}
