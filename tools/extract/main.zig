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
        try functions.append(arena, .{
            .name = tree.tokenSlice(name_token),
            .first = tree.firstToken(node),
            .last = tree.lastToken(node),
            .owner = innermost(containers.items, tree.firstToken(node)),
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
        std.debug.print("function {s} owner={s} returns={s}\n", .{ function.name, owner, returns });
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
            std.debug.print("initialiser {s} node={d} parent={s} field={s} value={s}\n", .{
                functions.items[owner].name,
                raw,
                parent_text,
                tree.tokenSlice(value_first - 2),
                try collapse(arena, tree.getNodeSource(field)),
            });
        }
    }
}
