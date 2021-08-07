use snafu::ResultExt;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::rc::Rc;

use crate::{
    config::model::RawValue,
    processing::{GlitterProcessor, ProcessingContext},
    rendering::{InvalidCalculateCall, TemplateRenderer, ValueRenderError, WriteValueError},
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
            RawValue::Boolean(true) => output.write(b"true").context(WriteValueError)?,
            RawValue::Boolean(false) => output.write(b"false").context(WriteValueError)?,
            RawValue::Integer(value) => output
                .write(format!("{}", value).as_bytes())
                .context(WriteValueError)?,
            RawValue::Float(value) | RawValue::String(value) => {
                output.write(value.as_bytes()).context(WriteValueError)?
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

        let input = File::open(fullname).context(WriteValueError)?;
        let buffered = BufReader::new(input);

        for line in buffered.lines() {
            output
                .write(line.context(WriteValueError)?.as_bytes())
                .context(WriteValueError)?;
            output.write(b"\n").context(WriteValueError)?;
        }

        Ok(())
    }

    fn calculate(&self) -> std::result::Result<RawValue, ValueRenderError> {
        let fullname = self.context.resolve_filename(&self.file);

        let input = File::open(fullname).context(WriteValueError)?;
        let mut buffered = BufReader::new(input);
        let mut result = String::new();

        buffered
            .read_to_string(&mut result)
            .context(WriteValueError)?;

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
        if let Err(error) = self.processor.render(renderer) {
            return Err(ValueRenderError::SubRenderError {
                source: Box::new(error),
            });
        }

        Ok(())
    }

    fn calculate(&self) -> std::result::Result<RawValue, ValueRenderError> {
        InvalidCalculateCall {}.fail()
    }
}
