const std = @import("std");

pub const Op = enum(u8) {
    push,
    add,
    subtract,
    multiply,
    duplicate,
    drop,
    jump_if_zero,
    halt,
};

/// What the stack holds. A tagged union rather than a bare integer, so a
/// program that mixes the two is a fault the machine reports.
pub const Value = union(enum) {
    number: i64,
    flag: bool,
};

pub const Fault = error{
    StackOverflow,
    StackUnderflow,
    TypeMismatch,
    BadJump,
    NoHalt,
};

/// One decoded instruction. `operand` is only read by the ops that take one,
/// which is what keeps the program a flat array rather than a tagged tree.
pub const Instruction = struct {
    op: Op,
    operand: i64,
};

pub fn push(operand: i64) Instruction {
    return .{ .op = .push, .operand = operand };
}

pub fn plain(op: Op) Instruction {
    return .{ .op = op, .operand = 0 };
}

pub fn describe(op: Op) []const u8 {
    return switch (op) {
        .push => "push",
        .add => "add",
        .subtract => "subtract",
        .multiply => "multiply",
        .duplicate => "duplicate",
        .drop => "drop",
        .jump_if_zero => "jump_if_zero",
        .halt => "halt",
    };
}
