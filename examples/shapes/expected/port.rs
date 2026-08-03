pub enum Colour {
    Red,
    Green,
    Blue,
}

pub struct Extent {
    pub width: u32,
    pub height: u32,
}

pub enum Shape {
    Circle(f32),
    Rectangle(Extent),
    Empty,
}

pub enum ParseError {
    Empty,
    TooLarge,
}
