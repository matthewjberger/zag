use zag_facts::tables::Strings;
use zag_facts::{NO_INDEX, StringId};

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NodeId(pub u32);

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeKind {
    File = 0,
    Struct = 1,
    Field = 2,
    TypePath = 3,
    TypeSliceBody = 4,
    TypeBoxed = 5,
    TypeReference = 6,
    TypeOptionNonNull = 7,
    AssertSize = 8,
    AssertAlignment = 9,
    AssertOffset = 10,
    Enum = 11,
    Variant = 12,
    Implementation = 13,
    Function = 14,
    Parameter = 15,
    ExpressionStruct = 16,
    FieldValue = 17,
    ExpressionLiteral = 18,
    ExpressionPath = 19,
    ExpressionCall = 20,
    ExpressionTry = 21,
    TypeOption = 22,
    TypeArray = 23,
    TypeResult = 24,
    Discard = 25,
}

#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lifetime {
    Borrow = 0,
    Static = 1,
    Arena = 2,
    /// A borrow in a function signature, where Rust supplies the lifetime.
    Elided = 3,
}

pub const STRUCT_FLAG_REPR_C: u32 = 1 << 0;
pub const STRUCT_FLAG_BORROW_LIFETIME: u32 = 1 << 1;
pub const STRUCT_FLAG_ARENA_LIFETIME: u32 = 1 << 2;

/// The parameter is a receiver. Its text is the whole thing Rust writes, so it
/// carries no type node of its own.
pub const PARAMETER_FLAG_RECEIVER: u32 = 1 << 0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ast {
    pub kind: Vec<NodeKind>,
    pub text: Vec<StringId>,
    pub secondary_text: Vec<StringId>,
    pub number: Vec<u32>,
    pub flags: Vec<u32>,
    pub child_start: Vec<u32>,
    pub child_count: Vec<u32>,
    pub children: Vec<NodeId>,
    pub strings: Strings,
    pub root: NodeId,
}

pub fn empty_ast() -> Ast {
    Ast {
        kind: Vec::new(),
        text: Vec::new(),
        secondary_text: Vec::new(),
        number: Vec::new(),
        flags: Vec::new(),
        child_start: Vec::new(),
        child_count: Vec::new(),
        children: Vec::new(),
        strings: Strings::default(),
        root: NodeId(NO_INDEX),
    }
}

pub fn push_node(
    ast: &mut Ast,
    kind: NodeKind,
    text: StringId,
    secondary_text: StringId,
    number: u32,
    flags: u32,
    children: &[NodeId],
) -> NodeId {
    let child_start = ast.children.len() as u32;
    ast.children.extend_from_slice(children);
    ast.kind.push(kind);
    ast.text.push(text);
    ast.secondary_text.push(secondary_text);
    ast.number.push(number);
    ast.flags.push(flags);
    ast.child_start.push(child_start);
    ast.child_count.push(children.len() as u32);
    NodeId(ast.kind.len() as u32 - 1)
}

pub fn node_children(ast: &Ast, node: NodeId) -> std::ops::Range<usize> {
    let index = node.0 as usize;
    if index >= ast.child_start.len() {
        return 0..0;
    }
    let start = ast.child_start[index] as usize;
    let count = ast.child_count[index] as usize;
    start..start + count
}

pub fn node_count(ast: &Ast) -> usize {
    ast.kind.len()
}
