use std::fs::File;
use std::io::BufReader;
use std::ops::Index;
use std::path::Path;
use std::rc::Rc;

use snafu::ResultExt;

use crate::rendering::var_rendering::RenderableExecutionResult;
use crate::rendering::{
    FailedAccessingFilesystemSnafu, FailedReadingTextSnafu, FailedResolvingVariableSnafu,
    InvalidSubRenderConfigSnafu, LoadCommandFailedSnafu, TemplateRenderError, ValueRenderError,
    ValueRenderer,
};
use crate::{
    config::{
        model::{
            GlitterConfig, RawValue, TemplateDefinition, ValueDefinition, VariableDefinitionBlock,
        },
        reader::ConfigReader,
        yaml_import::YamlImporter,
    },
    processing::var_store::{ProcessingInstruction, StoredVariable, VariableStore},
    rendering::{
        template_rendering::TemplateRenderer,
        var_rendering::{RenderableQuote, RenderableRawValue, RenderableVariable, SubRender},
    },
};

mod var_store;

enum NextVarProcessingInstruction {
    ContinueElsewhere(Option<Rc<ProcessingContext>>, ValuePath, RequestSource),
    ReturnValue(Box<dyn RenderableVariable>),
    ThrowError(ValueRenderError),
    ReportMissing,
}

pub(crate) struct ProcessingContext {
    pub(crate) directory: String,
    pub(crate) local: Rc<VariableStore>,
    pub(crate) injection: Vec<Rc<VariableStore>>,
    pub(crate) template: Option<TemplateDefinition>,
}

impl ProcessingContext {
    pub(crate) fn initial(
        filename: String,
        directory: String,
        local_source: VariableDefinitionBlock,
        injection_source: Vec<VariableDefinitionBlock>,
        template_source: TemplateDefinition,
    ) -> Self {
        let mut iteration_count = 0;
        let injection = injection_source
            .into_iter()
            .map(|i| {
                iteration_count += 1;

                Rc::new(
                    VariableStore::from(i)
                        .insert(
                            "$iteration",
                            ValueDefinition::Value(RawValue::Integer(iteration_count)),
                        )
                        .insert(
                            "$filename",
                            ValueDefinition::Value(RawValue::String(filename.clone())),
                        )
                        .insert(
                            "$directory",
                            ValueDefinition::Value(RawValue::String(directory.clone())),
                        ),
                )
            })
            .collect::<Vec<_>>();

        ProcessingContext {
            directory,
            local: Rc::new(VariableStore::from(local_source)),
            injection,
            template: Some(template_source),
        }
    }

    pub(crate) fn subcontext(
        filename: String,
        directory: String,
        local_source: VariableDefinitionBlock,
        injection_source: Vec<VariableDefinitionBlock>,
        template_source: TemplateDefinition,
        parameter_source: VariableDefinitionBlock,
        source_context: Rc<ProcessingContext>,
    ) -> Self {
        let mut iteration_count = 0;
        let injection = injection_source
            .into_iter()
            .map(|i| {
                iteration_count += 1;

                Rc::new(
                    VariableStore::from(i)
                        .insert(
                            "$iteration",
                            ValueDefinition::Value(RawValue::Integer(iteration_count)),
                        )
                        .insert(
                            "$filename",
                            ValueDefinition::Value(RawValue::String(filename.clone())),
                        )
                        .insert(
                            "$directory",
                            ValueDefinition::Value(RawValue::String(directory.clone())),
                        ),
                )
            })
            .collect::<Vec<_>>();

        ProcessingContext {
            directory,
            local: Rc::new(VariableStore::with_context(
                local_source,
                parameter_source,
                source_context,
            )),
            injection,
            template: Some(template_source),
        }
    }

    pub(crate) fn local_standalone(
        directory: String,
        local_source: VariableDefinitionBlock,
    ) -> Self {
        ProcessingContext {
            directory,
            local: Rc::new(VariableStore::from(local_source)),
            injection: vec![],
            template: None,
        }
    }

    pub(crate) fn local_subcontext(
        directory: String,
        local_source: VariableDefinitionBlock,
        parameter_source: VariableDefinitionBlock,
        source_context: Rc<ProcessingContext>,
    ) -> Self {
        ProcessingContext {
            directory,
            local: Rc::new(VariableStore::with_context(
                local_source,
                parameter_source,
                source_context,
            )),
            injection: vec![],
            template: None,
        }
    }

    pub(crate) fn resolve_filename(&self, filename: &str) -> String {
        Path::new(&self.directory)
            .join(filename)
            .to_str()
            .unwrap()
            .to_owned()
    }
}

#[derive(Clone, PartialEq)]
enum RequestSource {
    /// The variable is accessed from a template of same context
    Template(usize),

    /// The variable is referenced from an injection variable of same context
    Injection,

    /// The variable is referenced from a local variable of same context
    Local,

    /// The variable is referenced from a global variable
    Global,

    /// The variable is referenced from an outside context
    CallingContext,
}

pub struct GlitterProcessor {
    global: Rc<ProcessingContext>,
    root: Rc<ProcessingContext>,
}

impl GlitterProcessor {
    pub fn new(filename: String, directory: String, config: GlitterConfig) -> Self {
        let global = Rc::new(ProcessingContext::local_standalone(
            directory.clone(),
            config.global,
        ));
        GlitterProcessor {
            global,
            root: Rc::new(ProcessingContext::initial(
                filename,
                directory,
                config.local,
                config.injection,
                config.template,
            )),
        }
    }

    fn subprocessor(
        &self,
        filename: String,
        directory: String,
        config: GlitterConfig,
        parameter: VariableDefinitionBlock,
        source_context: Rc<ProcessingContext>,
    ) -> Self {
        let global = Rc::clone(&self.global);
        GlitterProcessor {
            global,
            root: Rc::new(ProcessingContext::subcontext(
                filename,
                directory,
                config.local,
                config.injection,
                config.template,
                parameter,
                source_context,
            )),
        }
    }

    pub fn run(self, mut renderer: TemplateRenderer) -> Result<(), TemplateRenderError> {
        self.render(&mut renderer)
    }

    pub(crate) fn render(
        &self,
        renderer: &mut TemplateRenderer,
    ) -> Result<(), TemplateRenderError> {
        let injection_count = self.root.injection.len();

        if let Some(template) = self.root.template.clone() {
            renderer.render(&template, injection_count, Rc::clone(&self.root), self)?;
        }

        Ok(())
    }

    fn read_variable(
        storages: &[(Rc<VariableStore>, RequestSource)],
        variable_path: &mut ValuePath,
    ) -> (StoredVariable, RequestSource) {
        let first_key = &variable_path[0];
        for (single_storage, next_request_source) in storages {
            if single_storage.contains(first_key) {
                return (
                    single_storage.resolve(variable_path),
                    next_request_source.clone(),
                );
            }
        }

        (StoredVariable::Missing, RequestSource::Global)
    }

    fn storages_for(
        global: &Rc<ProcessingContext>,
        context: &ProcessingContext,
        request_source: &RequestSource,
    ) -> Vec<(Rc<VariableStore>, RequestSource)> {
        match request_source {
            RequestSource::Template(iteration_count) => (0..=iteration_count.clone())
                .rev()
                .map(|i| (Rc::clone(&context.injection[i]), RequestSource::Injection))
                .collect::<Vec<_>>(),
            RequestSource::Injection | RequestSource::Local => vec![
                (Rc::clone(&context.local), RequestSource::Local),
                (Rc::clone(&global.local), RequestSource::Global),
            ],
            RequestSource::Global => vec![(Rc::clone(&global.local), RequestSource::Global)],
            RequestSource::CallingContext => {
                vec![(Rc::clone(&context.local), RequestSource::Local)]
            }
        }
    }

    fn sub_processor_for(
        &self,
        full_file_path: String,
        parameter: VariableDefinitionBlock,
        source_context: Rc<ProcessingContext>,
    ) -> Result<GlitterProcessor, ValueRenderError> {
        let path = std::path::Path::new(&full_file_path);
        let filename = if let Some(name) = path.file_name() {
            name.to_str().unwrap().to_owned()
        } else {
            return FailedAccessingFilesystemSnafu {}.fail();
        };
        let directory = if let Some(dir) = path.parent() {
            dir.to_str().unwrap().to_owned()
        } else {
            return FailedAccessingFilesystemSnafu {}.fail();
        };

        let mut input =
            BufReader::new(File::open(full_file_path).expect("Could not open input file"));
        let config_reader = ConfigReader::new();
        let config = config_reader
            .read(&mut input)
            .context(InvalidSubRenderConfigSnafu)?;

        Ok(self.subprocessor(filename, directory, config, parameter, source_context))
    }

    fn import_yaml(
        &self,
        full_file_path: String,
    ) -> Result<Rc<ProcessingContext>, ValueRenderError> {
        let path = std::path::Path::new(&full_file_path);
        let directory = Self::extract_parent_directory(path)?;

        let input_file = File::open(full_file_path).expect("Could not open input file");
        let mut input_reader = BufReader::new(input_file);
        let imported_vals = YamlImporter::new().read(&mut input_reader).unwrap();

        Ok(Rc::new(ProcessingContext::local_standalone(
            directory,
            imported_vals,
        )))
    }

    fn load(
        &self,
        full_file_path: String,
        parameter: VariableDefinitionBlock,
        source_context: Rc<ProcessingContext>,
    ) -> Result<Rc<ProcessingContext>, ValueRenderError> {
        let path = std::path::Path::new(&full_file_path);
        let directory = Self::extract_parent_directory(path)?;

        let input_file = File::open(&full_file_path).context(FailedReadingTextSnafu {
            input_file: full_file_path,
        })?;
        let mut input_reader = BufReader::new(input_file);
        let loaded_vals = ConfigReader::new()
            .load(&mut input_reader)
            .context(LoadCommandFailedSnafu)?;

        Ok(Rc::new(ProcessingContext::local_subcontext(
            directory,
            loaded_vals,
            parameter,
            source_context,
        )))
    }

    fn extract_parent_directory(path: &Path) -> Result<String, ValueRenderError> {
        if let Some(dir) = path.parent() {
            Ok(dir.to_str().unwrap().to_owned())
        } else {
            return FailedAccessingFilesystemSnafu {}.fail();
        }
    }

    fn convert_definition(
        &self,
        definition: ValueDefinition,
        context: Rc<ProcessingContext>,
        request_source: RequestSource,
    ) -> Result<Box<dyn RenderableVariable>, ValueRenderError> {
        let mut empty_path = ValuePath(vec![]);
        let var = StoredVariable::from(&definition, &mut empty_path);

        match self.process_variable(var, &context, &request_source) {
            NextVarProcessingInstruction::ReportMissing => {
                panic!("Can't be happening. There is no var name, so it can't be not resolved")
            }
            NextVarProcessingInstruction::ReturnValue(value) => Ok(value),
            NextVarProcessingInstruction::ThrowError(e) => Err(e),
            NextVarProcessingInstruction::ContinueElsewhere(
                _,
                new_variable_path,
                new_request_source,
            ) => return self.resolve_var(new_variable_path, new_request_source),
        }
    }

    fn process_variable(
        &self,
        variable: StoredVariable,
        context: &Rc<ProcessingContext>,
        request_source: &RequestSource,
    ) -> NextVarProcessingInstruction {
        return match variable {
            StoredVariable::Missing => NextVarProcessingInstruction::ReportMissing,
            StoredVariable::Value(actual_value) => NextVarProcessingInstruction::ReturnValue(
                Box::new(RenderableRawValue::from(actual_value)),
            ),
            StoredVariable::LocalReference(new_path) => {
                NextVarProcessingInstruction::ContinueElsewhere(
                    None,
                    new_path,
                    request_source.clone(),
                )
            }
            StoredVariable::DistantReference(new_path, new_context) => {
                NextVarProcessingInstruction::ContinueElsewhere(
                    Some(new_context),
                    new_path,
                    request_source.clone(),
                )
            }
            StoredVariable::Instruction(instruction, new_path) => match instruction {
                ProcessingInstruction::Quote(file) => NextVarProcessingInstruction::ReturnValue(
                    Box::new(RenderableQuote::from(file, Rc::clone(context))),
                ),
                ProcessingInstruction::Import(file) => {
                    match self.import_yaml(self.resolve_filename(&file, &request_source, context)) {
                        Ok(context) => NextVarProcessingInstruction::ContinueElsewhere(
                            Some(context),
                            new_path,
                            RequestSource::CallingContext,
                        ),
                        Err(e) => NextVarProcessingInstruction::ThrowError(e),
                    }
                }
                ProcessingInstruction::Load(file, parameter) => {
                    match self.load(
                        self.resolve_filename(&file, &request_source, context),
                        parameter,
                        Rc::clone(context),
                    ) {
                        Ok(context) => NextVarProcessingInstruction::ContinueElsewhere(
                            Some(context),
                            new_path,
                            RequestSource::CallingContext,
                        ),
                        Err(e) => NextVarProcessingInstruction::ThrowError(e),
                    }
                }
                ProcessingInstruction::Render(file, parameter) => {
                    match self.sub_processor_for(
                        self.resolve_filename(&file, &request_source, context),
                        parameter,
                        Rc::clone(context),
                    ) {
                        Ok(subprocessor) => NextVarProcessingInstruction::ReturnValue(Box::new(
                            SubRender::from(subprocessor),
                        )),
                        Err(e) => NextVarProcessingInstruction::ThrowError(e),
                    }
                }
                ProcessingInstruction::Execute(executable, arguments) => {
                    let mut has_error = false;
                    let rendered_values = arguments
                        .into_iter()
                        .map(|arg| {
                            match self.convert_definition(
                                arg,
                                Rc::clone(context),
                                request_source.clone(),
                            ) {
                                Ok(renderable) => renderable.calculate(),
                                Err(e) => {
                                    has_error = true;
                                    Err(e)
                                }
                            }
                        })
                        .collect::<Vec<_>>();

                    if has_error {
                        let error = rendered_values
                            .into_iter()
                            .find(|v| v.is_err())
                            .unwrap()
                            .unwrap_err();
                        return NextVarProcessingInstruction::ThrowError(error);
                    }

                    let rendered_argument_list = rendered_values
                        .into_iter()
                        .map(|v| v.unwrap())
                        .collect::<Vec<_>>();
                    NextVarProcessingInstruction::ReturnValue(Box::new(
                        RenderableExecutionResult::from(
                            executable,
                            rendered_argument_list,
                            Rc::clone(context),
                        ),
                    ))
                }
            },
        };
    }

    fn resolve_var(
        &self,
        mut variable_path: ValuePath,
        mut request_source: RequestSource,
    ) -> Result<Box<dyn RenderableVariable>, ValueRenderError> {
        let mut context = Rc::clone(&self.root);
        let mut storages;
        let mut current_variable_path: String;

        loop {
            current_variable_path = variable_path.render();
            storages = Self::storages_for(&self.global, &context, &request_source);

            let (result, next_request_source) = Self::read_variable(&storages, &mut variable_path);
            return match self.process_variable(result, &context, &next_request_source) {
                NextVarProcessingInstruction::ReportMissing => FailedResolvingVariableSnafu {
                    variable_path: current_variable_path,
                }
                .fail(),
                NextVarProcessingInstruction::ReturnValue(value) => Ok(value),
                NextVarProcessingInstruction::ThrowError(e) => Err(e),
                NextVarProcessingInstruction::ContinueElsewhere(
                    new_context_option,
                    new_variable_path,
                    new_request_source,
                ) => {
                    if let Some(new_context) = new_context_option {
                        context = new_context;
                    }
                    variable_path = new_variable_path;
                    request_source = new_request_source;

                    continue;
                }
            };
        }
    }

    fn resolve_filename(
        &self,
        filename: &str,
        request_source: &RequestSource,
        current_context: &Rc<ProcessingContext>,
    ) -> String {
        if *request_source == RequestSource::Global {
            self.global.resolve_filename(filename)
        } else {
            current_context.resolve_filename(filename)
        }
    }
}

impl ValueRenderer for GlitterProcessor {
    fn render_value(
        &self,
        variable_path: &str,
        iteration_count: usize,
        output: &mut TemplateRenderer,
    ) -> Result<(), ValueRenderError> {
        let vp = ValuePath(
            variable_path
                .split(|c| c == '.')
                .map(|s| s.to_owned())
                .collect::<Vec<_>>(),
        );
        let value = self.resolve_var(vp, RequestSource::Template(iteration_count))?;
        value.render(output)?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ValuePath(Vec<String>);

impl ValuePath {
    #[inline(always)]
    fn append(&mut self, other: &mut ValuePath) {
        self.0.append(&mut other.0)
    }

    #[inline(always)]
    pub(crate) fn is_empty(&self) -> bool {
        return self.0.is_empty();
    }

    #[inline(always)]
    pub(crate) fn take_first(&mut self) -> String {
        self.0.remove(0)
    }

    #[inline(always)]
    fn render(&self) -> String {
        self.0.join(".")
    }
}

impl Index<usize> for ValuePath {
    type Output = String;

    #[inline(always)]
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl From<&String> for ValuePath {
    #[inline(always)]
    fn from(path: &String) -> Self {
        ValuePath(
            path.split(".")
                .into_iter()
                .map(|s| s.to_owned())
                .collect::<Vec<_>>(),
        )
    }
}
