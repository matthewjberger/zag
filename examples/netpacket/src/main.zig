const std = @import("std");

/// A wire header with a layout the port has to preserve exactly.
pub const Header = extern struct {
    magic: u32,
    version: u16,
    flags: u16,
    length: u32,
};

comptime {
    // The same numbers the emitted Rust asserts about its own layout. If Zig
    // ever lays this out differently, both sides fail rather than one.
    std.debug.assert(@sizeOf(Header) == 12);
    std.debug.assert(@alignOf(Header) == 4);
    std.debug.assert(@offsetOf(Header, "magic") == 0);
    std.debug.assert(@offsetOf(Header, "version") == 4);
    std.debug.assert(@offsetOf(Header, "flags") == 6);
    std.debug.assert(@offsetOf(Header, "length") == 8);
}

pub const Packet = struct {
    header: Header,
    payload: []const u8,

    pub fn init(allocator: std.mem.Allocator, version: u16, body: []const u8) !Packet {
        return .{
            .header = .{
                .magic = 0x5A414750,
                .version = version,
                .flags = 0,
                .length = @intCast(body.len),
            },
            .payload = try allocator.dupe(u8, body),
        };
    }

    /// Freed here rather than a call away, which is the case the call graph
    /// does not have to work for.
    pub fn deinit(self: *Packet, allocator: std.mem.Allocator) void {
        allocator.free(self.payload);
    }
};

pub fn main() !void {
    var packet = try Packet.init(std.heap.page_allocator, 3, "hello wire");
    defer packet.deinit(std.heap.page_allocator);
    std.debug.print("magic {x} version {d} length {d}\n", .{
        packet.header.magic,
        packet.header.version,
        packet.header.length,
    });
}
