pub mod main {
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
            todo!()
        }
        pub fn run(&mut self, program: &[super::opcode::Instruction]) -> Result<i64, super::opcode::Fault> {
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
}
