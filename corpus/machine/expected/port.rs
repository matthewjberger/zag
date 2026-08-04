pub mod main {
    pub struct Machine {
        pub stack: super::opcode::Value,
        pub depth: u32,
        pub steps: u32,
    }

    impl Machine {
        pub fn init() -> Machine {
            todo!()
        }
        pub fn push(&mut self, value: super::opcode::Value) -> Result<(), Fault> {
            let _ = value;
            todo!()
        }
        pub fn pop(&mut self) -> Result<super::opcode::Value, Fault> {
            todo!()
        }
        pub fn pop_number(&mut self) -> Result<i64, Fault> {
            todo!()
        }
        pub fn top(&self) -> Option<super::opcode::Value> {
            if self.depth == 0 {
                null
            }
            self.stack[self.depth - 1]
        }
        pub fn run(&mut self, program: &[super::opcode::Instruction]) -> Result<i64, Fault> {
            let _ = program;
            todo!()
        }
    }
}

pub mod opcode {
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

    pub enum Value {
        Number(i64),
        Flag(bool),
    }

    pub struct Instruction {
        pub op: Op,
        pub operand: i64,
    }

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
        let _ = op;
        todo!()
    }

    pub fn describe(op: Op) -> [u8] {
        match op {
            Op::Push => "push",
            Op::Add => "add",
            Op::Subtract => "subtract",
            Op::Multiply => "multiply",
            Op::Duplicate => "duplicate",
            Op::Drop => "drop",
            Op::JumpIfZero => "jump_if_zero",
            Op::Halt => "halt",
        }
    }
}
