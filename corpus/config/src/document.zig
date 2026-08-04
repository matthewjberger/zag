const std = @import("std");

/// One `key = value` line, together with the section heading it appeared
/// under. All three point into the document's own copy of the text.
pub const Entry = struct {
    section: []const u8,
    key: []const u8,
    value: []const u8,
};

pub const Error = error{
    MissingEquals,
    UnterminatedSection,
    EmptyKey,
};

/// A parsed config file. The document owns one copy of the source text and one
/// array of entries pointing into it, so both are freed together and neither
/// outlives the other.
pub const Document = struct {
    text: []const u8,
    entries: []Entry,

    pub fn deinit(self: *Document, allocator: std.mem.Allocator) void {
        allocator.free(self.entries);
        allocator.free(self.text);
    }

    pub fn lookup(self: *const Document, section: []const u8, key: []const u8) ?[]const u8 {
        for (self.entries) |entry| {
            if (std.mem.eql(u8, entry.section, section) and std.mem.eql(u8, entry.key, key)) {
                return entry.value;
            }
        }
        return null;
    }

    pub fn count(self: *const Document) u32 {
        return @intCast(self.entries.len);
    }

    pub fn sectionSize(self: *const Document, section: []const u8) u32 {
        var found: u32 = 0;
        for (self.entries) |entry| {
            if (std.mem.eql(u8, entry.section, section)) {
                found += 1;
            }
        }
        return found;
    }
};
