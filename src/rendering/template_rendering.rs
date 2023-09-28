use crate::{
    config::model::{TemplateDefinition, TemplateValue},
    processing::ProcessingContext,
    rendering::{
        EmptyProcessingBlockSnafu, InvalidTemplateFileSnafu, NonTerminatedProcessingBlockSnafu,
        OutputWriteSnafu, ProcessingStatement, TemplateRenderError, ValueRenderer,
        ValueRenderingFailedSnafu,
    },
};
use snafu::ResultExt;
use std::io::{BufReader, Read, Write};
use std::rc::Rc;

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

    fn is_processing_block(&self, chars: &[char], char_index: usize) -> ProcessingStatement {
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
        chars: &[char],
        start_position: usize,
        end_marker: char,
    ) -> Result<(String, usize), TemplateRenderError> {
        if let Some(relative_index) = chars[start_position..]
            .iter()
            .position(|&c| c == end_marker)
        {
            Self::return_variable_name(template, start_position, relative_index)
        } else {
            NonTerminatedProcessingBlockSnafu {
                start_at: start_position,
            }
            .fail()
        }
    }

    fn extract_trailing_var_path(
        &self,
        template: &str,
        chars: &[char],
        start_position: usize,
    ) -> Result<(String, usize), TemplateRenderError> {
        if let Some(relative_index) = chars[start_position..].iter().position(|&c| c == '\n') {
            Self::return_variable_name(template, start_position, relative_index)
        } else {
            Ok((
                template[start_position..].trim().to_owned(),
                template.len() - start_position,
            ))
        }
    }

    fn return_variable_name(
        template: &str,
        start_position: usize,
        relative_index: usize,
    ) -> Result<(String, usize), TemplateRenderError> {
        if relative_index == 1 {
            EmptyProcessingBlockSnafu {
                start_at: start_position,
            }
            .fail()
        } else {
            let end = start_position + relative_index - 1;
            Ok((
                template[start_position..=end].trim().to_owned(),
                relative_index,
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
