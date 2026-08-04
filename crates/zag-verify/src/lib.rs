//! Compiling this crate is the check. Every port below is checked in output,
//! so its layout assertions run in the build and a struct the emitter spelled
//! wrong fails here rather than in a program. Cargo does it, on every build of
//! this workspace, which is the same thing anyone keeping a port would run.
//!
//! Each port gets a module because two programs may name the same type.

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

pub mod corpus_config {
    include!("../../../corpus/config/expected/port.rs");
}

pub mod corpus_lifegrid {
    include!("../../../corpus/lifegrid/expected/port.rs");
}

pub mod corpus_machine {
    include!("../../../corpus/machine/expected/port.rs");
}
