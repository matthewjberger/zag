const std = @import("std");

/// Two buffers of the same size, one being read while the other is written,
/// swapped at the end of every generation. Both are allocated by `init` and
/// both are freed by `deinit`, which is the ordinary shape and the one where a
/// reader that keeps only the first free gets the second field wrong.
pub const Grid = struct {
    cells: []u8,
    scratch: []u8,
    width: u32,
    height: u32,
    generation: u32,

    pub fn init(allocator: std.mem.Allocator, width: u32, height: u32) !Grid {
        const area = width * height;
        return .{
            .cells = try allocator.alloc(u8, area),
            .scratch = try allocator.alloc(u8, area),
            .width = width,
            .height = height,
            .generation = 0,
        };
    }

    pub fn deinit(self: *Grid, allocator: std.mem.Allocator) void {
        allocator.free(self.cells);
        allocator.free(self.scratch);
    }

    fn index(self: *const Grid, column: u32, row: u32) u32 {
        return row * self.width + column;
    }

    pub fn clear(self: *Grid) void {
        for (self.cells) |*cell| {
            cell.* = 0;
        }
    }

    pub fn set(self: *Grid, column: u32, row: u32, alive: bool) void {
        self.cells[self.index(column, row)] = if (alive) 1 else 0;
    }

    pub fn get(self: *const Grid, column: u32, row: u32) u8 {
        return self.cells[self.index(column, row)];
    }

    fn neighbours(self: *const Grid, column: u32, row: u32) u8 {
        var alive: u8 = 0;
        var vertical: i32 = -1;
        while (vertical <= 1) : (vertical += 1) {
            var horizontal: i32 = -1;
            while (horizontal <= 1) : (horizontal += 1) {
                if (vertical == 0 and horizontal == 0) {
                    continue;
                }
                const column_of = @as(i32, @intCast(column)) + horizontal;
                const row_of = @as(i32, @intCast(row)) + vertical;
                if (column_of < 0 or row_of < 0) {
                    continue;
                }
                const near = @as(u32, @intCast(column_of));
                const down = @as(u32, @intCast(row_of));
                if (near >= self.width or down >= self.height) {
                    continue;
                }
                alive += self.get(near, down);
            }
        }
        return alive;
    }

    /// Writes the next generation into the scratch buffer and swaps, so the
    /// two slices trade places rather than either being reallocated.
    pub fn step(self: *Grid) void {
        var row: u32 = 0;
        while (row < self.height) : (row += 1) {
            var column: u32 = 0;
            while (column < self.width) : (column += 1) {
                const near = self.neighbours(column, row);
                const here = self.get(column, row);
                const next: u8 = if (here == 1)
                    (if (near == 2 or near == 3) 1 else 0)
                else
                    (if (near == 3) 1 else 0);
                self.scratch[self.index(column, row)] = next;
            }
        }
        const swap = self.cells;
        self.cells = self.scratch;
        self.scratch = swap;
        self.generation += 1;
    }

    pub fn population(self: *const Grid) u32 {
        var alive: u32 = 0;
        for (self.cells) |cell| {
            alive += cell;
        }
        return alive;
    }
};

fn glider(grid: *Grid) void {
    grid.clear();
    grid.set(1, 0, true);
    grid.set(2, 1, true);
    grid.set(0, 2, true);
    grid.set(1, 2, true);
    grid.set(2, 2, true);
}

pub fn main() !void {
    var grid = try Grid.init(std.heap.page_allocator, 12, 12);
    defer grid.deinit(std.heap.page_allocator);

    glider(&grid);
    std.debug.print("generation 0 has {d} alive\n", .{grid.population()});

    var taken: u32 = 0;
    while (taken < 8) : (taken += 1) {
        grid.step();
    }
    std.debug.print("generation {d} has {d} alive\n", .{ grid.generation, grid.population() });
}
