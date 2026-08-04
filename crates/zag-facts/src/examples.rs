pub mod conflict;
pub mod ledger;
pub mod netpacket;
pub mod shapes;
pub mod telemetry;
pub mod tokenizer;
pub mod wordcount;

use crate::tables::Tables;

/// Every name here has a Zig project under `examples/<name>` that builds and
/// runs on its own, and the tables below are what reading that project hands
/// over. The frontend is held to them, so a table that drifts from the program
/// it describes fails rather than porting something that was never there.
pub const NAMES: [&str; 8] = [
    "conflict",
    "coverage",
    "ledger",
    "netpacket",
    "shapes",
    "telemetry",
    "tokenizer",
    "wordcount",
];

pub fn tables_for(name: &str) -> Option<Tables> {
    match name {
        "conflict" => Some(conflict::tables()),
        "coverage" => Some(crate::fixture::example_tables()),
        "ledger" => Some(ledger::tables()),
        "netpacket" => Some(netpacket::tables()),
        "shapes" => Some(shapes::tables()),
        "telemetry" => Some(telemetry::tables()),
        "tokenizer" => Some(tokenizer::tables()),
        "wordcount" => Some(wordcount::tables()),
        _ => None,
    }
}
