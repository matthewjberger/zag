pub mod main {
    #[derive(Clone)]
    pub struct Machine {
        pub stack: [super::opcode::Value; 32],
        pub depth: u32,
        pub steps: u32,
    }

    impl Machine {
        pub fn init() -> Machine {
            todo!()
        }
        pub fn push(&mut self, value: super::opcode::Value) -> Result<(), super::opcode::Fault> {
            let _ = value;
            todo!()
        }
        pub fn pop(&mut self) -> Result<super::opcode::Value, super::opcode::Fault> {
            todo!()
        }
        pub fn pop_number(&mut self) -> Result<i64, super::opcode::Fault> {
            todo!()
        }
        pub fn top(&self) -> Option<super::opcode::Value> {
            if self.depth == 0 {
                return None;
            }
            Some(self.stack[(self.depth - 1) as usize])
        }
        pub fn run(&mut self, program: &[super::opcode::Instruction]) -> Result<i64, super::opcode::Fault> {
            let _ = program;
            todo!()
        }
    }
}

pub mod opcode {
    #[derive(Clone, Copy)]
    pub enum Op {
        Push,
        Add,
        Subtract,
        Multiply,
        Duplicate,
        Drop,
        JumpIfZero,
        Halt,
    }

    #[derive(Clone, Copy)]
    pub enum Value {
        Number(i64),
        Flag(bool),
    }

    #[derive(Clone)]
    pub struct Instruction {
        pub op: Op,
        pub operand: i64,
    }

    #[derive(Clone, Copy)]
    pub enum Fault {
        StackOverflow,
        StackUnderflow,
        TypeMismatch,
        BadJump,
        NoHalt,
    }

    pub fn push(operand: i64) -> Instruction {
        let _ = operand;
        todo!()
    }

    pub fn plain(op: Op) -> Instruction {
        Instruction {
            op,
            operand: 0,
        }
    }
}
