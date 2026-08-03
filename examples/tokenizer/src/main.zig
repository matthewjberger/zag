const std = @import("std");

pub const Token = struct {
    text: []const u8,
    length: u32,
};

/// The source outlives the document and belongs to the caller. Everything the
/// tokenizer builds belongs to the arena instead, and the separator set is a
/// literal that outlives both.
pub const Document = struct {
    source: []const u8,
    tokens: []const Token,
    separators: []const u8,
};

pub fn parse(arena: std.mem.Allocator, source: []const u8) !Document {
    var counting = std.mem.tokenizeAny(u8, source, " \n\t");
    var count: usize = 0;
    while (counting.next()) |_| count += 1;

    const tokens = try arena.alloc(Token, count);
    var walking = std.mem.tokenizeAny(u8, source, " \n\t");
    var index: usize = 0;
    while (walking.next()) |word| {
        tokens[index] = .{
            .text = try arena.dupe(u8, word),
            .length = @intCast(word.len),
        };
        index += 1;
    }

    return .{
        .source = source,
        .tokens = tokens,
        .separators = " \n\t",
    };
}

pub fn main() !void {
    var arena_state = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena_state.deinit();

    const document = try parse(arena_state.allocator(), "let x = 1\nlet y = 2\n");
    std.debug.print("{d} tokens over {d} bytes\n", .{
        document.tokens.len,
        document.source.len,
    });
    for (document.tokens) |token| {
        std.debug.print("  {s} ({d})\n", .{ token.text, token.length });
    }
}
