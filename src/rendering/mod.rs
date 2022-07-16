use std::string::FromUtf8Error;

use snafu::{Backtrace, Snafu};

use crate::config::reader::ConfigReadError;
use crate::rendering::template_rendering::TemplateRenderer;

pub mod template_rendering;
pub(crate) mod var_rendering;

enum ProcessingStatement {
    Block,
    LineQuote,
    None,
}

#[derive(Debug, Snafu)]
pub enum TemplateRenderError {
    #[snafu(display("Failed to generate a value for a processing block"))]
    ValueRenderingFailed {
        #[snafu(backtrace)]
        #[snafu(source(from(ValueRenderError, Box::new)))]
        source: Box<ValueRenderError>,
    },
    #[snafu(display("Failed to write rendered text"))]
    OutputWriteError {
        #[snafu(source(from(std::io::Error, Box::new)))]
        source: Box<std::io::Error>,
        backtrace: Backtrace,
    },
    #[snafu(display("Empty processing block in template starting at {}", start_at))]
    EmptyProcessingBlock {
        start_at: usize,
        backtrace: Backtrace,
    },
    #[snafu(display("Non terminated processing block in template starting at {}", start_at))]
    NonTerminatedProcessingBlock {
        start_at: usize,
        backtrace: Backtrace,
    },
    #[snafu(display("Invalid template source definition {}", source_definition))]
    InvalidTemplateSource {
        source_definition: String,
        backtrace: Backtrace,
    },
    #[snafu(display("Invalid template source file {}", file_name))]
    InvalidTemplateFile {
        file_name: String,
        #[snafu(source(from(std::io::Error, Box::new)))]
        source: Box<std::io::Error>,
        backtrace: Backtrace,
    },
}

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum ValueRenderError {
    #[snafu(display("Failed to write rendered text"))]
    FailedWritingText {
        #[snafu(source(from(std::io::Error, Box::new)))]
        source: Box<std::io::Error>,
        backtrace: Backtrace,
    },

    #[snafu(display("Failed to read input text from file {}", input_file))]
    FailedReadingText {
        input_file: String,
        #[snafu(source(from(std::io::Error, Box::new)))]
        source: Box<std::io::Error>,
        backtrace: Backtrace,
    },
    #[snafu(display("Value can't be calculated, only rendered directly"))]
    InvalidCalculateCall { backtrace: Backtrace },

    #[snafu(display("Sub-Rendering failed"))]
    RenderCommandFailed {
        #[snafu(backtrace)]
        #[snafu(source(from(TemplateRenderError, Box::new)))]
        source: Box<TemplateRenderError>,
    },

    #[snafu(display("Filesystem could not be accessed successfully"))]
    FailedAccessingFilesystem { backtrace: Backtrace },

    #[snafu(display("Failed to process load"))]
    LoadCommandFailed {
        #[snafu(backtrace)]
        #[snafu(source(from(ConfigReadError, Box::new)))]
        source: Box<ConfigReadError>,
    },

    ExecuteCommandFailed {
        #[snafu(source(from(std::io::Error, Box::new)))]
        source: Box<std::io::Error>,
        backtrace: Backtrace,
    },

    ExecuteResultInvalid {
        #[snafu(source(from(FromUtf8Error, Box::new)))]
        source: Box<FromUtf8Error>,
        backtrace: Backtrace,
    },

    #[snafu(display(
        "Failed to process variable definition: {}",
        var_resolution_path.join("\n\t\t⇒ ")
    ))]
    FailedProcessingVariable {
        var_resolution_path: Vec<String>,
        #[snafu(backtrace)]
        #[snafu(source(from(ValueRenderError, Box::new)))]
        source: Box<ValueRenderError>,
    },

    #[snafu(display(
        "Failed to resolve variable path: {}",
        var_resolution_path.join("\n\t\t⇒")
    ))]
    FailedResolvingVariable {
        var_resolution_path: Vec<String>,
        backtrace: Backtrace,
    },

    #[snafu(display("Configuration for Sub-Rendering is invalid",))]
    InvalidSubRenderConfig {
        #[snafu(backtrace)]
        #[snafu(source(from(ConfigReadError, Box::new)))]
        source: Box<ConfigReadError>,
    },
}

pub trait ValueRenderer {
    fn render_value(
        &self,
        variable_path: &str,
        iteration_count: usize,
        output: &mut TemplateRenderer,
    ) -> Result<(), ValueRenderError>;
}
