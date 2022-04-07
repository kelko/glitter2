use snafu::ResultExt;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::rc::Rc;

use crate::{
    config::model::RawValue,
    processing::{GlitterProcessor, ProcessingContext},
    rendering::{
        FailedReadingTextSnafu, FailedWritingTextSnafu, InvalidCalculateCallSnafu,
        RenderCommandFailedSnafu, TemplateRenderer, ValueRenderError,
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
            RawValue::Boolean(true) => output.write(b"true").context(FailedWritingTextSnafu)?,
            RawValue::Boolean(false) => output.write(b"false").context(FailedWritingTextSnafu)?,
            RawValue::Integer(value) => output
                .write(format!("{}", value).as_bytes())
                .context(FailedWritingTextSnafu)?,
            RawValue::Float(value) | RawValue::String(value) => output
                .write(value.as_bytes())
                .context(FailedWritingTextSnafu)?,
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

        let input = File::open(&fullname).context(FailedReadingTextSnafu {
            input_file: fullname.clone(),
        })?;
        let buffered = BufReader::new(input);

        for line in buffered.lines() {
            output
                .write(
                    line.context(FailedReadingTextSnafu {
                        input_file: fullname.clone(),
                    })?
                    .as_bytes(),
                )
                .context(FailedWritingTextSnafu)?;
            output.write(b"\n").context(FailedWritingTextSnafu)?;
        }

        Ok(())
    }

    fn calculate(&self) -> std::result::Result<RawValue, ValueRenderError> {
        let fullname = self.context.resolve_filename(&self.file);

        let input = File::open(&fullname).context(FailedReadingTextSnafu {
            input_file: fullname,
        })?;
        let mut buffered = BufReader::new(input);
        let mut result = String::new();

        buffered
            .read_to_string(&mut result)
            .context(FailedWritingTextSnafu)?;

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
            .context(RenderCommandFailedSnafu)?;
        Ok(())
    }

    fn calculate(&self) -> std::result::Result<RawValue, ValueRenderError> {
        InvalidCalculateCallSnafu {}.fail()
    }
}
