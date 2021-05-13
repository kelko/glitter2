use std::rc::Rc;

use crate::config::model::RawValue;
use crate::processing::{split_value_path, VariableDefinitionBlock, ProcessingContext};

use crate::config::model::{ValueDefinition};

pub(crate) enum ProcessingInstruction {
    Load(String, VariableDefinitionBlock),
    Render(String, VariableDefinitionBlock),
    //Execute(ImportStatement), //execute a binary file and return the stdout
    Import(String), //import a YAML file and return the content
    Quote(String),
    //Select(Vec<CaseClause>),
}

pub(crate) enum StoredVariable {
    Value(RawValue),
    Instruction(ProcessingInstruction, Vec<String>),
    LocalReference(Vec<String>),
    DistantReference(Vec<String>, Rc<ProcessingContext>),
}

pub struct VariableStore {
    inner_store : VariableDefinitionBlock,
    parameter : VariableDefinitionBlock,
    source_context: Option<Rc<ProcessingContext>>,
}

impl VariableStore {
    pub(crate) fn with_context(variable_definition: VariableDefinitionBlock, parameter: VariableDefinitionBlock, source_context: Rc<ProcessingContext>) -> Self { 
        VariableStore { 
            inner_store: variable_definition,
            parameter,
            source_context: Some(source_context)
        }
    }

    pub(crate) fn contains(&self, key: &String) -> bool {
        self.inner_store.get(key).is_some() || (self.source_context.is_some() && self.parameter.get(key).is_some())
    }

    pub(crate) fn resolve(&self, key_path: Vec<String>) -> Result<StoredVariable,()> {
        if let Some(var) = Self::resolve_in_map(&self.inner_store, key_path.clone()) {
            return Ok(var);
        }
        
        if let Some(source_context) = &self.source_context {
            match Self::resolve_in_map(&self.parameter, key_path) {
                None => (),
                Some(StoredVariable::LocalReference(path_parts)) => return Ok(StoredVariable::DistantReference(path_parts, Rc::clone(source_context))),
                Some(v) => return Ok(v)
            };
        }

        Err(())
    }

    fn resolve_in_map(map: &VariableDefinitionBlock, mut key_path: Vec<String>) -> Option<StoredVariable> {
        if key_path.len() < 1 {
            return None;
        }
        let key = key_path.remove(0);

        match map.get(&key) {
            Some(ValueDefinition::Value(value)) => Some(StoredVariable::Value(value.clone())),
            Some(ValueDefinition::Object(map)) => Self::resolve_in_map(map, key_path),
            Some(ValueDefinition::Variable(value_path)) => {
                let path_parts = Self::new_value_path(split_value_path(value_path), key_path);

                Some(StoredVariable::LocalReference(path_parts))
            },
            Some(ValueDefinition::Quote(file_path)) => Some(
                StoredVariable::Instruction(
                    ProcessingInstruction::Quote(file_path.clone()),
                    key_path
                )
            ),
            Some(ValueDefinition::Import(file_path)) => Some(
                StoredVariable::Instruction(
                    ProcessingInstruction::Import(file_path.clone()),
                    key_path
                )
            ),
            Some(ValueDefinition::Load(load_statement)) => Some(
                StoredVariable::Instruction(
                    ProcessingInstruction::Load(load_statement.file.clone(), load_statement.parameter.clone()),
                    key_path
                )
            ),
            Some(ValueDefinition::Render(load_statement)) => Some(
                StoredVariable::Instruction(
                    ProcessingInstruction::Render(load_statement.file.clone(), load_statement.parameter.clone()),
                    key_path
                )
            ),
            Some(ValueDefinition::Select(_)) => todo!(),
            None => None,
        }
    }

    fn new_value_path(mut start: Vec<String>, mut rest: Vec<String>) -> Vec<String> {
        start.append(&mut rest);

        return start;
    }

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
