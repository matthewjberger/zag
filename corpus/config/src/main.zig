const std = @import("std");
const document = @import("document.zig");
const parser = @import("parser.zig");

const sample =
    \\# a small config, of the shape every project grows one of
    \\name = zag
    \\
    \\[build]
    \\optimize = ReleaseSafe
    \\target = native
    \\
    \\[report]
    \\evidence = true
    \\width = 80
;

pub fn main() !void {
    var arena_state = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena_state.deinit();

    var parsed = try parser.parse(std.heap.page_allocator, sample);
    defer parsed.deinit(std.heap.page_allocator);

    std.debug.print("{d} entries, {d} under build\n", .{
        parsed.count(),
        parsed.sectionSize("build"),
    });
    if (parsed.lookup("report", "width")) |width| {
        std.debug.print("report width is {s}\n", .{width});
    }
    if (parsed.lookup("build", "missing") == null) {
        std.debug.print("build has no missing key, which is the answer\n", .{});
    }
}
