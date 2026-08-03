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

/// Asks the compiler twice. Reflection resolves declarations and layout with
/// no linker involved, and the parser reaches the dataflow and the private
/// declarations reflection cannot see.
pub fn read_zig(path: &std::path::Path) -> Result<zag_frontend::program::Program, String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .ok_or("the workspace root is two directories up")?;
    let reflect = root.join("tools").join("reflect").join("main.zig");
    let extract = root.join("tools").join("extract").join("main.zig");
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
    if !extraction.contains("function ") && !extraction.contains("container ") {
        return Err(format!(
            "the parser reported nothing for {}:\n{extraction}",
            path.display()
        ));
    }
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
