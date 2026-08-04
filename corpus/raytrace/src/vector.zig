const std = @import("std");

/// Three floats and nothing else. Every function here is pure arithmetic over
/// fields, which is the shape most of a renderer is made of.
pub const Vector = struct {
    x: f32,
    y: f32,
    z: f32,

    pub fn add(self: Vector, other: Vector) Vector {
        return .{
            .x = self.x + other.x,
            .y = self.y + other.y,
            .z = self.z + other.z,
        };
    }

    pub fn subtract(self: Vector, other: Vector) Vector {
        return .{
            .x = self.x - other.x,
            .y = self.y - other.y,
            .z = self.z - other.z,
        };
    }

    pub fn scale(self: Vector, factor: f32) Vector {
        return .{
            .x = self.x * factor,
            .y = self.y * factor,
            .z = self.z * factor,
        };
    }

    pub fn multiply(self: Vector, other: Vector) Vector {
        return .{
            .x = self.x * other.x,
            .y = self.y * other.y,
            .z = self.z * other.z,
        };
    }

    pub fn dot(self: Vector, other: Vector) f32 {
        return self.x * other.x + self.y * other.y + self.z * other.z;
    }

    pub fn cross(self: Vector, other: Vector) Vector {
        return .{
            .x = self.y * other.z - self.z * other.y,
            .y = self.z * other.x - self.x * other.z,
            .z = self.x * other.y - self.y * other.x,
        };
    }

    pub fn lengthSquared(self: Vector) f32 {
        return self.dot(self);
    }

    pub fn length(self: Vector) f32 {
        return @sqrt(self.lengthSquared());
    }

    pub fn normalize(self: Vector) Vector {
        const size = self.length();
        if (size == 0) {
            return self;
        }
        return self.scale(1.0 / size);
    }

    pub fn negate(self: Vector) Vector {
        return .{ .x = -self.x, .y = -self.y, .z = -self.z };
    }
};

pub fn splat(value: f32) Vector {
    return .{ .x = value, .y = value, .z = value };
}

pub fn origin() Vector {
    return splat(0);
}

/// A point and a direction. The direction is expected to be normalised, which
/// is the caller's job rather than something checked on every use.
pub const Ray = struct {
    from: Vector,
    towards: Vector,

    pub fn at(self: Ray, distance: f32) Vector {
        return self.from.add(self.towards.scale(distance));
    }
};

pub fn reflect(incoming: Vector, normal: Vector) Vector {
    return incoming.subtract(normal.scale(2.0 * incoming.dot(normal)));
}
