const std = @import("std");
const document = @import("document.zig");

fn trim(text: []const u8) []const u8 {
    return std.mem.trim(u8, text, " \t\r");
}

fn isComment(line: []const u8) bool {
    return line.len == 0 or line[0] == '#' or line[0] == ';';
}

fn sectionName(line: []const u8) document.Error![]const u8 {
    if (line[line.len - 1] != ']') {
        return document.Error.UnterminatedSection;
    }
    return trim(line[1 .. line.len - 1]);
}

/// Copies the source once and points every entry into that copy, so the caller
/// may free whatever it handed in as soon as this returns.
pub fn parse(
    allocator: std.mem.Allocator,
    source: []const u8,
) (document.Error || std.mem.Allocator.Error)!document.Document {
    const text = try allocator.dupe(u8, source);
    var entries: std.ArrayList(document.Entry) = .empty;
    defer entries.deinit(allocator);

    var section: []const u8 = "";
    var lines = std.mem.splitScalar(u8, text, '\n');
    while (lines.next()) |raw| {
        const line = trim(raw);
        if (isComment(line)) {
            continue;
        }
        if (line[0] == '[') {
            section = try sectionName(line);
            continue;
        }
        const equals = std.mem.indexOfScalar(u8, line, '=') orelse {
            return document.Error.MissingEquals;
        };
        const key = trim(line[0..equals]);
        if (key.len == 0) {
            return document.Error.EmptyKey;
        }
        try entries.append(allocator, .{
            .section = section,
            .key = key,
            .value = trim(line[equals + 1 ..]),
        });
    }

    return .{
        .text = text,
        .entries = try entries.toOwnedSlice(allocator),
    };
}
