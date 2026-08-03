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
            out.extend_from_slice(lifetime_text(ast.flags[node.0 as usize]));
            out.extend_from_slice(b" ");
            render_type(out, ast, only_child(ast, node)?)?;
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

fn lifetime_text(flags: u32) -> &'static [u8] {
    if flags == Lifetime::Static as u32 {
        b"'static"
    } else if flags == Lifetime::Arena as u32 {
        b"'bump"
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
