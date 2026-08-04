pub mod geometry {
    #[derive(Clone)]
    pub struct Material {
        pub albedo: super::vector::Vector,
        pub shininess: f32,
        pub mirror: bool,
    }

    #[derive(Clone)]
    pub struct Hit {
        pub distance: f32,
        pub point: super::vector::Vector,
        pub normal: super::vector::Vector,
        pub material: Material,
    }

    #[derive(Clone)]
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

    #[derive(Clone)]
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

    #[derive(Clone)]
    pub enum Shape {
        Sphere(Sphere),
        Plane(Plane),
    }
}

pub mod image {
    #[derive(Clone, Copy)]
    pub enum ImageInitError {
        ZeroSized,
        OutOfBounds,
        OutOfMemory,
    }

    #[derive(Clone, Copy)]
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

    #[derive(Clone)]
    pub struct Image {
        pub red: Box<[u8]>,
        pub green: Box<[u8]>,
        pub blue: Box<[u8]>,
        pub width: u32,
        pub height: u32,
    }

    impl Image {
        pub fn init(width: u32, height: u32) -> Result<Image, ImageInitError> {
            let _ = width;
            let _ = height;
            todo!()
        }
        pub fn header(&self) -> Header {
            Header {
                magic: 0x5036,
                width: self.width.try_into().unwrap(),
                height: self.height.try_into().unwrap(),
                depth: 255,
            }
        }
        pub fn offset(&self, column: u32, row: u32) -> u32 {
            row * self.width + column
        }
        pub fn write(&mut self, column: u32, row: u32, colour: super::vector::Vector) -> Result<(), Error> {
            if column >= self.width || row >= self.height {
                return Err(Error::OutOfBounds);
            }
            let at = self.offset(column, row);
            self.red[at as usize] = channel(colour.x);
            self.green[at as usize] = channel(colour.y);
            self.blue[at as usize] = channel(colour.z);
            Ok(())
        }
        pub fn luminance(&self, column: u32, row: u32) -> u32 {
            let _ = column;
            let _ = row;
            todo!()
        }
    }

    #[derive(Clone, Copy)]
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

    pub fn render(world: &super::scene::Scene, canvas: &mut super::image::Image) -> Result<(), super::image::Error> {
        let _ = world;
        let _ = canvas;
        todo!()
    }

    pub fn main() -> Result<(), super::image::ImageInitError> {
        todo!()
    }
}

pub mod scene {
    #[derive(Clone, Copy)]
    pub enum BuildError {
        OutOfMemory,
    }

    #[derive(Clone)]
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

    pub fn build() -> Result<Scene, BuildError> {
        todo!()
    }
}

pub mod vector {
    #[derive(Clone, Copy)]
    pub struct Vector {
        pub x: f32,
        pub y: f32,
        pub z: f32,
    }

    impl Vector {
        pub fn add(self, other: Vector) -> Vector {
            Vector {
                x: self.x + other.x,
                y: self.y + other.y,
                z: self.z + other.z,
            }
        }
        pub fn subtract(self, other: Vector) -> Vector {
            Vector {
                x: self.x - other.x,
                y: self.y - other.y,
                z: self.z - other.z,
            }
        }
        pub fn scale(self, factor: f32) -> Vector {
            Vector {
                x: self.x * factor,
                y: self.y * factor,
                z: self.z * factor,
            }
        }
        pub fn multiply(self, other: Vector) -> Vector {
            Vector {
                x: self.x * other.x,
                y: self.y * other.y,
                z: self.z * other.z,
            }
        }
        pub fn dot(self, other: Vector) -> f32 {
            self.x * other.x + self.y * other.y + self.z * other.z
        }
        pub fn cross(self, other: Vector) -> Vector {
            Vector {
                x: self.y * other.z - self.z * other.y,
                y: self.z * other.x - self.x * other.z,
                z: self.x * other.y - self.y * other.x,
            }
        }
        pub fn length_squared(self) -> f32 {
            self.dot(self)
        }
        pub fn length(self) -> f32 {
            self.length_squared().sqrt()
        }
        pub fn normalize(self) -> Vector {
            let size = self.length();
            if size == 0.0 {
                return self;
            }
            self.scale(1.0 / size)
        }
        pub fn negate(self) -> Vector {
            Vector {
                x: -self.x,
                y: -self.y,
                z: -self.z,
            }
        }
    }

    #[derive(Clone)]
    pub struct Ray {
        pub from: Vector,
        pub towards: Vector,
    }

    impl Ray {
        pub fn at(self, distance: f32) -> Vector {
            self.from.add(self.towards.scale(distance))
        }
    }

    pub fn splat(value: f32) -> Vector {
        Vector {
            x: value,
            y: value,
            z: value,
        }
    }

    pub fn origin() -> Vector {
        splat(0.0)
    }

    pub fn reflect(incoming: Vector, normal: Vector) -> Vector {
        incoming.subtract(normal.scale(2.0 * incoming.dot(normal)))
    }
}
