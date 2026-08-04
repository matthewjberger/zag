pub mod main {
    pub struct Grid {
        pub cells: Box<[u8]>,
        pub scratch: Box<[u8]>,
        pub width: u32,
        pub height: u32,
        pub generation: u32,
    }

    impl Grid {
        pub fn index(&self, column: u32, row: u32) -> u32 {
            row * self.width + column
        }
        pub fn clear(&mut self) {
            todo!()
        }
        pub fn set(&mut self, column: u32, row: u32, alive: bool) {
            let _ = column;
            let _ = row;
            let _ = alive;
            todo!()
        }
        pub fn get(&self, column: u32, row: u32) -> u8 {
            let _ = column;
            let _ = row;
            todo!()
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

    pub fn glider(grid: &Grid) {
        let _ = grid;
        todo!()
    }
}
