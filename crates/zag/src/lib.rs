pub mod project;

pub use project::read_project;
use zag_emit::Output;
use zag_facts::tables::Tables;
use zag_facts::validate::{Violation, validate};
use zag_facts::wire::DecodeError;
use zag_render::RenderError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PipelineError {
    InvalidTables(Vec<Violation>),
    Decode(DecodeError),
    Render(RenderError),
}

pub fn generate(tables: &Tables) -> Result<Output, PipelineError> {
    validate(tables).map_err(PipelineError::InvalidTables)?;
    let analysis = zag_analysis::analyze(tables);
    zag_emit::generate(tables, &analysis).map_err(PipelineError::Render)
}

pub fn generate_from_bytes(bytes: &[u8]) -> Result<Output, PipelineError> {
    let tables = zag_facts::wire::decode(bytes).map_err(PipelineError::Decode)?;
    generate(&tables)
}

fn run_zig(arguments: &[&std::ffi::OsStr]) -> Result<String, String> {
    let output = std::process::Command::new("zig")
        .args(arguments)
        .output()
        .map_err(|cause| format!("running zig: {cause}"))?;
    Ok(String::from_utf8_lossy(&output.stderr).into_owned())
}

/// A path with `.` and `..` taken out, so two spellings of one file compare
/// equal and the crawl does not read it twice.
pub(crate) fn normalise(path: &std::path::Path) -> std::path::PathBuf {
    let absolute = std::env::current_dir()
        .map(|directory| directory.join(path))
        .unwrap_or_else(|_| path.to_path_buf());
    let mut out = std::path::PathBuf::new();
    for part in absolute.components() {
        match part {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// The two Zig tools travel inside this crate rather than beside it, so an
/// installed `zag` has them and a published one is not missing half of itself.
/// They are written out on demand, because zig is given a path rather than a
/// string.
const REFLECT_SOURCE: &str = include_str!("../tools/reflect/main.zig");
const EXTRACT_SOURCE: &str = include_str!("../tools/extract/main.zig");

fn tool_path(name: &str, source: &str) -> Result<std::path::PathBuf, String> {
    let directory = std::env::temp_dir()
        .join("zag-tools")
        .join(env!("CARGO_PKG_VERSION"))
        .join(name);
    std::fs::create_dir_all(&directory)
        .map_err(|cause| format!("{}: {cause}", directory.display()))?;
    let path = directory.join("main.zig");
    // Rewritten only when it differs, so zig's own cache keeps its hits.
    if std::fs::read_to_string(&path).ok().as_deref() != Some(source) {
        std::fs::write(&path, source).map_err(|cause| format!("{}: {cause}", path.display()))?;
    }
    Ok(path)
}

/// Asks the compiler twice about one file. Reflection resolves declarations
/// and layout with no linker involved, and the parser reaches the dataflow and
/// the private declarations reflection cannot see.
pub(crate) fn ask_zig(path: &std::path::Path) -> Result<(String, String), String> {
    let reflect = tool_path("reflect", REFLECT_SOURCE)?;
    let extract = tool_path("extract", EXTRACT_SOURCE)?;
    let reflection = run_zig(&[
        "build-obj".as_ref(),
        "-fno-emit-bin".as_ref(),
        "--dep".as_ref(),
        "target".as_ref(),
        format!("-Mroot={}", reflect.display()).as_ref(),
        format!("-Mtarget={}", path.display()).as_ref(),
    ])?;
    let extraction = run_zig(&[
        "run".as_ref(),
        extract.as_ref(),
        "--".as_ref(),
        path.as_ref(),
    ])?;
    if !extraction.contains("function ")
        && !extraction.contains("container ")
        && !extraction.contains("import ")
    {
        return Err(format!(
            "the parser reported nothing for {}:\n{extraction}",
            path.display()
        ));
    }
    Ok((extraction, reflection))
}

/// One file, read on its own. `read_project` follows what it imports.
pub fn read_zig(path: &std::path::Path) -> Result<zag_frontend::program::Program, String> {
    let (extraction, reflection) = ask_zig(path)?;
    Ok(zag_frontend::program::parse(&extraction, &reflection))
}

pub fn describe(error: &PipelineError) -> String {
    match error {
        PipelineError::InvalidTables(violations) => {
            format!("the fact tables are not well formed: {violations:?}")
        }
        PipelineError::Decode(cause) => format!("the fact file could not be read: {cause:?}"),
        PipelineError::Render(cause) => format!("the syntax tree could not be printed: {cause:?}"),
    }
}
