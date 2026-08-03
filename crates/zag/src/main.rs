use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "zag", version, about)]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Writes the worked example fact file that stands in for the Zig frontend
    Fixture {
        #[arg(long)]
        output: PathBuf,
    },
    /// Reads a fact file and writes the ported Rust and its ownership report
    Emit {
        #[arg(long)]
        facts: PathBuf,
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        report: PathBuf,
    },
}

fn main() -> std::process::ExitCode {
    match run(Arguments::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("zag: {message}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(arguments: Arguments) -> Result<(), String> {
    match arguments.command {
        Command::Fixture { output } => {
            let tables = zag_facts::fixture::example_tables();
            write(&output, &zag_facts::wire::encode(&tables))
        }
        Command::Emit {
            facts,
            source,
            report,
        } => {
            let bytes =
                std::fs::read(&facts).map_err(|cause| format!("{}: {cause}", facts.display()))?;
            let output = zag::generate_from_bytes(&bytes).map_err(|cause| zag::describe(&cause))?;
            write(&source, &output.source)?;
            write(&report, &output.report)
        }
    }
}

fn write(path: &PathBuf, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|cause| format!("{}: {cause}", parent.display()))?;
    }
    std::fs::write(path, bytes).map_err(|cause| format!("{}: {cause}", path.display()))
}
