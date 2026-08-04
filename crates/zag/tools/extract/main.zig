//! Reads a Zig file with the compiler's own parser and reports everything the
//! fact tables need that does not require a type to decide: containers and
//! their fields, functions and their parameters, the call graph, what each
//! local is initialised from, and what each struct literal writes into each
//! field.
//!
//! Layout is not here. `tools/reflect` gets that from the compiler, and the
//! frontend merges the two.
//!
//! This is syntax. It recognises `x.dupe(...)` as an allocation because of how
//! it is spelled, not because it resolved `x` to an allocator. Comptime and
//! generics need semantic analysis and are out of reach.
//!
//! Nodes are attributed to the innermost declaration whose token range covers
//! them, which avoids traversing every expression shape the language has.

const std = @import("std");
const Ast = std.zig.Ast;

const Span = struct {
    name: []const u8,
    first: Ast.TokenIndex,
    last: Ast.TokenIndex,
    owner: ?usize,
    node: Ast.Node.Index,
};

fn collapse(allocator: std.mem.Allocator, text: []const u8) ![]const u8 {
    var out: std.ArrayList(u8) = .empty;
    var pending = false;
    for (std.mem.trim(u8, text, " \t\r\n")) |byte| {
        if (byte == ' ' or byte == '\t' or byte == '\r' or byte == '\n') {
            pending = true;
            continue;
        }
        if (pending and out.items.len != 0) try out.append(allocator, ' ');
        pending = false;
        try out.append(allocator, byte);
    }
    return out.items;
}

fn innermost(spans: []const Span, token: Ast.TokenIndex) ?usize {
    var found: ?usize = null;
    for (spans, 0..) |span, index| {
        if (token < span.first or token > span.last) continue;
        const better = if (found) |current|
            span.last - span.first < spans[current].last - spans[current].first
        else
            true;
        if (better) found = index;
    }
    return found;
}

fn containerName(tree: Ast, container: Ast.Node.Index) ?[]const u8 {
    var walk = tree.firstToken(container);
    while (walk > 0) {
        walk -= 1;
        if (tree.tokenTag(walk) == .identifier and walk > 0 and
            (tree.tokenTag(walk - 1) == .keyword_const or tree.tokenTag(walk - 1) == .keyword_var))
        {
            return tree.tokenSlice(walk);
        }
        if (tree.tokenTag(walk) == .semicolon) return null;
    }
    return null;
}

fn isContainer(tag: Ast.Node.Tag) bool {
    return switch (tag) {
        .container_decl,
        .container_decl_trailing,
        .container_decl_two,
        .container_decl_two_trailing,
        .container_decl_arg,
        .container_decl_arg_trailing,
        // A tagged union has its own tags rather than being a container with
        // an argument, so leaving these out loses every union in the file.
        .tagged_union,
        .tagged_union_trailing,
        .tagged_union_two,
        .tagged_union_two_trailing,
        .tagged_union_enum_tag,
        .tagged_union_enum_tag_trailing,
        => true,
        else => false,
    };
}

fn containerKeyword(tree: Ast, container: Ast.Node.Index) []const u8 {
    var walk = tree.firstToken(container);
    const limit = @min(walk + 4, tree.tokens.len);
    while (walk < limit) : (walk += 1) {
        switch (tree.tokenTag(walk)) {
            .keyword_struct => return "struct",
            .keyword_enum => return "enum",
            .keyword_union => return "union",
            .keyword_opaque => return "opaque",
            else => {},
        }
    }
    return "struct";
}

fn isStructInit(tag: Ast.Node.Tag) bool {
    return switch (tag) {
        .struct_init,
        .struct_init_comma,
        .struct_init_one,
        .struct_init_one_comma,
        .struct_init_dot,
        .struct_init_dot_comma,
        .struct_init_dot_two,
        .struct_init_dot_two_comma,
        => true,
        else => false,
    };
}

/// The Rust spelling of a binary operator, or nothing where Rust has no
/// operator that means the same thing. Zig's `and` and `or` are Rust's `&&`
/// and `||`, and the arithmetic ones carry across unchanged.
fn binaryOperator(tag: Ast.Node.Tag) ?[]const u8 {
    return switch (tag) {
        .add => "+",
        .sub => "-",
        .mul => "*",
        .div => "/",
        .mod => "%",
        .bit_and => "&",
        .bit_or => "|",
        .bit_xor => "^",
        .shl => "<<",
        .shr => ">>",
        .equal_equal => "==",
        .bang_equal => "!=",
        .less_than => "<",
        .greater_than => ">",
        .less_or_equal => "<=",
        .greater_or_equal => ">=",
        .bool_and => "&&",
        .bool_or => "||",
        else => null,
    };
}

/// Zig's wrapping and saturating operators, which Rust spells as methods
/// rather than as operators. The name is what the caller calls them.
fn overflowOperator(tag: Ast.Node.Tag) ?[]const u8 {
    return switch (tag) {
        .add_wrap => "wrapping_add",
        .sub_wrap => "wrapping_sub",
        .mul_wrap => "wrapping_mul",
        .add_sat => "saturating_add",
        .sub_sat => "saturating_sub",
        .mul_sat => "saturating_mul",
        else => null,
    };
}

fn isBuiltinCall(tag: Ast.Node.Tag) bool {
    return switch (tag) {
        .builtin_call,
        .builtin_call_comma,
        .builtin_call_two,
        .builtin_call_two_comma,
        => true,
        else => false,
    };
}

fn unaryOperator(tag: Ast.Node.Tag) ?[]const u8 {
    return switch (tag) {
        .bool_not => "!",
        .negation => "-",
        else => null,
    };
}

const Walker = struct {
    tree: Ast,
    arena: std.mem.Allocator,
    owner: []const u8,
};

fn lineOf(tree: Ast, node: Ast.Node.Index) usize {
    return tree.tokenLocation(0, tree.firstToken(node)).line + 1;
}

fn unsupportedNode(walker: Walker, node: Ast.Node.Index, raw: u32, line: usize) !void {
    std.debug.print("expression {s} {d} kind=unsupported line={d} text={s}\n", .{
        walker.owner,
        raw,
        line,
        try collapse(walker.arena, walker.tree.getNodeSource(node)),
    });
}

/// Prints one expression and everything under it, children first, so a reader
/// building a tree from these lines always has a child before its parent.
/// Nodes are named by their index in the syntax tree, which is unique per file
/// and needs no numbering of its own.
///
/// Anything with no shape here is reported as unsupported with the Zig it
/// stands for, which is what keeps a body from being half ported.
fn emitExpression(walker: Walker, node: Ast.Node.Index) anyerror!void {
    const tree = walker.tree;
    const tag = tree.nodeTag(node);
    const raw = @intFromEnum(node);
    const line = lineOf(tree, node);

    if (binaryOperator(tag)) |operator| {
        const sides = tree.nodeData(node).node_and_node;
        try emitExpression(walker, sides[0]);
        try emitExpression(walker, sides[1]);
        std.debug.print("expression {s} {d} kind=binary line={d} left={d} right={d} operator={s}\n", .{
            walker.owner, raw, line, @intFromEnum(sides[0]), @intFromEnum(sides[1]), operator,
        });
        return;
    }
    if (overflowOperator(tag)) |method| {
        const sides = tree.nodeData(node).node_and_node;
        try emitExpression(walker, sides[0]);
        try emitExpression(walker, sides[1]);
        std.debug.print("expression {s} {d} kind=method line={d} left={d} arguments=1 text={s}\n", .{
            walker.owner, raw, line, @intFromEnum(sides[0]), method,
        });
        std.debug.print("operand {s} {d} 0 node={d}\n", .{
            walker.owner, raw, @intFromEnum(sides[1]),
        });
        return;
    }
    if (unaryOperator(tag)) |operator| {
        const inner = tree.nodeData(node).node;
        try emitExpression(walker, inner);
        std.debug.print("expression {s} {d} kind=unary line={d} left={d} operator={s}\n", .{
            walker.owner, raw, line, @intFromEnum(inner), operator,
        });
        return;
    }

    switch (tag) {
        .identifier => {
            std.debug.print("expression {s} {d} kind=identifier line={d} text={s}\n", .{
                walker.owner, raw, line, tree.tokenSlice(tree.nodeMainToken(node)),
            });
            return;
        },
        .number_literal, .string_literal, .char_literal => {
            std.debug.print("expression {s} {d} kind=literal line={d} text={s}\n", .{
                walker.owner, raw, line, try collapse(walker.arena, tree.getNodeSource(node)),
            });
            return;
        },
        .field_access => {
            const base, const name = tree.nodeData(node).node_and_token;
            try emitExpression(walker, base);
            std.debug.print("expression {s} {d} kind=field line={d} left={d} text={s}\n", .{
                walker.owner, raw, line, @intFromEnum(base), tree.tokenSlice(name),
            });
            return;
        },
        .array_access => {
            const sides = tree.nodeData(node).node_and_node;
            try emitExpression(walker, sides[0]);
            try emitExpression(walker, sides[1]);
            std.debug.print("expression {s} {d} kind=index line={d} left={d} right={d}\n", .{
                walker.owner, raw, line, @intFromEnum(sides[0]), @intFromEnum(sides[1]),
            });
            return;
        },
        .grouped_expression => {
            const inner = tree.nodeData(node).node_and_token[0];
            try emitExpression(walker, inner);
            std.debug.print("expression {s} {d} kind=group line={d} left={d}\n", .{
                walker.owner, raw, line, @intFromEnum(inner),
            });
            return;
        },
        .@"try" => {
            const inner = tree.nodeData(node).node;
            try emitExpression(walker, inner);
            std.debug.print("expression {s} {d} kind=try line={d} left={d}\n", .{
                walker.owner, raw, line, @intFromEnum(inner),
            });
            return;
        },
        else => {},
    }

    // A struct literal in a body is the same shape as one in an initialiser,
    // and both are how a Zig function returns a value it just built. The field
    // values are emitted first so the reader has them before the literal that
    // names them.
    var init_buffer: [2]Ast.Node.Index = undefined;
    if (isStructInit(tree.nodeTag(node))) {
        if (tree.fullStructInit(&init_buffer, node)) |initializer| {
            for (initializer.ast.fields) |field| try emitExpression(walker, field);
            std.debug.print("expression {s} {d} kind=structliteral line={d} fields={d}\n", .{
                walker.owner, raw, line, initializer.ast.fields.len,
            });
            for (initializer.ast.fields, 0..) |field, index| {
                const value_first = tree.firstToken(field);
                if (value_first < 2) continue;
                std.debug.print("initfield {s} {d} {d} node={d} name={s}\n", .{
                    walker.owner,
                    raw,
                    index,
                    @intFromEnum(field),
                    tree.tokenSlice(value_first - 2),
                });
            }
            return;
        }
    }

    var call_buffer: [1]Ast.Node.Index = undefined;
    if (tree.fullCall(&call_buffer, node)) |call| {
        for (call.ast.params) |parameter| try emitExpression(walker, parameter);
        // The callee goes last, because it is Zig that may contain a space and
        // the reader takes the rest of the line for it.
        std.debug.print("expression {s} {d} kind=call line={d} arguments={d} text={s}\n", .{
            walker.owner,
            raw,
            line,
            call.ast.params.len,
            try collapse(walker.arena, tree.getNodeSource(call.ast.fn_expr)),
        });
        for (call.ast.params, 0..) |parameter, index| {
            std.debug.print("operand {s} {d} {d} node={d}\n", .{
                walker.owner, raw, index, @intFromEnum(parameter),
            });
        }
        return;
    }

    if (tree.fullIf(node)) |branch| {
        // A Zig `if` is an expression and so is a Rust one, so the shape
        // carries across whether it was written as a value or as a statement.
        // The arms go through the statement reader, because `if (x) return y`
        // puts a statement where a value would otherwise be.
        try emitExpression(walker, branch.ast.cond_expr);
        try emitStatement(walker, branch.ast.then_expr);
        const otherwise = branch.ast.else_expr.unwrap();
        if (otherwise) |expression| try emitStatement(walker, expression);
        std.debug.print("expression {s} {d} kind=if line={d} left={d} right={d} otherwise={s}\n", .{
            walker.owner,
            raw,
            line,
            @intFromEnum(branch.ast.cond_expr),
            @intFromEnum(branch.ast.then_expr),
            if (otherwise) |expression|
                try std.fmt.allocPrint(walker.arena, "{d}", .{@intFromEnum(expression)})
            else
                "-",
        });
        return;
    }

    var builtin_buffer: [2]Ast.Node.Index = undefined;
    if (isBuiltinCall(tag)) {
        const parameters = tree.builtinCallParams(&builtin_buffer, node) orelse &.{};
        for (parameters) |parameter| try emitExpression(walker, parameter);
        std.debug.print("expression {s} {d} kind=builtin line={d} arguments={d} text={s}\n", .{
            walker.owner,
            raw,
            line,
            parameters.len,
            tree.tokenSlice(tree.nodeMainToken(node)),
        });
        for (parameters, 0..) |parameter, index| {
            std.debug.print("operand {s} {d} {d} node={d}\n", .{
                walker.owner, raw, index, @intFromEnum(parameter),
            });
        }
        return;
    }

    if (tree.fullWhile(node)) |loop| {
        // A capture turns the condition into a pattern, which is a different
        // shape from a plain loop, so only the plain one is reported.
        if (loop.payload_token == null and loop.ast.cont_expr == .none and
            loop.ast.else_expr == .none)
        {
            try emitExpression(walker, loop.ast.cond_expr);
            try emitExpression(walker, loop.ast.then_expr);
            std.debug.print("expression {s} {d} kind=while line={d} left={d} right={d}\n", .{
                walker.owner,
                raw,
                line,
                @intFromEnum(loop.ast.cond_expr),
                @intFromEnum(loop.ast.then_expr),
            });
            return;
        }
    }

    if (tree.fullFor(node)) |loop| {
        // One thing to walk and one name to bind it to. Anything else, an
        // index alongside it or an else arm, is a different shape.
        if (loop.ast.inputs.len == 1 and loop.ast.else_expr == .none) {
            try emitExpression(walker, loop.ast.inputs[0]);
            try emitExpression(walker, loop.ast.then_expr);
            std.debug.print("expression {s} {d} kind=for line={d} left={d} right={d} text={s}\n", .{
                walker.owner,
                raw,
                line,
                @intFromEnum(loop.ast.inputs[0]),
                @intFromEnum(loop.ast.then_expr),
                tree.tokenSlice(loop.payload_token),
            });
            return;
        }
    }

    if (tree.fullSwitch(node)) |chosen| {
        // The patterns are reported as text rather than as expressions,
        // because what a bare `.red` means depends on the type being switched
        // on and only the reader on the other side knows that.
        var arms: usize = 0;
        for (chosen.ast.cases) |case| {
            const full_case = tree.fullSwitchCase(case) orelse continue;
            if (full_case.ast.values.len > 1) return unsupportedNode(walker, node, raw, line);
            try emitExpression(walker, full_case.ast.target_expr);
            arms += 1;
        }
        try emitExpression(walker, chosen.ast.condition);
        std.debug.print("expression {s} {d} kind=switch line={d} left={d} arms={d}\n", .{
            walker.owner, raw, line, @intFromEnum(chosen.ast.condition), arms,
        });
        for (chosen.ast.cases, 0..) |case, index| {
            const full_case = tree.fullSwitchCase(case) orelse continue;
            const pattern = if (full_case.ast.values.len == 0)
                "else"
            else
                try collapse(walker.arena, tree.getNodeSource(full_case.ast.values[0]));
            const capture = if (full_case.payload_token) |token|
                tree.tokenSlice(token)
            else
                "-";
            std.debug.print("arm {s} {d} {d} node={d} capture={s} pattern={s}\n", .{
                walker.owner,
                raw,
                index,
                @intFromEnum(full_case.ast.target_expr),
                capture,
                pattern,
            });
        }
        return;
    }

    var block_buffer: [2]Ast.Node.Index = undefined;
    if (tree.blockStatements(&block_buffer, node)) |statements| {
        for (statements) |statement| try emitStatement(walker, statement);
        std.debug.print("expression {s} {d} kind=block line={d} statements={d}\n", .{
            walker.owner, raw, line, statements.len,
        });
        for (statements, 0..) |statement, index| {
            std.debug.print("operand {s} {d} {d} node={d}\n", .{
                walker.owner, raw, index, @intFromEnum(statement),
            });
        }
        return;
    }

    std.debug.print("expression {s} {d} kind=unsupported line={d} text={s}\n", .{
        walker.owner, raw, line, try collapse(walker.arena, tree.getNodeSource(node)),
    });
}

/// One statement of a body. A statement is an expression here where Zig makes
/// no distinction, which keeps the reader on the other side from needing two
/// vocabularies for the same thing.
fn emitStatement(walker: Walker, node: Ast.Node.Index) anyerror!void {
    const tree = walker.tree;
    const raw = @intFromEnum(node);
    const line = lineOf(tree, node);

    if (tree.nodeTag(node) == .@"return") {
        const returned = tree.nodeData(node).opt_node.unwrap();
        if (returned) |expression| {
            try emitExpression(walker, expression);
            std.debug.print("statement {s} {d} kind=return line={d} left={d}\n", .{
                walker.owner, raw, line, @intFromEnum(expression),
            });
        } else {
            std.debug.print("statement {s} {d} kind=return line={d} left=-\n", .{
                walker.owner, raw, line,
            });
        }
        return;
    }

    if (tree.fullVarDecl(node)) |declaration| {
        const initialiser = declaration.ast.init_node.unwrap();
        if (initialiser) |expression| {
            try emitExpression(walker, expression);
            // Zig says `var` where Rust says `let mut`, and a `const` that the
            // port made mutable would be a different program.
            const mutable = std.mem.eql(u8, tree.tokenSlice(declaration.ast.mut_token), "var");
            std.debug.print("statement {s} {d} kind=let line={d} left={d} mutable={d} text={s}\n", .{
                walker.owner,
                raw,
                line,
                @intFromEnum(expression),
                @intFromBool(mutable),
                tree.tokenSlice(declaration.ast.mut_token + 1),
            });
            return;
        }
    }

    if (tree.nodeTag(node) == .assign) {
        const sides = tree.nodeData(node).node_and_node;
        try emitExpression(walker, sides[0]);
        try emitExpression(walker, sides[1]);
        std.debug.print("statement {s} {d} kind=assign line={d} left={d} right={d}\n", .{
            walker.owner, raw, line, @intFromEnum(sides[0]), @intFromEnum(sides[1]),
        });
        return;
    }

    // Anything else is an expression standing where a statement was written,
    // and it keeps its own node rather than gaining a wrapper that would
    // report the same thing twice.
    try emitExpression(walker, node);
}

fn emitBody(walker: Walker, body: Ast.Node.Index) !void {
    var block_buffer: [2]Ast.Node.Index = undefined;
    const statements = walker.tree.blockStatements(&block_buffer, body) orelse return;
    for (statements) |statement| try emitStatement(walker, statement);
    std.debug.print("body {s} statements={d}\n", .{ walker.owner, statements.len });
    for (statements, 0..) |statement, index| {
        std.debug.print("step {s} {d} node={d}\n", .{
            walker.owner, index, @intFromEnum(statement),
        });
    }
}

pub fn main() !void {
    var arena_state = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    var arguments = try std.process.argsWithAllocator(arena);
    _ = arguments.next();
    const path = arguments.next() orelse {
        std.debug.print("usage: extract <file.zig>\n", .{});
        std.process.exit(2);
    };

    const source = try std.fs.cwd().readFileAllocOptions(arena, path, 1 << 22, null, .of(u8), 0);
    var tree = try Ast.parse(arena, source, .zig);
    defer tree.deinit(arena);
    if (tree.errors.len != 0) {
        std.debug.print("parse failed with {d} errors\n", .{tree.errors.len});
        std.process.exit(1);
    }

    var containers: std.ArrayList(Span) = .empty;
    var functions: std.ArrayList(Span) = .empty;

    for (0..tree.nodes.len) |raw| {
        const node: Ast.Node.Index = @enumFromInt(raw);
        if (!isContainer(tree.nodeTag(node))) continue;
        const name = containerName(tree, node) orelse continue;
        try containers.append(arena, .{
            .name = name,
            .first = tree.firstToken(node),
            .last = tree.lastToken(node),
            .owner = null,
            .node = node,
        });
    }

    var buffer: [1]Ast.Node.Index = undefined;
    for (0..tree.nodes.len) |raw| {
        const node: Ast.Node.Index = @enumFromInt(raw);
        if (tree.nodeTag(node) != .fn_decl) continue;
        const proto = tree.fullFnProto(&buffer, node) orelse continue;
        const name_token = proto.name_token orelse continue;
        const holder = innermost(containers.items, tree.firstToken(node));
        // Qualified with the container it was declared in, because a file may
        // hold several structs with a method of the same name and every row
        // below names the function it belongs to. Zig forbids two declarations
        // of one name in one container, so this is unique per file.
        const bare = tree.tokenSlice(name_token);
        const named = if (holder) |index|
            try std.fmt.allocPrint(arena, "{s}.{s}", .{ containers.items[index].name, bare })
        else
            bare;
        try functions.append(arena, .{
            .name = named,
            .first = tree.firstToken(node),
            .last = tree.lastToken(node),
            .owner = holder,
            .node = node,
        });
    }

    for (containers.items) |container| {
        std.debug.print("container {s} kind={s}\n", .{
            container.name,
            containerKeyword(tree, container.node),
        });
    }

    // An error set is not a container declaration, so its names are read off
    // the tokens between its braces.
    for (0..tree.nodes.len) |raw| {
        const node: Ast.Node.Index = @enumFromInt(raw);
        if (tree.nodeTag(node) != .error_set_decl) continue;
        const name = containerName(tree, node) orelse continue;
        std.debug.print("container {s} kind=error\n", .{name});
        var walk = tree.firstToken(node);
        const last = tree.lastToken(node);
        while (walk <= last) : (walk += 1) {
            if (tree.tokenTag(walk) != .identifier) continue;
            std.debug.print("member {s}.{s} type=-\n", .{ name, tree.tokenSlice(walk) });
        }
    }

    for (0..tree.nodes.len) |raw| {
        const node: Ast.Node.Index = @enumFromInt(raw);
        const field = tree.fullContainerField(node) orelse continue;
        const owner = innermost(containers.items, tree.firstToken(node)) orelse continue;
        const kind = if (field.ast.type_expr.unwrap()) |expression|
            try collapse(arena, tree.getNodeSource(expression))
        else
            "-";
        std.debug.print("member {s}.{s} type={s}\n", .{
            containers.items[owner].name,
            tree.tokenSlice(field.ast.main_token),
            kind,
        });
    }

    for (functions.items) |function| {
        const owner = if (function.owner) |index| containers.items[index].name else "-";
        const proto = tree.fullFnProto(&buffer, function.node).?;
        // A named error set is part of the return type expression, but the `!`
        // of an inferred one is a token in front of it, so it has to be put
        // back or a fallible function reads as one that cannot fail.
        const returns = if (proto.ast.return_type.unwrap()) |expression| blk: {
            const text = try collapse(arena, tree.getNodeSource(expression));
            const first = tree.firstToken(expression);
            if (first > 0 and tree.tokenTag(first - 1) == .bang) {
                break :blk try std.fmt.allocPrint(arena, "!{s}", .{text});
            }
            break :blk text;
        } else "-";
        // The line goes ahead of `returns`, because the reader takes the whole
        // rest of the line for a return type that may contain spaces.
        std.debug.print("function {s} owner={s} line={d} returns={s}\n", .{
            function.name,
            owner,
            tree.tokenLocation(0, function.first).line + 1,
            returns,
        });
        var iterator = proto.iterate(&tree);
        var index: usize = 0;
        while (iterator.next()) |parameter| : (index += 1) {
            const parameter_name = if (parameter.name_token) |token| tree.tokenSlice(token) else "-";
            const kind = if (parameter.type_expr) |expression|
                try collapse(arena, tree.getNodeSource(expression))
            else
                "anytype";
            std.debug.print("parameter {s}.{d} name={s} type={s}\n", .{
                function.name,
                index,
                parameter_name,
                kind,
            });
        }
        try emitBody(
            .{ .tree = tree, .arena = arena, .owner = function.name },
            tree.nodeData(function.node).node_and_node[1],
        );
    }

    var call_buffer: [1]Ast.Node.Index = undefined;
    for (0..tree.nodes.len) |raw| {
        const node: Ast.Node.Index = @enumFromInt(raw);
        const call = tree.fullCall(&call_buffer, node) orelse continue;
        const owner = innermost(functions.items, tree.firstToken(node)) orelse continue;
        const callee = try collapse(arena, tree.getNodeSource(call.ast.fn_expr));
        std.debug.print("call {s} callee={s} arguments={d}\n", .{
            functions.items[owner].name,
            callee,
            call.ast.params.len,
        });
        for (call.ast.params, 0..) |parameter, index| {
            std.debug.print("argument {s}|{s}|{d} text={s}\n", .{
                functions.items[owner].name,
                callee,
                index,
                try collapse(arena, tree.getNodeSource(parameter)),
            });
        }
    }

    // What this file pulls in. The caller follows these to find the rest of the
    // program, and uses the name each one is bound to so that `buffer.Buffer`
    // can be read as a type in the buffer module.
    for (0..tree.nodes.len) |raw| {
        const node: Ast.Node.Index = @enumFromInt(raw);
        const declaration = tree.fullVarDecl(node) orelse continue;
        if (innermost(functions.items, tree.firstToken(node)) != null) continue;
        const initializer = declaration.ast.init_node.unwrap() orelse continue;
        const text = try collapse(arena, tree.getNodeSource(initializer));
        const opened = std.mem.indexOf(u8, text, "@import(\"") orelse continue;
        const rest = text[opened + "@import(\"".len ..];
        const closed = std.mem.indexOfScalar(u8, rest, '"') orelse continue;
        std.debug.print("import {s} path={s}\n", .{
            tree.tokenSlice(declaration.ast.mut_token + 1),
            rest[0..closed],
        });
    }

    for (0..tree.nodes.len) |raw| {
        const node: Ast.Node.Index = @enumFromInt(raw);
        const declaration = tree.fullVarDecl(node) orelse continue;
        const owner = innermost(functions.items, tree.firstToken(node)) orelse continue;
        const initializer = declaration.ast.init_node.unwrap() orelse continue;
        std.debug.print("local {s}.{s} value={s}\n", .{
            functions.items[owner].name,
            tree.tokenSlice(declaration.ast.mut_token + 1),
            try collapse(arena, tree.getNodeSource(initializer)),
        });
    }

    var init_buffer: [2]Ast.Node.Index = undefined;
    for (0..tree.nodes.len) |raw| {
        const node: Ast.Node.Index = @enumFromInt(raw);
        var nested_buffer: [2]Ast.Node.Index = undefined;
        const initializer = tree.fullStructInit(&init_buffer, node) orelse continue;
        const owner = innermost(functions.items, tree.firstToken(node)) orelse continue;
        // The parent is the innermost struct literal that contains this one,
        // which is how a nested literal is attributed to the field it fills
        // rather than to the function's return type.
        var parent: ?usize = null;
        for (0..tree.nodes.len) |candidate_raw| {
            const candidate: Ast.Node.Index = @enumFromInt(candidate_raw);
            if (candidate == node) continue;
            if (!isStructInit(tree.nodeTag(candidate))) continue;
            if (tree.fullStructInit(&nested_buffer, candidate) == null) continue;
            const first = tree.firstToken(candidate);
            const last = tree.lastToken(candidate);
            if (tree.firstToken(node) < first or tree.lastToken(node) > last) continue;
            if (parent) |current| {
                const width = tree.lastToken(@enumFromInt(current)) - tree.firstToken(@enumFromInt(current));
                if (last - first >= width) continue;
            }
            parent = candidate_raw;
        }
        const parent_text = if (parent) |index|
            try std.fmt.allocPrint(arena, "{d}", .{index})
        else
            "-";
        for (initializer.ast.fields) |field| {
            const value_first = tree.firstToken(field);
            if (value_first < 2) continue;
            std.debug.print("initialiser {s} node={d} parent={s} field={s} line={d} value={s}\n", .{
                functions.items[owner].name,
                raw,
                parent_text,
                tree.tokenSlice(value_first - 2),
                tree.tokenLocation(0, value_first).line + 1,
                try collapse(arena, tree.getNodeSource(field)),
            });
        }
    }
}
