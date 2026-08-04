const std = @import("std");
const geometry = @import("geometry.zig");
const vector = @import("vector.zig");

/// The shapes are arena allocated and the scene never frees them, which is the
/// case a port has to tell apart from an owning one.
pub const Scene = struct {
    shapes: []const geometry.Shape,
    light: vector.Vector,
    sky: vector.Vector,

    pub fn nearest(self: *const Scene, ray: vector.Ray) ?geometry.Hit {
        var found: ?geometry.Hit = null;
        var closest: f32 = 1000000.0;
        for (self.shapes) |shape| {
            if (shape.hit(ray, closest)) |touch| {
                closest = touch.distance;
                found = touch;
            }
        }
        return found;
    }

    /// Whether anything stands between the point and the light. The shadow ray
    /// stops at the first hit, so this asks a different question from `nearest`
    /// and cannot reuse it.
    pub fn shadowed(self: *const Scene, point: vector.Vector) bool {
        const towards = self.light.subtract(point).normalize();
        const ray = vector.Ray{ .from = point, .towards = towards };
        for (self.shapes) |shape| {
            if (shape.hit(ray, 1000000.0) != null) {
                return true;
            }
        }
        return false;
    }
};

pub fn build(arena: std.mem.Allocator) !Scene {
    var shapes: std.ArrayList(geometry.Shape) = .empty;
    try shapes.append(arena, .{ .plane = .{
        .height = -1.0,
        .material = .{
            .albedo = .{ .x = 0.6, .y = 0.6, .z = 0.6 },
            .shininess = 4.0,
            .mirror = false,
        },
    } });
    try shapes.append(arena, .{ .sphere = .{
        .centre = .{ .x = -1.1, .y = 0.0, .z = -3.0 },
        .radius = 1.0,
        .material = .{
            .albedo = .{ .x = 0.9, .y = 0.3, .z = 0.2 },
            .shininess = 32.0,
            .mirror = false,
        },
    } });
    try shapes.append(arena, .{ .sphere = .{
        .centre = .{ .x = 1.1, .y = 0.0, .z = -3.2 },
        .radius = 1.0,
        .material = .{
            .albedo = .{ .x = 0.8, .y = 0.8, .z = 0.9 },
            .shininess = 64.0,
            .mirror = true,
        },
    } });
    return .{
        .shapes = try shapes.toOwnedSlice(arena),
        .light = .{ .x = 3.0, .y = 4.0, .z = 0.0 },
        .sky = .{ .x = 0.5, .y = 0.7, .z = 1.0 },
    };
}
