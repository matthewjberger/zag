//! Reading the two tool outputs into the shape the table builder walks.
//!
//! An argument row names the function and the callee rather than the call, so
//! anything that calls one thing twice is where this goes wrong, and it goes
//! wrong quietly: the arguments pile onto the first call and the second looks
//! like it was written with none.

use zag_frontend::program::parse;

fn extraction(text: &str) -> zag_frontend::program::Program {
    parse(text, "")
}

/// The shape every Zig struct with more than one owned field has. Losing the
/// second free costs that field its ownership, which is silent.
#[test]
fn two_calls_to_one_callee_keep_an_argument_each() {
    let program = extraction(
        "function deinit owner=Buffer line=1 returns=void\n\
         call deinit callee=allocator.free arguments=1\n\
         argument deinit|allocator.free|0 text=self.head\n\
         call deinit callee=allocator.free arguments=1\n\
         argument deinit|allocator.free|0 text=self.tail\n",
    );
    let calls = &program.functions[0].calls;
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].arguments, vec!["self.head".to_string()]);
    assert_eq!(calls[1].arguments, vec!["self.tail".to_string()]);
}

#[test]
fn a_call_taking_several_arguments_keeps_them_in_order() {
    let program = extraction(
        "function init owner=Buffer line=1 returns=void\n\
         call init callee=allocator.dupe arguments=2\n\
         argument init|allocator.dupe|0 text=u8\n\
         argument init|allocator.dupe|1 text=bytes\n",
    );
    assert_eq!(
        program.functions[0].calls[0].arguments,
        vec!["u8".to_string(), "bytes".to_string()]
    );
}

/// The arity is what stops the fill, so a call reported as taking none takes
/// none rather than absorbing whatever the next call was handed.
#[test]
fn a_call_reported_with_no_arguments_takes_none() {
    let program = extraction(
        "function build owner=- line=1 returns=void\n\
         call build callee=b.getInstallStep arguments=0\n\
         call build callee=b.getInstallStep arguments=1\n\
         argument build|b.getInstallStep|0 text=step\n",
    );
    let calls = &program.functions[0].calls;
    assert!(calls[0].arguments.is_empty(), "{calls:?}");
    assert_eq!(calls[1].arguments, vec!["step".to_string()]);
}

/// A corrupt arity is a number from a file, so it decides nothing about how
/// much the reader will hold.
#[test]
fn an_absurd_arity_does_not_make_the_reader_hold_a_row_per_number() {
    let program = extraction(
        "function wide owner=- line=1 returns=void\n\
         call wide callee=thing arguments=4000000000\n\
         argument wide|thing|0 text=one\n",
    );
    assert_eq!(program.functions[0].calls[0].arguments, vec!["one"]);
}
