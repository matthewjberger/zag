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

pub fn describe(error: &PipelineError) -> String {
    match error {
        PipelineError::InvalidTables(violations) => {
            format!("the fact tables are not well formed: {violations:?}")
        }
        PipelineError::Decode(cause) => format!("the fact file could not be read: {cause:?}"),
        PipelineError::Render(cause) => format!("the syntax tree could not be printed: {cause:?}"),
    }
}
