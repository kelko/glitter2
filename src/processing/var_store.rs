use std::rc::Rc;

use crate::config::model::ValueDefinition;
use crate::config::model::{RawValue, ValueDefinitionList};
use crate::processing::{ProcessingContext, ValuePath, VariableDefinitionBlock};

pub(crate) enum ProcessingInstruction {
    Load(String, VariableDefinitionBlock),
    Render(String, VariableDefinitionBlock),
    //execute a binary file and return the stdout:
    Execute(String, ValueDefinitionList),
    //import a YAML file and return the content:
    Import(String),
    Quote(String),
    //TODO: Select(Vec<CaseClause>),
}

pub(crate) enum StoredVariable {
    Value(RawValue),
    Instruction(ProcessingInstruction, ValuePath),
    LocalReference(ValuePath),
    DistantReference(ValuePath, Rc<ProcessingContext>),
    Missing,
}

impl StoredVariable {
    fn out_of_block(block: &VariableDefinitionBlock, key_path: &mut ValuePath) -> Self {
        if key_path.is_empty() {
            return Self::Missing;
        }
        let key = &key_path[0];

        if let Some(value_definition) = block.get(key) {
            key_path.drop_first();
            Self::from(value_definition, key_path)
        } else {
            Self::Missing
        }
    }

    pub(crate) fn from(definition: &ValueDefinition, key_path: &mut ValuePath) -> Self {
        match definition {
            ValueDefinition::Value(value) => StoredVariable::Value(value.clone()),
            ValueDefinition::Object(map) => StoredVariable::out_of_block(map, key_path),
            ValueDefinition::Variable(value_path) => {
                let mut path_parts = ValuePath::from(value_path);
                path_parts.append(key_path);
                StoredVariable::LocalReference(path_parts)
            }
            ValueDefinition::Quote(file_path) => StoredVariable::Instruction(
                ProcessingInstruction::Quote(file_path.clone()),
                key_path.clone(),
            ),
            ValueDefinition::Import(file_path) => StoredVariable::Instruction(
                ProcessingInstruction::Import(file_path.clone()),
                key_path.clone(),
            ),
            ValueDefinition::Load(load_statement) => StoredVariable::Instruction(
                ProcessingInstruction::Load(
                    load_statement.file.clone(),
                    load_statement.parameter.clone(),
                ),
                key_path.clone(),
            ),
            ValueDefinition::Render(load_statement) => StoredVariable::Instruction(
                ProcessingInstruction::Render(
                    load_statement.file.clone(),
                    load_statement.parameter.clone(),
                ),
                key_path.clone(),
            ),
            ValueDefinition::Execute(execute_statement) => StoredVariable::Instruction(
                ProcessingInstruction::Execute(
                    execute_statement.executable.clone(),
                    execute_statement.arguments.clone(),
                ),
                key_path.clone(),
            ),
        }
    }
}

pub struct VariableStore {
    inner_store: VariableDefinitionBlock,
    parameter: VariableDefinitionBlock,
    source_context: Option<Rc<ProcessingContext>>,
}

impl VariableStore {
    pub(crate) fn with_context(
        variable_definition: VariableDefinitionBlock,
        parameter: VariableDefinitionBlock,
        source_context: Rc<ProcessingContext>,
    ) -> Self {
        VariableStore {
            inner_store: variable_definition,
            parameter,
            source_context: Some(source_context),
        }
    }

    pub(crate) fn contains(&self, key: &String) -> bool {
        self.inner_store.get(key).is_some()
            || (self.source_context.is_some() && self.parameter.get(key).is_some())
    }

    pub(crate) fn resolve(&self, key_path: &mut ValuePath) -> StoredVariable {
        match StoredVariable::out_of_block(&self.inner_store, key_path) {
            StoredVariable::Missing => (),
            v => return v,
        }

        if let Some(source_context) = &self.source_context {
            match StoredVariable::out_of_block(&self.parameter, key_path) {
                StoredVariable::Missing => (),
                StoredVariable::LocalReference(path_parts) => {
                    return StoredVariable::DistantReference(path_parts, Rc::clone(source_context))
                }
                v => return v,
            };
        }

        StoredVariable::Missing
    }

    #[inline(always)]
    pub(crate) fn insert<T: Into<String>>(mut self, key: T, value: ValueDefinition) -> Self {
        self.inner_store.insert(key.into(), value);

        self
    }
}

impl From<VariableDefinitionBlock> for VariableStore {
    fn from(variable_definition: VariableDefinitionBlock) -> Self {
        VariableStore {
            inner_store: variable_definition,
            parameter: VariableDefinitionBlock::new(),
            source_context: None,
        }
    }
}
