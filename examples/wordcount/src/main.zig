const std = @import("std");

/// Counts held alongside a copy of the text they describe.
pub const Counts = struct {
    text: []const u8,
    name: []const u8,
    words: u32,
    lines: u32,

    pub fn init(allocator: std.mem.Allocator, name: []const u8, input: []const u8) !Counts {
        var words: u32 = 0;
        var lines: u32 = 0;
        var inside_word = false;
        for (input) |byte| {
            if (byte == '\n') lines += 1;
            if (byte == ' ' or byte == '\n' or byte == '\t') {
                inside_word = false;
            } else if (!inside_word) {
                inside_word = true;
                words += 1;
            }
        }
        return .{
            .text = try allocator.dupe(u8, input),
            .name = try allocator.dupe(u8, name),
            .words = words,
            .lines = lines,
        };
    }

    /// Both frees happen one call away, so deciding either field is owned
    /// needs the call graph rather than the body of this function.
    pub fn deinit(self: *Counts, allocator: std.mem.Allocator) void {
        release(self, allocator);
    }
};

fn release(self: *Counts, allocator: std.mem.Allocator) void {
    allocator.free(self.text);
    allocator.free(self.name);
}

pub fn main() !void {
    const input = "the quick brown fox\njumps over the lazy dog\n";
    var counts = try Counts.init(std.heap.page_allocator, "sample", input);
    defer counts.deinit(std.heap.page_allocator);
    std.debug.print(
        "{s}: {d} words, {d} lines, {d} bytes\n",
        .{ counts.name, counts.words, counts.lines, counts.text.len },
    );
}
