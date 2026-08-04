const std = @import("std");
const vector = @import("vector.zig");

pub const Material = struct {
    albedo: vector.Vector,
    shininess: f32,
    mirror: bool,
};

/// What a ray found where it hit. Returned as an optional, so a miss is the
/// absence of one rather than a distance nobody should read.
pub const Hit = struct {
    distance: f32,
    point: vector.Vector,
    normal: vector.Vector,
    material: Material,
};

pub const Sphere = struct {
    centre: vector.Vector,
    radius: f32,
    material: Material,

    pub fn hit(self: Sphere, ray: vector.Ray, nearest: f32) ?Hit {
        const offset = ray.from.subtract(self.centre);
        const along = ray.towards.dot(offset);
        const outside = offset.lengthSquared() - self.radius * self.radius;
        const discriminant = along * along - outside;
        if (discriminant < 0) {
            return null;
        }
        const root = @sqrt(discriminant);
        var distance = -along - root;
        if (distance < 0.001) {
            distance = -along + root;
        }
        if (distance < 0.001 or distance > nearest) {
            return null;
        }
        const point = ray.at(distance);
        return .{
            .distance = distance,
            .point = point,
            .normal = point.subtract(self.centre).normalize(),
            .material = self.material,
        };
    }
};

pub const Plane = struct {
    height: f32,
    material: Material,

    pub fn hit(self: Plane, ray: vector.Ray, nearest: f32) ?Hit {
        if (@abs(ray.towards.y) < 0.0001) {
            return null;
        }
        const distance = (self.height - ray.from.y) / ray.towards.y;
        if (distance < 0.001 or distance > nearest) {
            return null;
        }
        return .{
            .distance = distance,
            .point = ray.at(distance),
            .normal = .{ .x = 0, .y = 1, .z = 0 },
            .material = self.material,
        };
    }
};

/// The two shapes as one thing a scene can hold. A tagged union rather than a
/// table of function pointers, because the set is closed and the switch is the
/// whole dispatch.
pub const Shape = union(enum) {
    sphere: Sphere,
    plane: Plane,

    pub fn hit(self: Shape, ray: vector.Ray, nearest: f32) ?Hit {
        return switch (self) {
            .sphere => |shape| shape.hit(ray, nearest),
            .plane => |shape| shape.hit(ray, nearest),
        };
    }
};
