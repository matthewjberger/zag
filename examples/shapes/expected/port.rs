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

pub fn area(shape: Shape) -> f32 {
    let _ = shape;
    todo!()
}

pub fn shade(colour: Colour) -> u32 {
    match colour {
        Colour::Red => 1,
        Colour::Green => 2,
        Colour::Blue => 3,
    }
}

pub fn parse(text: &[u8]) -> Result<Colour, ParseError> {
    let _ = text;
    todo!()
}
