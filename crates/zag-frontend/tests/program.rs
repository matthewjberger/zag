//! Reading the two tool outputs into the shape the table builder walks.
//!
//! Every row about a call names the call by its number inside the function that
//! made it. Keying on the callee instead is where this used to go wrong, and it
//! went wrong quietly: a function calling one thing twice piled both sets of
//! arguments onto the first call and left the second looking like it was
//! written with none.

use zag_frontend::program::parse;

fn extraction(text: &str) -> zag_frontend::program::Program {
    parse(text, "")
}

/// The shape every Zig struct with more than one owned field has. Losing the
/// second free costs that field its ownership, which is silent.
#[test]
fn two_calls_to_one_callee_keep_an_argument_each() {
    let program = extraction(
        "function Buffer.deinit owner=Buffer line=1 returns=void\n\
         call Buffer.deinit 0 arguments=1 callee=allocator.free\n\
         argument Buffer.deinit 0 0 text=self.head\n\
         call Buffer.deinit 1 arguments=1 callee=allocator.free\n\
         argument Buffer.deinit 1 0 text=self.tail\n",
    );
    let calls = &program.functions[0].calls;
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].arguments, vec!["self.head".to_string()]);
    assert_eq!(calls[1].arguments, vec!["self.tail".to_string()]);
}

#[test]
fn a_call_taking_several_arguments_keeps_them_in_order() {
    let program = extraction(
        "function Buffer.init owner=Buffer line=1 returns=void\n\
         call Buffer.init 0 arguments=2 callee=allocator.dupe\n\
         argument Buffer.init 0 0 text=u8\n\
         argument Buffer.init 0 1 text=bytes\n",
    );
    assert_eq!(
        program.functions[0].calls[0].arguments,
        vec!["u8".to_string(), "bytes".to_string()]
    );
}

/// A callee is Zig and may contain a space, which is why it is written last and
/// read to the end of the line.
#[test]
fn a_callee_containing_a_space_is_kept_whole() {
    let program = extraction(
        "function build owner=- line=1 returns=void\n\
         call build 0 arguments=1 callee=b.step(\"run\", \"Run it\").dependOn\n\
         argument build 0 0 text=&run.step\n",
    );
    assert_eq!(
        program.functions[0].calls[0].callee,
        "b.step(\"run\", \"Run it\").dependOn"
    );
}

/// Two functions may share a name, so a row names the container it was declared
/// in. Zig forbids two declarations of one name in one container, which is what
/// makes that unique.
#[test]
fn two_methods_of_one_name_keep_their_own_parameters() {
    let program = extraction(
        "function Sphere.hit owner=Sphere line=1 returns=?Hit\n\
         parameter Sphere.hit.0 name=self type=Sphere\n\
         function Plane.hit owner=Plane line=9 returns=?Hit\n\
         parameter Plane.hit.0 name=self type=Plane\n",
    );
    assert_eq!(program.functions.len(), 2);
    assert_eq!(program.functions[0].name, "hit");
    assert_eq!(program.functions[0].parameters.len(), 1);
    assert_eq!(program.functions[0].parameters[0].declared, "Sphere");
    assert_eq!(program.functions[1].parameters.len(), 1);
    assert_eq!(program.functions[1].parameters[0].declared, "Plane");
}

/// A corrupt number is a number from a file, so it decides nothing about how
/// much the reader will hold.
#[test]
fn an_argument_naming_a_call_that_is_not_there_is_dropped() {
    let program = extraction(
        "function wide owner=- line=1 returns=void\n\
         call wide 0 arguments=1 callee=thing\n\
         argument wide 4000000000 0 text=one\n\
         argument wide 0 0 text=two\n",
    );
    assert_eq!(program.functions[0].calls[0].arguments, vec!["two"]);
}

/// A struct literal in a body carries the field each value fills, which is what
/// lets the port write the struct rather than the text.
#[test]
fn a_struct_literal_keeps_a_field_per_value() {
    let program = extraction(
        "function Vector.add owner=Vector line=1 returns=Vector\n\
         expression Vector.add 31 kind=structliteral line=2 fields=2\n\
         initfield Vector.add 31 0 node=20 name=x\n\
         initfield Vector.add 31 1 node=25 name=y\n",
    );
    let node = &program.functions[0].nodes[0];
    assert_eq!(node.kind, "structliteral");
    assert_eq!(
        node.fields,
        vec![("x".to_string(), 20), ("y".to_string(), 25)]
    );
}
