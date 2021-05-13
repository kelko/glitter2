mod var_store;

use std::path::Path;
use std::fs::File;
use std::io::BufReader;
use std::rc::Rc;

use snafu::{ResultExt};

use crate::{
    rendering::{TemplateRenderer, TemplateRenderError, ValueRenderer, ValueRenderError, ResolveVarError, SubRenderConfigError},
    config::{
        reader::ConfigReader,
        yaml_import::YamlImporter,
        model::{GlitterConfig, VariableDefinitionBlock, ValueDefinition, RawValue, TemplateDefinition}
    },
    processing::var_store::{VariableStore, StoredVariable, ProcessingInstruction},
    rendering::{
        LoadError,
        var_rendering::{ RenderableVariable, RenderableRawValue, RenderableQuote, SubRender },
    },
};

pub(crate) struct ProcessingContext {
    pub(crate) directory: String,
    pub(crate) local: Rc<VariableStore>,
    pub(crate) injection: Vec<Rc<VariableStore>>,
    pub(crate) template: Option<TemplateDefinition>
}

impl ProcessingContext {
    pub(crate) fn initial(filename: String, directory: String, local_source: VariableDefinitionBlock, injection_source: Vec<VariableDefinitionBlock>, template_source: TemplateDefinition) -> Self {
        let mut iteration_count = 0;
        let injection = injection_source.into_iter().map(|i| {
            iteration_count += 1;

            Rc::new(VariableStore::from(i)
                .insert("$iteration", ValueDefinition::Value(RawValue::Integer(iteration_count)))
                .insert("$filename", ValueDefinition::Value(RawValue::String(filename.clone())))
                .insert("$directory", ValueDefinition::Value(RawValue::String(directory.clone())))
            )

        }).collect::<Vec<_>>();

        ProcessingContext {
            directory,
            local: Rc::new(VariableStore::from(local_source)),
            injection,
            template: Some(template_source)
        }
    }

    pub(crate) fn subcontext(filename: String, directory: String, local_source: VariableDefinitionBlock, injection_source: Vec<VariableDefinitionBlock>, template_source: TemplateDefinition, parameter_source: VariableDefinitionBlock, source_context: Rc<ProcessingContext>) -> Self {
        let mut iteration_count = 0;
        let injection = injection_source.into_iter().map(|i| {
            iteration_count += 1;

            Rc::new(VariableStore::from(i)
                .insert("$iteration", ValueDefinition::Value(RawValue::Integer(iteration_count)))
                .insert("$filename", ValueDefinition::Value(RawValue::String(filename.clone())))
                .insert("$directory", ValueDefinition::Value(RawValue::String(directory.clone())))
            )

        }).collect::<Vec<_>>();

        ProcessingContext {
            directory,
            local: Rc::new(VariableStore::with_context(local_source, parameter_source, source_context)),
            injection,
            template: Some(template_source)
        }
    }

    pub(crate) fn local_standalone(directory: String, local_source: VariableDefinitionBlock) -> Self {
        ProcessingContext {
            directory,
            local: Rc::new(VariableStore::from(local_source)),
            injection: vec![],
            template: None
        }
    }

    pub(crate) fn local_subcontext(directory: String, local_source: VariableDefinitionBlock, parameter_source: VariableDefinitionBlock, source_context: Rc<ProcessingContext>) -> Self {
        ProcessingContext {
            directory,
            local: Rc::new(VariableStore::with_context(local_source, parameter_source, source_context)),
            injection: vec![],
            template: None
        }
    }

    pub(crate) fn resolve_filename(&self, filename: &str) -> String {
        Path::new(&self.directory).join(filename).to_str().unwrap().to_owned()
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
    root: Rc<ProcessingContext>
}

impl GlitterProcessor {
    pub fn new(filename: String, directory: String, config: GlitterConfig) -> Self {
        let global = Rc::new( ProcessingContext::local_standalone(directory.clone(), config.global));
        GlitterProcessor { 
            global,
            root: Rc::new(ProcessingContext::initial(filename, directory, config.local, config.injection, config.template))
        }
    }

    fn subprocessor(&self, filename: String, directory: String, config: GlitterConfig, parameter: VariableDefinitionBlock, source_context: Rc<ProcessingContext>) -> Self {
        let global = Rc::clone(&self.global);
        GlitterProcessor {
            global,
            root: Rc::new(ProcessingContext::subcontext(filename, directory, config.local, config.injection, config.template, parameter, source_context))
        }
    }

    pub fn run(self, mut renderer: TemplateRenderer) -> Result<(), TemplateRenderError> {
        self.render(&mut renderer)
    }

    pub(crate) fn render(&self, renderer: &mut TemplateRenderer) -> Result<(), TemplateRenderError> {
        let injection_count = self.root.injection.len();

        if let Some(template) = self.root.template.clone() {
            renderer.render(&template, injection_count, Rc::clone(&self.root), self)?;
        }

        Ok(())
    }

    fn read_variable(storages: &[(Rc<VariableStore>, RequestSource)], variable_path: Vec<String>) -> Option<(StoredVariable, RequestSource)> {
        let first_key = &variable_path[0];
        for (single_storage, next_request_source) in storages {
            if single_storage.contains(first_key) {
                return Some( (single_storage.resolve(variable_path).ok()?, next_request_source.clone()) );
            }
        }

        None
    }

    fn storages_for(global: &Rc<ProcessingContext>, context: &ProcessingContext, request_source: &RequestSource) -> Vec<(Rc<VariableStore>, RequestSource)> {
        match request_source {
            RequestSource::Template(iteration_count) => {
                (0..=iteration_count.clone()).rev().map(|i| (Rc::clone(&context.injection[i]), RequestSource::Injection) ).collect::<Vec<_>>()
            },
            RequestSource::Injection | RequestSource::Local  => vec![
                (Rc::clone(&context.local), RequestSource::Local),
                (Rc::clone(&global.local), RequestSource::Global)
            ],
            RequestSource::Global => vec![ (Rc::clone(&global.local), RequestSource::Global) ],
            RequestSource::CallingContext => vec![ (Rc::clone(&context.local), RequestSource::Local) ],
        }
    }

    fn sub_processor_for(&self, full_file_path: String, parameter: VariableDefinitionBlock, source_context: Rc<ProcessingContext>) -> Result<GlitterProcessor, ValueRenderError> {
        let path = std::path::Path::new(&full_file_path);
        let filename = if let Some(name) = path.file_name() {
            name.to_str().unwrap().to_owned()
        } else {
            todo!()
        };
        let directory = if let Some(dir) = path.parent() {
            dir.to_str().unwrap().to_owned()
        } else {
            todo!()
        };

        let mut input = BufReader::new(File::open(full_file_path).expect("Could not open input file"));
        let config_reader = ConfigReader::new();
        let config = config_reader.read(&mut input).context(SubRenderConfigError)?;

        Ok(self.subprocessor(filename, directory, config, parameter, source_context))
    }

    fn import_yaml(&self, full_file_path: String) -> Result<Rc<ProcessingContext>, ValueRenderError> {
        let path = std::path::Path::new(&full_file_path);
        let directory = if let Some(dir) = path.parent() {
            dir.to_str().unwrap().to_owned()
        } else {
            todo!()
        };

        let input_file = File::open(full_file_path).expect("Could not open input file");
        let mut input_reader = BufReader::new(input_file);
        let imported_vals = YamlImporter::new().read(&mut input_reader).unwrap();
        
        Ok(Rc::new(ProcessingContext::local_standalone(directory, imported_vals)))
    }

    fn load(&self, full_file_path: String, parameter: VariableDefinitionBlock, source_context: Rc<ProcessingContext>) -> Result<Rc<ProcessingContext>, ValueRenderError> {
        let path = std::path::Path::new(&full_file_path);
        let directory = if let Some(dir) = path.parent() {
            dir.to_str().unwrap().to_owned()
        } else {
            todo!()
        };

        let input_file = File::open(full_file_path).expect("Could not open input file");
        let mut input_reader = BufReader::new(input_file);
        let loaded_vals = ConfigReader::new().load(&mut input_reader).context( LoadError )?;
        
        Ok(Rc::new(ProcessingContext::local_subcontext(directory, loaded_vals, parameter, source_context)))
    }

    fn resolve_var(&self, mut variable_path: Vec<String>, mut request_source: RequestSource) -> Result<Box<dyn RenderableVariable>, ValueRenderError> {
        let mut context = Rc::clone(&self.root);
        let mut storages;
        let mut current_variable_path;

        loop {
            current_variable_path = build_value_path(&variable_path);
            storages =  Self::storages_for(&self.global, &context, &request_source);

            if let Some( (result, next_request_source) ) = Self::read_variable(&storages, variable_path) {
                match result {
                    StoredVariable::Value(actual_value) => return Ok(Box::new(RenderableRawValue::from(actual_value))),
                    StoredVariable::LocalReference(new_path) => {
                        variable_path = new_path;
                        request_source = next_request_source;

                        continue;
                    },
                    StoredVariable::DistantReference(new_path, new_context) => {
                        variable_path = new_path;
                        context = new_context;
                        request_source = next_request_source;

                        continue;
                    },
                    StoredVariable::Instruction(instruction, new_path) => {
                        match instruction {
                            ProcessingInstruction::Quote(file) => return Ok(Box::new(RenderableQuote::from(file, Rc::clone(&context)))),
                            ProcessingInstruction::Import(file) => {
                                variable_path = new_path;
                                request_source = RequestSource::CallingContext;
                                context = self.import_yaml(self.resolve_filename(&file, &next_request_source, &context))?;

                                continue;
                            },
                            ProcessingInstruction::Load(file, parameter) => {
                                variable_path = new_path;
                                request_source = RequestSource::CallingContext;
                                context = self.load(self.resolve_filename(&file, &next_request_source, &context), parameter, Rc::clone(&context))?;

                                continue;
                            },
                            ProcessingInstruction::Render(file, parameter) => {
                                let subprocessor = self.sub_processor_for(self.resolve_filename(&file, &next_request_source, &context), parameter, Rc::clone(&context))?;
                                return Ok(Box::new(SubRender::from(subprocessor)));
                            },
                            /*ProcessingInstruction::Select(_) => {
                                todo!()
                            }*/
                        }
                    }
                }
                
            } else {
                break;
            }
        }

        ResolveVarError {variable_path: current_variable_path}.fail()
    }

    fn resolve_filename(&self, filename: &str, request_source: &RequestSource, current_context: &Rc<ProcessingContext>) -> String {
        if *request_source == RequestSource::Global {
            self.global.resolve_filename(filename)

        } else {
            current_context.resolve_filename(filename)
        }
    }
}

impl ValueRenderer for GlitterProcessor {
    fn render_value(&self, variable_path: &str, iteration_count: usize, output: &mut TemplateRenderer) -> Result<(), ValueRenderError> {
        let vp = variable_path.split(|c| c == '.').map(|s| s.to_owned()).collect::<Vec<_>>();
        let value = self.resolve_var(vp, RequestSource::Template(iteration_count))?;
        value.render(output)?;
        
        Ok(())
    }
}

pub(crate) fn split_value_path(value_path: &str) -> Vec<String> {
    value_path.split(".").into_iter().map(|s| s.to_owned()).collect::<Vec<String>>()
}

pub(crate) fn build_value_path(path_elements: &Vec<String>) -> String {
    path_elements.join(".")
}
