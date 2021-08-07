extern crate yaml_rust;

pub mod config;
pub mod processing;
pub mod rendering;

use snafu::{Backtrace, ResultExt, Snafu};
use std::io::{BufRead, Write};

use crate::config::reader::ConfigReader;
use crate::processing::GlitterProcessor;
use crate::rendering::TemplateRenderer;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Failed to read input: {}", source))]
    InputReadError {
        source: crate::config::reader::ConfigReadError,
        backtrace: Backtrace,
    },
    #[snafu(display("Failed to render value: {}", source))]
    OutputRenderError {
        source: crate::rendering::TemplateRenderError,
        backtrace: Backtrace,
    },
}

pub fn process<TInput: BufRead, TOutput: Write>(
    input: &mut TInput,
    inputname: String,
    starting_directory: String,
    output: &mut TOutput,
) -> Result<(), Error> {
    let config_reader = ConfigReader::new();
    let config = config_reader.read(input).context(InputReadError)?;

    let processor = GlitterProcessor::new(inputname, starting_directory, config);
    processor
        .run(TemplateRenderer::new(output))
        .context(OutputRenderError)?;

    Ok(())
}
