const std = @import("std");

/// Counts held alongside a copy of the text they describe.
pub const Counts = struct {
    text: []const u8,
    words: u32,
    lines: u32,

    pub fn init(allocator: std.mem.Allocator, input: []const u8) !Counts {
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
            .words = words,
            .lines = lines,
        };
    }

    /// The free happens one call away, so deciding that text is owned needs
    /// the call graph rather than the body of this function.
    pub fn deinit(self: *Counts, allocator: std.mem.Allocator) void {
        release(self, allocator);
    }
};

fn release(self: *Counts, allocator: std.mem.Allocator) void {
    allocator.free(self.text);
}

pub fn main() !void {
    const input = "the quick brown fox\njumps over the lazy dog\n";
    var counts = try Counts.init(std.heap.page_allocator, input);
    defer counts.deinit(std.heap.page_allocator);
    std.debug.print(
        "{d} words, {d} lines, {d} bytes\n",
        .{ counts.words, counts.lines, counts.text.len },
    );
}
