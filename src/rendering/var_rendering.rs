use snafu::ResultExt;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::rc::Rc;

use crate::{
    config::model::RawValue,
    processing::{GlitterProcessor, ProcessingContext},
    rendering::{
        FailedReadingText, FailedWritingText, InvalidCalculateCall, RenderCommandFailed,
        TemplateRenderer, ValueRenderError,
    },
};

pub(crate) trait RenderableVariable {
    fn render(&self, output: &mut TemplateRenderer) -> Result<(), ValueRenderError>;
    fn calculate(&self) -> Result<RawValue, ValueRenderError>;
}

pub(crate) struct RenderableRawValue {
    value: RawValue,
}

impl From<RawValue> for RenderableRawValue {
    fn from(value: RawValue) -> Self {
        RenderableRawValue { value }
    }
}

impl RenderableVariable for RenderableRawValue {
    fn render(&self, output: &mut TemplateRenderer) -> std::result::Result<(), ValueRenderError> {
        match &self.value {
            RawValue::Boolean(true) => output.write(b"true").context(FailedWritingText)?,
            RawValue::Boolean(false) => output.write(b"false").context(FailedWritingText)?,
            RawValue::Integer(value) => output
                .write(format!("{}", value).as_bytes())
                .context(FailedWritingText)?,
            RawValue::Float(value) | RawValue::String(value) => {
                output.write(value.as_bytes()).context(FailedWritingText)?
            }
        };

        Ok(())
    }

    fn calculate(&self) -> std::result::Result<RawValue, ValueRenderError> {
        Ok(self.value.clone())
    }
}

pub(crate) struct RenderableQuote {
    file: String,
    context: Rc<ProcessingContext>,
}

impl RenderableQuote {
    pub(crate) fn from(file: String, context: Rc<ProcessingContext>) -> Self {
        RenderableQuote { file, context }
    }
}

impl RenderableVariable for RenderableQuote {
    fn render(&self, output: &mut TemplateRenderer) -> std::result::Result<(), ValueRenderError> {
        let fullname = self.context.resolve_filename(&self.file);

        let input = File::open(&fullname).context(FailedReadingText {
            input_file: fullname.clone(),
        })?;
        let buffered = BufReader::new(input);

        for line in buffered.lines() {
            output
                .write(
                    line.context(FailedReadingText {
                        input_file: fullname.clone(),
                    })?
                    .as_bytes(),
                )
                .context(FailedWritingText)?;
            output.write(b"\n").context(FailedWritingText)?;
        }

        Ok(())
    }

    fn calculate(&self) -> std::result::Result<RawValue, ValueRenderError> {
        let fullname = self.context.resolve_filename(&self.file);

        let input = File::open(&fullname).context(FailedReadingText {
            input_file: fullname,
        })?;
        let mut buffered = BufReader::new(input);
        let mut result = String::new();

        buffered
            .read_to_string(&mut result)
            .context(FailedWritingText)?;

        Ok(RawValue::String(result))
    }
}

pub(crate) struct SubRender {
    processor: GlitterProcessor,
}

impl From<GlitterProcessor> for SubRender {
    fn from(processor: GlitterProcessor) -> Self {
        SubRender { processor }
    }
}

impl RenderableVariable for SubRender {
    fn render(&self, renderer: &mut TemplateRenderer) -> std::result::Result<(), ValueRenderError> {
        self.processor
            .render(renderer)
            .context(RenderCommandFailed)?;
        Ok(())
    }

    fn calculate(&self) -> std::result::Result<RawValue, ValueRenderError> {
        InvalidCalculateCall {}.fail()
    }
}
