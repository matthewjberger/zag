//! Reads the dataflow out of a Zig file with the compiler's own parser, so the
//! facts about which function calls which, which call allocates, and what a
//! struct literal puts in each field come from the source rather than from a
//! person reading it.
//!
//! This is syntax, not semantics. It recognises `x.dupe(...)` as an allocation
//! because of how it is spelled, not because it resolved `x` to an allocator.
//! Comptime, generics, and anything that needs a type to decide are out of
//! reach here and are what the Sema frontend is for.
//!
//! Nodes are visited by scanning the whole array and attributing each one to
//! the innermost declaration whose token range covers it, which avoids
//! traversing every expression shape the language has.

const std = @import("std");
const Ast = std.zig.Ast;

const Span = struct {
    name: []const u8,
    first: Ast.TokenIndex,
    last: Ast.TokenIndex,
    owner: ?usize,
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

fn structName(tree: Ast, container: Ast.Node.Index) ?[]const u8 {
    // A struct is named by the variable declaration it initialises, and the
    // declaration's identifier sits two tokens before the container keyword.
    const first = tree.firstToken(container);
    if (first < 2) return null;
    var walk = first;
    while (walk > 0) {
        walk -= 1;
        if (tree.tokenTag(walk) == .identifier) {
            if (walk > 0 and (tree.tokenTag(walk - 1) == .keyword_const or
                tree.tokenTag(walk - 1) == .keyword_var))
            {
                return tree.tokenSlice(walk);
            }
        }
        if (tree.tokenTag(walk) == .semicolon) return null;
    }
    return null;
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
        switch (tree.nodeTag(node)) {
            .container_decl,
            .container_decl_trailing,
            .container_decl_two,
            .container_decl_two_trailing,
            .container_decl_arg,
            .container_decl_arg_trailing,
            => {
                const name = structName(tree, node) orelse continue;
                try containers.append(arena, .{
                    .name = name,
                    .first = tree.firstToken(node),
                    .last = tree.lastToken(node),
                    .owner = null,
                });
            },
            else => {},
        }
    }

    var buffer: [1]Ast.Node.Index = undefined;
    for (0..tree.nodes.len) |raw| {
        const node: Ast.Node.Index = @enumFromInt(raw);
        if (tree.nodeTag(node) != .fn_decl) continue;
        const proto = tree.fullFnProto(&buffer, node) orelse continue;
        const name_token = proto.name_token orelse continue;
        const first = tree.firstToken(node);
        try functions.append(arena, .{
            .name = tree.tokenSlice(name_token),
            .first = first,
            .last = tree.lastToken(node),
            .owner = innermost(containers.items, first),
        });
    }

    for (functions.items) |function| {
        const owner = if (function.owner) |index| containers.items[index].name else "-";
        std.debug.print("function {s} owner={s}\n", .{ function.name, owner });
    }

    for (0..tree.nodes.len) |raw| {
        const node: Ast.Node.Index = @enumFromInt(raw);
        const proto_node: Ast.Node.Index = node;
        if (tree.nodeTag(node) != .fn_decl) continue;
        const proto = tree.fullFnProto(&buffer, proto_node) orelse continue;
        const name_token = proto.name_token orelse continue;
        const name = tree.tokenSlice(name_token);
        var iterator = proto.iterate(&tree);
        var index: usize = 0;
        while (iterator.next()) |parameter| : (index += 1) {
            const parameter_name = if (parameter.name_token) |token| tree.tokenSlice(token) else "-";
            const kind = if (parameter.type_expr) |kind|
                try collapse(arena, tree.getNodeSource(kind))
            else
                "anytype";
            std.debug.print("parameter {s}.{d} name={s} type={s}\n", .{
                name,
                index,
                parameter_name,
                kind,
            });
        }
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
            const text = try collapse(arena, tree.getNodeSource(parameter));
            std.debug.print("argument {s}|{s}|{d} text={s}\n", .{
                functions.items[owner].name,
                callee,
                index,
                text,
            });
        }
    }

    var init_buffer: [2]Ast.Node.Index = undefined;
    for (0..tree.nodes.len) |raw| {
        const node: Ast.Node.Index = @enumFromInt(raw);
        const initializer = tree.fullStructInit(&init_buffer, node) orelse continue;
        const owner = innermost(functions.items, tree.firstToken(node)) orelse continue;
        for (initializer.ast.fields) |field| {
            const value_first = tree.firstToken(field);
            if (value_first < 2) continue;
            const field_name = tree.tokenSlice(value_first - 2);
            const text = try collapse(arena, tree.getNodeSource(field));
            std.debug.print("initialiser {s} field={s} value={s}\n", .{
                functions.items[owner].name,
                field_name,
                text,
            });
        }
    }
}
