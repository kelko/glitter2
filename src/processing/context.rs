use crate::config::model::{
    RawValue, TemplateDefinition, ValueDefinition, VariableDefinitionBlock,
};
use crate::processing::var_store::VariableStore;
use std::path::Path;
use std::rc::Rc;

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
        let injection = Self::build_injection_store(injection_source, &filename, &directory);

        ProcessingContext {
            directory,
            local: Rc::new(VariableStore::from(local_source)),
            injection,
            template: Some(template_source),
        }
    }

    fn build_injection_store(
        injection_source: Vec<VariableDefinitionBlock>,
        filename: &String,
        directory: &String,
    ) -> Vec<Rc<VariableStore>> {
        let mut iteration_count = 0;
        injection_source
            .into_iter()
            .map(|i| {
                iteration_count += 1;

                let store = VariableStore::from(i)
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
                    );

                Rc::new(store)
            })
            .collect::<Vec<_>>()
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
        let injection = Self::build_injection_store(injection_source, &filename, &directory);

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
