extern crate yaml_rust;

use std::io::{BufRead, Write};

use snafu::{ResultExt, Snafu};

use crate::config::reader::ConfigReader;
use crate::processing::GlitterProcessor;
use crate::rendering::template_rendering::TemplateRenderer;

pub mod config;
pub mod processing;
pub mod rendering;

#[derive(Debug, Snafu)]
pub enum GlitterError {
    #[snafu(display("Failed to read input."))]
    InvalidConfig {
        #[snafu(backtrace)]
        source: crate::config::reader::ConfigReadError,
    },
    #[snafu(display("Failed to render value."))]
    RenderingFailed {
        #[snafu(backtrace)]
        source: crate::rendering::TemplateRenderError,
    },
}

pub fn process<TInput: BufRead, TOutput: Write>(
    input: &mut TInput,
    inputname: String,
    starting_directory: String,
    output: &mut TOutput,
) -> Result<(), GlitterError> {
    let config_reader = ConfigReader::new();
    let config = config_reader.read(input).context(InvalidConfigSnafu)?;

    let processor = GlitterProcessor::new(inputname, starting_directory, config);
    processor
        .run(TemplateRenderer::new(output))
        .context(RenderingFailedSnafu)?;

    Ok(())
}

pub fn report<E: 'static>(err: &E)
where
    E: std::error::Error,
    E: snafu::ErrorCompat,
    E: Send + Sync,
{
    eprintln!("[ERROR] {}", err);
    if let Some(source) = err.source() {
        eprintln!();
        eprintln!("Caused by:");
        for (i, e) in std::iter::successors(Some(source), |e| e.source()).enumerate() {
            eprintln!("   {}: {}", i, e);
        }
    }

    if let Some(backtrace) = snafu::ErrorCompat::backtrace(err) {
        eprintln!("Backtrace:");
        eprintln!("{}", backtrace);
    }
}
