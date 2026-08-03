pub mod body;
pub mod constructor;
pub mod function;
pub mod index;
pub mod lower;
pub mod report;

use zag_analysis::Analysis;
use zag_facts::tables::Tables;
use zag_render::RenderError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub source: Vec<u8>,
    pub report: Vec<u8>,
}

pub fn generate(tables: &Tables, analysis: &Analysis) -> Result<Output, RenderError> {
    let ast = lower::lower(tables, &analysis.ownership);
    Ok(Output {
        source: zag_render::render(&ast)?,
        report: report::render_report(tables, analysis),
    })
}
