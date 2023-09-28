use crate::{
    config::model::{GlitterConfig, ValueDefinition, VariableDefinitionBlock},
    config::yaml_import::YamlImporter,
    processing::var_store::{ProcessingInstruction, StoredVariable, VariableStore},
    processing::{ProcessingContext, ValuePath},
    rendering::var_rendering::{
        RenderableExecutionResult, RenderableQuote, RenderableRawValue, RenderableVariable,
        SubRender,
    },
    rendering::{
        FailedAccessingFilesystemSnafu, FailedProcessingVariableSnafu, FailedReadingTextSnafu,
        FailedResolvingVariableSnafu, InvalidSubRenderConfigSnafu, LoadCommandFailedSnafu,
        TemplateRenderError, ValueRenderError, ValueRenderer,
    },
    ConfigReader, TemplateRenderer,
};
use snafu::ResultExt;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::rc::Rc;

enum NextVarProcessingInstruction {
    ContinueElsewhere(Option<Rc<ProcessingContext>>, ValuePath, RequestSource),
    ReturnValue(Box<dyn RenderableVariable>),
    ReportMissing,
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
            RequestSource::Template(iteration_count) => (0..=*iteration_count)
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
            FailedAccessingFilesystemSnafu {}.fail()
        }
    }

    fn convert_definition(
        &self,
        definition: ValueDefinition,
        context: Rc<ProcessingContext>,
        request_source: RequestSource,
        variable_path: String,
    ) -> Result<Box<dyn RenderableVariable>, ValueRenderError> {
        let mut empty_path = ValuePath(vec![]);
        let var = StoredVariable::from(&definition, &mut empty_path);

        match self
            .process_variable(var, &context, request_source)
            .context(FailedProcessingVariableSnafu {
                var_resolution_path: vec![variable_path],
            })? {
            NextVarProcessingInstruction::ReportMissing => {
                panic!("Can't be happening. There is no var name, so it can't be not resolved")
            }
            NextVarProcessingInstruction::ReturnValue(value) => Ok(value),
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
        request_source: RequestSource,
    ) -> Result<NextVarProcessingInstruction, ValueRenderError> {
        match variable {
            StoredVariable::Missing => Ok(NextVarProcessingInstruction::ReportMissing),
            StoredVariable::Value(actual_value) => Ok(NextVarProcessingInstruction::ReturnValue(
                Box::new(RenderableRawValue::from(actual_value)),
            )),
            StoredVariable::LocalReference(new_path) => Ok(
                NextVarProcessingInstruction::ContinueElsewhere(None, new_path, request_source),
            ),
            StoredVariable::DistantReference(new_path, new_context) => {
                Ok(NextVarProcessingInstruction::ContinueElsewhere(
                    Some(new_context),
                    new_path,
                    request_source,
                ))
            }
            StoredVariable::Instruction(instruction, new_path) => match instruction {
                ProcessingInstruction::Quote(file) => {
                    Ok(NextVarProcessingInstruction::ReturnValue(Box::new(
                        RenderableQuote::from(file, Rc::clone(context)),
                    )))
                }
                ProcessingInstruction::Import(file) => {
                    let import_context =
                        self.import_yaml(self.resolve_filename(&file, &request_source, context))?;
                    Ok(NextVarProcessingInstruction::ContinueElsewhere(
                        Some(import_context),
                        new_path,
                        RequestSource::CallingContext,
                    ))
                }
                ProcessingInstruction::Load(file, parameter) => {
                    let load_context = self.load(
                        self.resolve_filename(&file, &request_source, context),
                        parameter,
                        Rc::clone(context),
                    )?;

                    Ok(NextVarProcessingInstruction::ContinueElsewhere(
                        Some(load_context),
                        new_path,
                        RequestSource::CallingContext,
                    ))
                }
                ProcessingInstruction::Render(file, parameter) => {
                    let subprocessor = self.sub_processor_for(
                        self.resolve_filename(&file, &request_source, context),
                        parameter,
                        Rc::clone(context),
                    )?;

                    Ok(NextVarProcessingInstruction::ReturnValue(Box::new(
                        SubRender::from(subprocessor),
                    )))
                }
                ProcessingInstruction::Execute(executable, arguments) => {
                    let mut has_error = false;
                    let mut index = -1;
                    let rendered_values = arguments
                        .into_iter()
                        .map(|arg| {
                            index += 1;
                            match self.convert_definition(
                                arg,
                                Rc::clone(context),
                                request_source.clone(),
                                format!("[{}]", &index),
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
                        return Err(error);
                    }

                    let rendered_argument_list = rendered_values
                        .into_iter()
                        .map(|v| v.unwrap())
                        .collect::<Vec<_>>();

                    Ok(NextVarProcessingInstruction::ReturnValue(Box::new(
                        RenderableExecutionResult::from(
                            executable,
                            rendered_argument_list,
                            Rc::clone(context),
                        ),
                    )))
                }
            },
        }
    }

    fn resolve_var(
        &self,
        mut variable_path: ValuePath,
        mut request_source: RequestSource,
    ) -> Result<Box<dyn RenderableVariable>, ValueRenderError> {
        let mut context = Rc::clone(&self.root);
        let mut storages;
        let mut current_variable_path: String;
        let mut path_history = vec![];

        loop {
            current_variable_path = variable_path.render();
            path_history.push(current_variable_path.clone());
            storages = Self::storages_for(&self.global, &context, &request_source);

            let (result, next_request_source) = Self::read_variable(&storages, &mut variable_path);
            return match self
                .process_variable(result, &context, next_request_source)
                .context(FailedProcessingVariableSnafu {
                    var_resolution_path: path_history.clone(),
                })? {
                NextVarProcessingInstruction::ReportMissing => FailedResolvingVariableSnafu {
                    var_resolution_path: path_history,
                }
                .fail(),
                NextVarProcessingInstruction::ReturnValue(value) => Ok(value),
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
