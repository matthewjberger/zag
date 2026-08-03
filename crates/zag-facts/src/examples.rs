pub mod conflict;
pub mod ledger;
pub mod netpacket;
pub mod shapes;
pub mod telemetry;
pub mod tokenizer;
pub mod wordcount;

use crate::tables::Tables;

/// Every name here has a Zig project under `examples/<name>` that builds on
/// its own, and the tables below are what a frontend reading that project
/// would hand over. The two are kept in step by hand until the frontend lands.
pub const NAMES: [&str; 8] = [
    "conflict",
    "fixture",
    "ledger",
    "netpacket",
    "shapes",
    "telemetry",
    "tokenizer",
    "wordcount",
];

/// The coverage fixture is not a runnable project, so it has no directory
/// under `examples`. It exists to reach every ownership class in one program.
pub const SYNTHETIC: [&str; 1] = ["fixture"];

pub fn tables_for(name: &str) -> Option<Tables> {
    match name {
        "conflict" => Some(conflict::tables()),
        "fixture" => Some(crate::fixture::example_tables()),
        "ledger" => Some(ledger::tables()),
        "netpacket" => Some(netpacket::tables()),
        "shapes" => Some(shapes::tables()),
        "telemetry" => Some(telemetry::tables()),
        "tokenizer" => Some(tokenizer::tables()),
        "wordcount" => Some(wordcount::tables()),
        _ => None,
    }
}

pub fn is_synthetic(name: &str) -> bool {
    SYNTHETIC.contains(&name)
}
