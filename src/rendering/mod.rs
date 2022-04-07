pub(crate) mod var_rendering;

use snafu::{Backtrace, ResultExt, Snafu};
use std::io::{BufReader, Read, Write};
use std::rc::Rc;

use crate::config::model::{TemplateDefinition, TemplateValue};
use crate::config::reader::ConfigReadError;
use crate::processing::ProcessingContext;

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

    #[snafu(display("Failed to process load"))]
    LoadCommandFailed {
        #[snafu(backtrace)]
        #[snafu(source(from(ConfigReadError, Box::new)))]
        source: Box<ConfigReadError>,
    },

    #[snafu(display("Failed to resolve variable path: {}", variable_path))]
    FailedResolvingVariable {
        variable_path: String,
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

pub struct TemplateRendererConfig {
    pub marker: char,
    pub start_block: char,
    pub end_block: char,
    pub line_quote: char,
}

pub const DEFAULT_RENDERER_CONFIG: TemplateRendererConfig = TemplateRendererConfig {
    marker: '*',
    start_block: '{',
    end_block: '}',
    line_quote: '>',
};

pub struct TemplateRenderer<'a> {
    config: TemplateRendererConfig,
    output: &'a mut dyn Write,
}

impl<'a> TemplateRenderer<'a> {
    pub fn new(output: &'a mut dyn Write) -> Self {
        Self {
            config: DEFAULT_RENDERER_CONFIG,
            output,
        }
    }

    pub fn based_upon_config(config: TemplateRendererConfig, output: &'a mut dyn Write) -> Self {
        Self { config, output }
    }

    pub(crate) fn render<TValueRenderer: ValueRenderer>(
        &mut self,
        template: &TemplateDefinition,
        body_iterations: usize,
        context: Rc<ProcessingContext>,
        value_renderer: &TValueRenderer,
    ) -> Result<(), TemplateRenderError> {
        if let Some(header) = &template.header {
            self.heavylift_render(header, 0, &context, value_renderer)?;
        }

        for iteration_count in 0..body_iterations {
            self.heavylift_render(&template.body, iteration_count, &context, value_renderer)?;
            self.output.write(b"\n").context(OutputWriteSnafu)?;
        }

        if let Some(footer) = &template.footer {
            self.heavylift_render(footer, 0, &context, value_renderer)?;
        }

        Ok(())
    }

    fn heavylift_render<TValueRenderer: ValueRenderer>(
        &mut self,
        template: &TemplateValue,
        iteration_count: usize,
        context: &Rc<ProcessingContext>,
        value_renderer: &TValueRenderer,
    ) -> Result<(), TemplateRenderError> {
        let mut from: usize = 0;
        let mut to: Option<usize> = None;

        let template_content = self.read_template(template, context)?;

        let chars = template_content.chars().collect::<Vec<_>>();
        let mut char_index = 0;

        while char_index < template_content.len() {
            if chars[char_index] == self.config.marker {
                match self.is_processing_block(&chars, char_index) {
                    ProcessingStatement::Block => {
                        self.quote(&template_content, from, to)?;

                        let (var_path, block_length) = self.extract_enclosed_var_path(
                            &template_content,
                            &chars,
                            char_index + 2,
                            self.config.end_block,
                        )?;
                        value_renderer
                            .render_value(&var_path, iteration_count, self)
                            .context(ValueRenderingFailedSnafu)?;

                        char_index += 3 + block_length;
                        from = char_index;
                        to = None;
                    }
                    ProcessingStatement::LineQuote => {
                        self.quote(&template_content, from, to)?;

                        let (var_path, line_length) = self.extract_trailing_var_path(
                            &template_content,
                            &chars,
                            char_index + 2,
                        )?;
                        value_renderer
                            .render_value(&var_path, iteration_count, self)
                            .context(ValueRenderingFailedSnafu)?;

                        char_index += 2 + line_length;
                        from = char_index;
                        to = None;
                    }
                    ProcessingStatement::None => {
                        char_index += 1;
                    }
                }
            } else {
                to = Some(char_index);
                char_index += 1;
            }
        }

        self.quote(&template_content, from, to)?;

        Ok(())
    }

    fn read_template(
        &self,
        template: &TemplateValue,
        context: &Rc<ProcessingContext>,
    ) -> Result<String, TemplateRenderError> {
        match template {
            TemplateValue::RawValue(template) => Ok(template.clone()),
            TemplateValue::Quote(file_path) => {
                let full_file_path = context.resolve_filename(file_path);
                let file =
                    std::fs::File::open(full_file_path).context(InvalidTemplateFileSnafu {
                        file_name: file_path,
                    })?;

                let mut buf_reader = BufReader::new(file);
                let mut contents = String::new();
                buf_reader
                    .read_to_string(&mut contents)
                    .context(InvalidTemplateFileSnafu {
                        file_name: file_path,
                    })?;

                Ok(contents)
            }
        }
    }

    fn is_processing_block(&self, chars: &Vec<char>, char_index: usize) -> ProcessingStatement {
        if chars[char_index + 1] == self.config.start_block {
            ProcessingStatement::Block
        } else if chars[char_index + 1] == self.config.line_quote {
            ProcessingStatement::LineQuote
        } else {
            ProcessingStatement::None
        }
    }

    fn quote(
        &mut self,
        template: &str,
        from: usize,
        to_option: Option<usize>,
    ) -> Result<(), TemplateRenderError> {
        if let Some(to) = to_option {
            self.output
                .write(template[from..=to].as_bytes())
                .context(OutputWriteSnafu)?;
        }

        Ok(())
    }

    fn extract_enclosed_var_path(
        &self,
        template: &str,
        chars: &Vec<char>,
        start_position: usize,
        end_marker: char,
    ) -> Result<(String, usize), TemplateRenderError> {
        let mut end = start_position;
        let mut hit_end = false;

        for char_index in start_position..template.len() {
            if chars[char_index] == end_marker {
                hit_end = true;
                break;
            }

            end = char_index;
        }

        if !hit_end {
            NonTerminatedProcessingBlockSnafu {
                start_at: start_position,
            }
            .fail()
        } else if end == start_position {
            EmptyProcessingBlockSnafu {
                start_at: start_position,
            }
            .fail()
        } else {
            Ok((
                template[start_position..=end].trim().to_owned(),
                end - start_position + 1,
            ))
        }
    }

    fn extract_trailing_var_path(
        &self,
        template: &str,
        chars: &Vec<char>,
        start_position: usize,
    ) -> Result<(String, usize), TemplateRenderError> {
        let mut end = start_position;

        for char_index in start_position..template.len() {
            if chars[char_index] == '\n' {
                break;
            }

            end = char_index;
        }

        if end == start_position {
            EmptyProcessingBlockSnafu {
                start_at: start_position,
            }
            .fail()
        } else {
            Ok((
                template[start_position..=end].trim().to_owned(),
                end - start_position + 1,
            ))
        }
    }
}

impl<'a> Write for TemplateRenderer<'a> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.output.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.output.flush()
    }
}
