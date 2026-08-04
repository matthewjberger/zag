pub mod main {
    #[derive(Clone, Copy)]
    pub enum GridInitError {
        OutOfMemory,
    }

    #[derive(Clone)]
    pub struct Grid {
        pub cells: Box<[u8]>,
        pub scratch: Box<[u8]>,
        pub width: u32,
        pub height: u32,
        pub generation: u32,
    }

    impl Grid {
        pub fn init(width: u32, height: u32) -> Result<Grid, GridInitError> {
            let _ = width;
            let _ = height;
            todo!()
        }
        pub fn index(&self, column: u32, row: u32) -> u32 {
            row * self.width + column
        }
        pub fn clear(&mut self) {
            todo!()
        }
        pub fn set(&mut self, column: u32, row: u32, alive: bool) {
            self.cells[self.index(column, row) as usize] = if alive {
                1
            } else {
                0
            };
        }
        pub fn get(&self, column: u32, row: u32) -> u8 {
            self.cells[self.index(column, row) as usize]
        }
        pub fn neighbours(&self, column: u32, row: u32) -> u8 {
            let _ = column;
            let _ = row;
            todo!()
        }
        pub fn step(&mut self) {
            todo!()
        }
        pub fn population(&self) -> u32 {
            todo!()
        }
    }

    pub fn glider(grid: &mut Grid) {
        let _ = grid;
        todo!()
    }

    pub fn main() -> Result<(), GridInitError> {
        todo!()
    }
}
