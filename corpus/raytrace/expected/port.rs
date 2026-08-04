pub mod geometry {
    pub struct Material {
        pub albedo: super::vector::Vector,
        pub shininess: f32,
        pub mirror: bool,
    }

    pub struct Hit {
        pub distance: f32,
        pub point: super::vector::Vector,
        pub normal: super::vector::Vector,
        pub material: Material,
    }

    pub struct Sphere {
        pub centre: super::vector::Vector,
        pub radius: f32,
        pub material: Material,
    }

    impl Sphere {
        pub fn hit(self, ray: super::vector::Ray, nearest: f32) -> Option<Hit> {
            let _ = ray;
            let _ = nearest;
            todo!()
        }
    }

    pub struct Plane {
        pub height: f32,
        pub material: Material,
    }

    impl Plane {
        pub fn hit(self, ray: super::vector::Ray, nearest: f32) -> Option<Hit> {
            let _ = ray;
            let _ = nearest;
            todo!()
        }
    }

    pub enum Shape {
        Sphere(Sphere),
        Plane(Plane),
    }
}

pub mod image {
    #[repr(C)]
    pub struct Header {
        pub magic: u16,
        pub width: u16,
        pub height: u16,
        pub depth: u16,
    }

    const _: () = assert!(core::mem::size_of::<Header>() == 8);
    const _: () = assert!(core::mem::align_of::<Header>() == 2);
    const _: () = assert!(core::mem::offset_of!(Header, magic) == 0);
    const _: () = assert!(core::mem::offset_of!(Header, width) == 2);
    const _: () = assert!(core::mem::offset_of!(Header, height) == 4);
    const _: () = assert!(core::mem::offset_of!(Header, depth) == 6);

    pub struct Image {
        pub red: Box<[u8]>,
        pub green: Box<[u8]>,
        pub blue: Box<[u8]>,
        pub width: u32,
        pub height: u32,
    }

    impl Image {
        pub fn header(&self) -> Header {
            todo!()
        }
        pub fn offset(&self, column: u32, row: u32) -> u32 {
            row * self.width + column
        }
        pub fn write(&mut self, column: u32, row: u32, colour: super::vector::Vector) -> Result<(), Error> {
            let _ = column;
            let _ = row;
            let _ = colour;
            todo!()
        }
        pub fn luminance(&self, column: u32, row: u32) -> u32 {
            let _ = column;
            let _ = row;
            todo!()
        }
    }

    pub enum Error {
        ZeroSized,
        OutOfBounds,
    }

    pub fn channel(value: f32) -> u8 {
        let _ = value;
        todo!()
    }
}

pub mod main {
    pub fn trace(world: &super::scene::Scene, ray: super::vector::Ray, bounces: u32) -> super::vector::Vector {
        let _ = world;
        let _ = ray;
        let _ = bounces;
        todo!()
    }

    pub fn camera_ray(column: u32, row: u32) -> super::vector::Ray {
        let _ = column;
        let _ = row;
        todo!()
    }
}

pub mod scene {
    pub struct Scene {
        pub shapes: Option<core::ptr::NonNull<[super::geometry::Shape]>>,
        pub light: super::vector::Vector,
        pub sky: super::vector::Vector,
    }

    impl Scene {
        pub fn nearest(&self, ray: super::vector::Ray) -> Option<super::geometry::Hit> {
            let _ = ray;
            todo!()
        }
        pub fn shadowed(&self, point: super::vector::Vector) -> bool {
            let _ = point;
            todo!()
        }
    }
}

pub mod vector {
    pub struct Vector {
        pub x: f32,
        pub y: f32,
        pub z: f32,
    }

    impl Vector {
        pub fn add(self, other: Vector) -> Vector {
            let _ = other;
            todo!()
        }
        pub fn subtract(self, other: Vector) -> Vector {
            let _ = other;
            todo!()
        }
        pub fn scale(self, factor: f32) -> Vector {
            let _ = factor;
            todo!()
        }
        pub fn multiply(self, other: Vector) -> Vector {
            let _ = other;
            todo!()
        }
        pub fn dot(self, other: Vector) -> f32 {
            self.x * other.x + self.y * other.y + self.z * other.z
        }
        pub fn cross(self, other: Vector) -> Vector {
            let _ = other;
            todo!()
        }
        pub fn length_squared(self) -> f32 {
            todo!()
        }
        pub fn length(self) -> f32 {
            todo!()
        }
        pub fn normalize(self) -> Vector {
            todo!()
        }
        pub fn negate(self) -> Vector {
            todo!()
        }
    }

    pub struct Ray {
        pub from: Vector,
        pub towards: Vector,
    }

    impl Ray {
        pub fn at(self, distance: f32) -> Vector {
            let _ = distance;
            todo!()
        }
    }

    pub fn splat(value: f32) -> Vector {
        let _ = value;
        todo!()
    }

    pub fn origin() -> Vector {
        todo!()
    }

    pub fn reflect(incoming: Vector, normal: Vector) -> Vector {
        let _ = incoming;
        let _ = normal;
        todo!()
    }
}
