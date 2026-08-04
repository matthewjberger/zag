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
    /// Reads a Zig project with the compiler and writes its fact file. The
    /// path is a root file, whose imports are read with it, or a build.zig or
    /// the directory holding one, whose artifacts are each read from their own
    /// root.
    Read {
        #[arg(long)]
        zig: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "x86_64-linux")]
        target: String,
    },
    /// Prints a fact file as text, one row per line
    Dump {
        #[arg(long)]
        facts: PathBuf,
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
    /// Reads a fact file and writes the port as a crate, or as a workspace
    /// where the program needs more than one crate
    Build {
        #[arg(long)]
        facts: PathBuf,
        #[arg(long)]
        into: PathBuf,
        /// What to call the crate the program itself becomes
        #[arg(long, default_value = "port")]
        name: String,
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
            let project = zag::read_project(&zig)?;
            let tables = zag_frontend::build_project(&project, &target);
            write(&output, &zag_facts::wire::encode(&tables))
        }
        Command::Dump { facts } => {
            let bytes =
                std::fs::read(&facts).map_err(|cause| format!("{}: {cause}", facts.display()))?;
            let tables = zag_facts::wire::decode(&bytes)
                .map_err(|cause| format!("the fact file could not be read: {cause:?}"))?;
            print!("{}", zag_facts::dump::dump(&tables));
            Ok(())
        }
        Command::Build { facts, into, name } => {
            let bytes =
                std::fs::read(&facts).map_err(|cause| format!("{}: {cause}", facts.display()))?;
            let tables = zag_facts::wire::decode(&bytes)
                .map_err(|cause| format!("the fact file could not be read: {cause:?}"))?;
            let output = zag::generate(&tables).map_err(|cause| zag::describe(&cause))?;
            let port = zag_emit::layout::lay_out(&tables, &output, &name);
            for file in &port.files {
                write(&into.join(&file.path), &file.contents)?;
            }
            write(&into.join("port.report.txt"), &output.report)?;
            let crates = format!(
                "{} {}",
                port.crates.len(),
                plural(port.crates.len(), "crate", "crates")
            );
            let binaries = if port.binaries.is_empty() {
                String::new()
            } else {
                format!(
                    " and {} {}",
                    port.binaries.len(),
                    plural(port.binaries.len(), "binary", "binaries")
                )
            };
            println!("{crates}{binaries} into {}", into.display());
            Ok(())
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

fn plural(count: usize, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 { one } else { many }
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
