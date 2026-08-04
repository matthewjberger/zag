//! Compiling this crate is the check. Every port below is checked in output,
//! so its layout assertions run in the build and a struct the emitter spelled
//! wrong fails here rather than in a program.
//!
//! Each port gets a module because two examples may name the same type.

pub mod coverage {
    include!("../../../examples/coverage/expected/port.rs");
}

pub mod conflict {
    include!("../../../examples/conflict/expected/port.rs");
}

pub mod ledger {
    include!("../../../examples/ledger/expected/port.rs");
}

pub mod netpacket {
    include!("../../../examples/netpacket/expected/port.rs");
}

pub mod shapes {
    include!("../../../examples/shapes/expected/port.rs");
}

pub mod telemetry {
    include!("../../../examples/telemetry/expected/port.rs");
}

pub mod tokenizer {
    include!("../../../examples/tokenizer/expected/port.rs");
}

pub mod wordcount {
    include!("../../../examples/wordcount/expected/port.rs");
}
