const std = @import("std");
const opcode = @import("opcode.zig");

const stack_size = 32;

/// A stack machine whose whole state is a fixed array and two indices, so it
/// allocates nothing and every field is a value.
pub const Machine = struct {
    stack: [stack_size]opcode.Value,
    depth: u32,
    steps: u32,

    pub fn init() Machine {
        return .{
            .stack = undefined,
            .depth = 0,
            .steps = 0,
        };
    }

    fn push(self: *Machine, value: opcode.Value) opcode.Fault!void {
        if (self.depth == stack_size) {
            return opcode.Fault.StackOverflow;
        }
        self.stack[self.depth] = value;
        self.depth += 1;
    }

    fn pop(self: *Machine) opcode.Fault!opcode.Value {
        if (self.depth == 0) {
            return opcode.Fault.StackUnderflow;
        }
        self.depth -= 1;
        return self.stack[self.depth];
    }

    fn popNumber(self: *Machine) opcode.Fault!i64 {
        const value = try self.pop();
        return switch (value) {
            .number => |number| number,
            .flag => opcode.Fault.TypeMismatch,
        };
    }

    pub fn top(self: *const Machine) ?opcode.Value {
        if (self.depth == 0) {
            return null;
        }
        return self.stack[self.depth - 1];
    }

    pub fn run(self: *Machine, program: []const opcode.Instruction) opcode.Fault!i64 {
        var counter: u32 = 0;
        while (counter < program.len) {
            const instruction = program[counter];
            self.steps += 1;
            counter += 1;
            switch (instruction.op) {
                .push => try self.push(.{ .number = instruction.operand }),
                .add => {
                    const right = try self.popNumber();
                    const left = try self.popNumber();
                    try self.push(.{ .number = left + right });
                },
                .subtract => {
                    const right = try self.popNumber();
                    const left = try self.popNumber();
                    try self.push(.{ .number = left - right });
                },
                .multiply => {
                    const right = try self.popNumber();
                    const left = try self.popNumber();
                    try self.push(.{ .number = left * right });
                },
                .duplicate => {
                    const value = self.top() orelse return opcode.Fault.StackUnderflow;
                    try self.push(value);
                },
                .drop => _ = try self.pop(),
                .jump_if_zero => {
                    const value = try self.popNumber();
                    if (value == 0) {
                        if (instruction.operand < 0) {
                            return opcode.Fault.BadJump;
                        }
                        counter = @intCast(instruction.operand);
                    }
                },
                .halt => return self.popNumber(),
            }
        }
        return opcode.Fault.NoHalt;
    }
};

pub fn main() !void {
    const program = [_]opcode.Instruction{
        opcode.push(6),
        opcode.push(7),
        opcode.plain(.multiply),
        opcode.plain(.duplicate),
        opcode.push(2),
        opcode.plain(.subtract),
        opcode.plain(.drop),
        opcode.plain(.halt),
    };

    var machine = Machine.init();
    const answer = try machine.run(&program);
    std.debug.print("{d} after {d} steps\n", .{ answer, machine.steps });
    std.debug.print("last op was {s}\n", .{opcode.describe(.halt)});

    var empty = Machine.init();
    const underflow = empty.run(&[_]opcode.Instruction{opcode.plain(.add)});
    std.debug.print("an empty stack gives {any}\n", .{underflow});
}
