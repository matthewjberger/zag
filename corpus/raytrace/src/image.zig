const std = @import("std");
const vector = @import("vector.zig");

/// The header a PPM file starts with, laid out the way the format wants it.
/// Nothing here is read back, so it exists to be a struct whose layout the port
/// has to preserve exactly.
pub const Header = extern struct {
    magic: u16,
    width: u16,
    height: u16,
    depth: u16,
};

pub const Error = error{
    ZeroSized,
    OutOfBounds,
};

/// One owned buffer per channel rather than one interleaved buffer, so a
/// `deinit` frees three fields and the analysis has to reach all three.
pub const Image = struct {
    red: []u8,
    green: []u8,
    blue: []u8,
    width: u32,
    height: u32,

    pub fn init(allocator: std.mem.Allocator, width: u32, height: u32) !Image {
        if (width == 0 or height == 0) {
            return Error.ZeroSized;
        }
        const area = width * height;
        return .{
            .red = try allocator.alloc(u8, area),
            .green = try allocator.alloc(u8, area),
            .blue = try allocator.alloc(u8, area),
            .width = width,
            .height = height,
        };
    }

    pub fn deinit(self: *Image, allocator: std.mem.Allocator) void {
        allocator.free(self.red);
        allocator.free(self.green);
        allocator.free(self.blue);
    }

    pub fn header(self: *const Image) Header {
        return .{
            .magic = 0x5036,
            .width = @intCast(self.width),
            .height = @intCast(self.height),
            .depth = 255,
        };
    }

    fn offset(self: *const Image, column: u32, row: u32) u32 {
        return row * self.width + column;
    }

    pub fn write(self: *Image, column: u32, row: u32, colour: vector.Vector) Error!void {
        if (column >= self.width or row >= self.height) {
            return Error.OutOfBounds;
        }
        const at = self.offset(column, row);
        self.red[at] = channel(colour.x);
        self.green[at] = channel(colour.y);
        self.blue[at] = channel(colour.z);
    }

    pub fn luminance(self: *const Image, column: u32, row: u32) u32 {
        const at = self.offset(column, row);
        return (@as(u32, self.red[at]) * 3 + @as(u32, self.green[at]) * 6 + self.blue[at]) / 10;
    }
};

/// Linear colour to a byte, with the square root standing in for gamma. Clamped
/// rather than wrapped, because a sample brighter than white is ordinary.
fn channel(value: f32) u8 {
    const corrected = @sqrt(@max(0.0, @min(1.0, value)));
    return @intFromFloat(corrected * 255.0);
}
