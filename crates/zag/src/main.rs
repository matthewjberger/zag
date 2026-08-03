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
    /// Lists the example programs that carry hand-built fact tables
    Examples,
    /// Writes an example's fact file from its hand-built tables
    Facts {
        #[arg(long)]
        example: String,
        #[arg(long)]
        output: PathBuf,
    },
    /// Reads a Zig program with the compiler and writes its fact file. The
    /// path is the root file, and everything it imports is read with it.
    Read {
        #[arg(long)]
        zig: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "x86_64-linux")]
        target: String,
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
        Command::Examples => {
            for name in zag_facts::examples::NAMES {
                println!("{name}");
            }
            Ok(())
        }
        Command::Facts { example, output } => {
            let tables = zag_facts::examples::tables_for(&example).ok_or_else(|| {
                format!(
                    "unknown example {example:?}, try one of: {}",
                    zag_facts::examples::NAMES.join(", ")
                )
            })?;
            write(&output, &zag_facts::wire::encode(&tables))
        }
        Command::Read {
            zig,
            output,
            target,
        } => {
            let modules = zag::read_project(&zig)?;
            let tables = zag_frontend::build_project(&modules, &target);
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
