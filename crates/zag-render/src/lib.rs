pub mod ast;

use ast::{
    Ast, Lifetime, NodeId, NodeKind, STRUCT_FLAG_ARENA_LIFETIME, STRUCT_FLAG_BORROW_LIFETIME,
    STRUCT_FLAG_REPR_C, node_children, node_count,
};
use zag_facts::tables::string_bytes;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderError {
    MissingRoot,
    NodeOutOfRange { node: NodeId },
    WrongKind { node: NodeId, found: NodeKind },
    MissingChild { node: NodeId },
}

pub fn render(ast: &Ast) -> Result<Vec<u8>, RenderError> {
    let root = ast.root;
    if root.0 as usize >= node_count(ast) {
        return Err(RenderError::MissingRoot);
    }
    if ast.kind[root.0 as usize] != NodeKind::File {
        return Err(RenderError::WrongKind {
            node: root,
            found: ast.kind[root.0 as usize],
        });
    }
    let mut out = Vec::new();
    let mut previous: Option<NodeKind> = None;
    for slot in node_children(ast, root) {
        let item = ast.children[slot];
        let kind = kind_of(ast, item)?;
        if let Some(previous) = previous
            && !(is_assertion(previous) && is_assertion(kind))
        {
            out.push(b'\n');
        }
        render_item(&mut out, ast, item)?;
        previous = Some(kind);
    }
    Ok(out)
}

fn is_assertion(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::AssertSize | NodeKind::AssertAlignment | NodeKind::AssertOffset
    )
}

fn kind_of(ast: &Ast, node: NodeId) -> Result<NodeKind, RenderError> {
    let index = node.0 as usize;
    if index >= node_count(ast) {
        return Err(RenderError::NodeOutOfRange { node });
    }
    Ok(ast.kind[index])
}

fn text_of(ast: &Ast, node: NodeId) -> &[u8] {
    string_bytes(&ast.strings, ast.text[node.0 as usize])
}

fn secondary_text_of(ast: &Ast, node: NodeId) -> &[u8] {
    string_bytes(&ast.strings, ast.secondary_text[node.0 as usize])
}

fn only_child(ast: &Ast, node: NodeId) -> Result<NodeId, RenderError> {
    let range = node_children(ast, node);
    if range.len() != 1 {
        return Err(RenderError::MissingChild { node });
    }
    Ok(ast.children[range.start])
}

fn render_item(out: &mut Vec<u8>, ast: &Ast, node: NodeId) -> Result<(), RenderError> {
    match kind_of(ast, node)? {
        NodeKind::Struct => render_struct(out, ast, node),
        NodeKind::Enum => render_enum(out, ast, node),
        NodeKind::Implementation => render_implementation(out, ast, node),
        NodeKind::AssertSize => render_assertion(out, ast, node, b"size_of"),
        NodeKind::AssertAlignment => render_assertion(out, ast, node, b"align_of"),
        NodeKind::AssertOffset => render_offset_assertion(out, ast, node),
        found => Err(RenderError::WrongKind { node, found }),
    }
}

fn render_struct(out: &mut Vec<u8>, ast: &Ast, node: NodeId) -> Result<(), RenderError> {
    let flags = ast.flags[node.0 as usize];
    if flags & STRUCT_FLAG_REPR_C != 0 {
        out.extend_from_slice(b"#[repr(C)]\n");
    }
    out.extend_from_slice(b"pub struct ");
    out.extend_from_slice(text_of(ast, node));
    render_lifetime_parameters(out, flags);
    out.extend_from_slice(b" {\n");
    for slot in node_children(ast, node) {
        let field = ast.children[slot];
        match kind_of(ast, field)? {
            NodeKind::Field => {}
            found => return Err(RenderError::WrongKind { node: field, found }),
        }
        out.extend_from_slice(b"    pub ");
        out.extend_from_slice(text_of(ast, field));
        out.extend_from_slice(b": ");
        render_type(out, ast, only_child(ast, field)?)?;
        out.extend_from_slice(b",\n");
    }
    out.extend_from_slice(b"}\n");
    Ok(())
}

/// A Zig enum and a Zig tagged union are the same Rust shape. The difference
/// is whether a variant carries a payload, which is a child node here.
fn render_enum(out: &mut Vec<u8>, ast: &Ast, node: NodeId) -> Result<(), RenderError> {
    let flags = ast.flags[node.0 as usize];
    if flags & STRUCT_FLAG_REPR_C != 0 {
        out.extend_from_slice(b"#[repr(C)]\n");
    }
    out.extend_from_slice(b"pub enum ");
    out.extend_from_slice(text_of(ast, node));
    render_lifetime_parameters(out, flags);
    out.extend_from_slice(b" {\n");
    for slot in node_children(ast, node) {
        let variant = ast.children[slot];
        match kind_of(ast, variant)? {
            NodeKind::Variant => {}
            found => {
                return Err(RenderError::WrongKind {
                    node: variant,
                    found,
                });
            }
        }
        out.extend_from_slice(b"    ");
        out.extend_from_slice(text_of(ast, variant));
        let payload = node_children(ast, variant);
        if !payload.is_empty() {
            out.extend_from_slice(b"(");
            render_type(out, ast, ast.children[payload.start])?;
            out.extend_from_slice(b")");
        }
        out.extend_from_slice(b",\n");
    }
    out.extend_from_slice(b"}\n");
    Ok(())
}

fn render_lifetime_parameters(out: &mut Vec<u8>, flags: u32) {
    let borrow = flags & STRUCT_FLAG_BORROW_LIFETIME != 0;
    let arena = flags & STRUCT_FLAG_ARENA_LIFETIME != 0;
    if !borrow && !arena {
        return;
    }
    out.extend_from_slice(b"<");
    if borrow {
        out.extend_from_slice(b"'a");
    }
    if borrow && arena {
        out.extend_from_slice(b", ");
    }
    if arena {
        out.extend_from_slice(b"'bump");
    }
    out.extend_from_slice(b">");
}

fn render_type(out: &mut Vec<u8>, ast: &Ast, node: NodeId) -> Result<(), RenderError> {
    match kind_of(ast, node)? {
        NodeKind::TypePath => {
            out.extend_from_slice(text_of(ast, node));
            render_lifetime_parameters(out, ast.flags[node.0 as usize]);
            Ok(())
        }
        NodeKind::TypeSliceBody => {
            out.extend_from_slice(b"[");
            render_type(out, ast, only_child(ast, node)?)?;
            out.extend_from_slice(b"]");
            Ok(())
        }
        NodeKind::TypeBoxed => {
            out.extend_from_slice(b"Box<");
            render_type(out, ast, only_child(ast, node)?)?;
            out.extend_from_slice(b">");
            Ok(())
        }
        NodeKind::TypeReference => {
            out.extend_from_slice(b"&");
            let lifetime = lifetime_text(ast.flags[node.0 as usize]);
            if !lifetime.is_empty() {
                out.extend_from_slice(lifetime);
                out.extend_from_slice(b" ");
            }
            render_type(out, ast, only_child(ast, node)?)?;
            Ok(())
        }
        NodeKind::TypeOption => {
            out.extend_from_slice(b"Option<");
            render_type(out, ast, only_child(ast, node)?)?;
            out.extend_from_slice(b">");
            Ok(())
        }
        NodeKind::TypeArray => {
            out.extend_from_slice(b"[");
            render_type(out, ast, only_child(ast, node)?)?;
            out.extend_from_slice(b"; ");
            out.extend_from_slice(ast.number[node.0 as usize].to_string().as_bytes());
            out.extend_from_slice(b"]");
            Ok(())
        }
        NodeKind::TypeOptionNonNull => {
            out.extend_from_slice(b"Option<core::ptr::NonNull<");
            render_type(out, ast, only_child(ast, node)?)?;
            out.extend_from_slice(b">>");
            Ok(())
        }
        found => Err(RenderError::WrongKind { node, found }),
    }
}

fn indent(out: &mut Vec<u8>, depth: usize) {
    for _ in 0..depth {
        out.extend_from_slice(b"    ");
    }
}

fn render_implementation(out: &mut Vec<u8>, ast: &Ast, node: NodeId) -> Result<(), RenderError> {
    out.extend_from_slice(b"impl ");
    out.extend_from_slice(text_of(ast, node));
    out.extend_from_slice(b" {\n");
    for slot in node_children(ast, node) {
        render_function(out, ast, ast.children[slot])?;
    }
    out.extend_from_slice(b"}\n");
    Ok(())
}

fn render_function(out: &mut Vec<u8>, ast: &Ast, node: NodeId) -> Result<(), RenderError> {
    let children = node_children(ast, node);
    let parameters = ast.number[node.0 as usize] as usize;
    if children.len() != parameters + 2 {
        return Err(RenderError::MissingChild { node });
    }
    indent(out, 1);
    out.extend_from_slice(b"pub fn ");
    out.extend_from_slice(text_of(ast, node));
    out.extend_from_slice(b"(");
    for index in 0..parameters {
        if index != 0 {
            out.extend_from_slice(b", ");
        }
        let parameter = ast.children[children.start + index];
        out.extend_from_slice(text_of(ast, parameter));
        out.extend_from_slice(b": ");
        render_type(out, ast, only_child(ast, parameter)?)?;
    }
    out.extend_from_slice(b") -> ");
    render_type(out, ast, ast.children[children.start + parameters])?;
    out.extend_from_slice(b" {\n");
    indent(out, 2);
    render_expression(out, ast, ast.children[children.start + parameters + 1], 2)?;
    out.extend_from_slice(b"\n");
    indent(out, 1);
    out.extend_from_slice(b"}\n");
    Ok(())
}

fn render_expression(
    out: &mut Vec<u8>,
    ast: &Ast,
    node: NodeId,
    depth: usize,
) -> Result<(), RenderError> {
    match kind_of(ast, node)? {
        NodeKind::ExpressionLiteral | NodeKind::ExpressionPath => {
            out.extend_from_slice(text_of(ast, node));
            Ok(())
        }
        NodeKind::ExpressionCall => {
            out.extend_from_slice(text_of(ast, node));
            out.extend_from_slice(b"(");
            for (position, slot) in node_children(ast, node).enumerate() {
                if position != 0 {
                    out.extend_from_slice(b", ");
                }
                render_expression(out, ast, ast.children[slot], depth)?;
            }
            out.extend_from_slice(b")");
            Ok(())
        }
        NodeKind::ExpressionTry => {
            out.extend_from_slice(text_of(ast, node));
            out.extend_from_slice(b"::try_from(");
            render_expression(out, ast, only_child(ast, node)?, depth)?;
            out.extend_from_slice(b").unwrap()");
            Ok(())
        }
        NodeKind::ExpressionStruct => {
            out.extend_from_slice(text_of(ast, node));
            out.extend_from_slice(b" {\n");
            for slot in node_children(ast, node) {
                let field = ast.children[slot];
                indent(out, depth + 1);
                let value = only_child(ast, field)?;
                // Field init shorthand, which is what a person writes and what
                // the linter asks for when the names already match.
                let shorthand = kind_of(ast, value)? == NodeKind::ExpressionPath
                    && text_of(ast, value) == text_of(ast, field);
                out.extend_from_slice(text_of(ast, field));
                if !shorthand {
                    out.extend_from_slice(b": ");
                    render_expression(out, ast, value, depth + 1)?;
                }
                out.extend_from_slice(b",\n");
            }
            indent(out, depth);
            out.extend_from_slice(b"}");
            Ok(())
        }
        found => Err(RenderError::WrongKind { node, found }),
    }
}

fn lifetime_text(flags: u32) -> &'static [u8] {
    if flags == Lifetime::Static as u32 {
        b"'static"
    } else if flags == Lifetime::Arena as u32 {
        b"'bump"
    } else if flags == Lifetime::Elided as u32 {
        b""
    } else {
        b"'a"
    }
}

fn render_assertion(
    out: &mut Vec<u8>,
    ast: &Ast,
    node: NodeId,
    operation: &[u8],
) -> Result<(), RenderError> {
    out.extend_from_slice(b"const _: () = assert!(core::mem::");
    out.extend_from_slice(operation);
    out.extend_from_slice(b"::<");
    out.extend_from_slice(text_of(ast, node));
    out.extend_from_slice(b">() == ");
    out.extend_from_slice(ast.number[node.0 as usize].to_string().as_bytes());
    out.extend_from_slice(b");\n");
    Ok(())
}

fn render_offset_assertion(out: &mut Vec<u8>, ast: &Ast, node: NodeId) -> Result<(), RenderError> {
    out.extend_from_slice(b"const _: () = assert!(core::mem::offset_of!(");
    out.extend_from_slice(text_of(ast, node));
    out.extend_from_slice(b", ");
    out.extend_from_slice(secondary_text_of(ast, node));
    out.extend_from_slice(b") == ");
    out.extend_from_slice(ast.number[node.0 as usize].to_string().as_bytes());
    out.extend_from_slice(b");\n");
    Ok(())
}
