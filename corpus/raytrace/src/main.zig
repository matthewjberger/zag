const std = @import("std");
const geometry = @import("geometry.zig");
const image = @import("image.zig");
const scene = @import("scene.zig");
const vector = @import("vector.zig");

const width: u32 = 48;
const height: u32 = 24;
const maximum_bounces: u32 = 3;
const shades = " .:-=+*#%@";

/// The colour a ray comes back with, following mirrors until it runs out of
/// bounces. Recursive, which is what a bounce is, and bounded rather than
/// trusting the geometry to terminate it.
fn trace(world: *const scene.Scene, ray: vector.Ray, bounces: u32) vector.Vector {
    if (bounces == 0) {
        return vector.origin();
    }
    const touch = world.nearest(ray) orelse return world.sky;
    const material = touch.material;
    if (material.mirror) {
        const bounced = vector.Ray{
            .from = touch.point,
            .towards = vector.reflect(ray.towards, touch.normal),
        };
        const carried = trace(world, bounced, bounces - 1);
        return carried.multiply(material.albedo);
    }

    const towards_light = world.light.subtract(touch.point).normalize();
    var lit: f32 = 0.15;
    if (!world.shadowed(touch.point)) {
        lit += @max(0.0, touch.normal.dot(towards_light)) * 0.85;
    }
    return material.albedo.scale(lit);
}

fn cameraRay(column: u32, row: u32) vector.Ray {
    const across = (@as(f32, @floatFromInt(column)) / @as(f32, @floatFromInt(width))) * 2.0 - 1.0;
    const down = 1.0 - (@as(f32, @floatFromInt(row)) / @as(f32, @floatFromInt(height))) * 2.0;
    const aspect = @as(f32, @floatFromInt(width)) / @as(f32, @floatFromInt(height));
    return .{
        .from = .{ .x = 0.0, .y = 0.5, .z = 1.0 },
        .towards = (vector.Vector{
            .x = across * aspect * 0.5,
            .y = down * 0.5,
            .z = -1.0,
        }).normalize(),
    };
}

fn render(world: *const scene.Scene, canvas: *image.Image) !void {
    var row: u32 = 0;
    while (row < canvas.height) : (row += 1) {
        var column: u32 = 0;
        while (column < canvas.width) : (column += 1) {
            const colour = trace(world, cameraRay(column, row), maximum_bounces);
            try canvas.write(column, row, colour);
        }
    }
}

pub fn main() !void {
    var arena_state = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena_state.deinit();

    const world = try scene.build(arena_state.allocator());
    var canvas = try image.Image.init(std.heap.page_allocator, width, height);
    defer canvas.deinit(std.heap.page_allocator);

    try render(&world, &canvas);

    const written = canvas.header();
    std.debug.print("{d}x{d}, depth {d}\n", .{ written.width, written.height, written.depth });

    var row: u32 = 0;
    while (row < canvas.height) : (row += 2) {
        var line: [width]u8 = undefined;
        var column: u32 = 0;
        while (column < canvas.width) : (column += 1) {
            const level = canvas.luminance(column, row) * (shades.len - 1) / 255;
            line[column] = shades[level];
        }
        std.debug.print("{s}\n", .{line[0..canvas.width]});
    }
}
