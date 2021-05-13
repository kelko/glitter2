use std::io::BufRead;
use snafu::{Backtrace, Snafu, ResultExt};
use yaml_rust::{YamlLoader, Yaml, yaml::Hash};
use crate::config::model::{VariableDefinitionBlock, ValueDefinition, RawValue};

#[derive(Debug, Snafu)]
pub enum ImportReadError {
    #[snafu(display("Could not access file"))]
    FileIOError {
        source: std::io::Error,
        backtrace: Backtrace,
    },
    #[snafu(display("Input is not valid YAML"))]
    YamlError {
        source: yaml_rust::ScanError,
        backtrace: Backtrace,
    }
}

pub(crate) struct YamlImporter {

}

impl YamlImporter {
    pub fn new() -> Self {
        YamlImporter {}
    }

    pub fn read<T: BufRead>(&self, input: &mut T) -> Result<VariableDefinitionBlock, ImportReadError> {
        let mut buffer = String::new();
        input.read_to_string(&mut buffer).context(FileIOError)?;

        let yaml_stream = YamlLoader::load_from_str(&buffer).context(YamlError)?;
        if let Yaml::Hash(yaml_content) = &yaml_stream[0] {
            self.convert_hash(yaml_content)

        } else {
            todo!()
        }
    }

    pub fn convert_hash(&self, hash: &Hash) -> Result<VariableDefinitionBlock, ImportReadError> {
        let mut result = VariableDefinitionBlock::new();

        for hash_key in hash.keys() {
            let key_string = hash_key.as_str().unwrap().to_owned();

            match &hash[hash_key] {
                Yaml::Hash(sub_hash) => result.insert(key_string, ValueDefinition::Object(self.convert_hash(&sub_hash)?)),
                Yaml::Integer(int_value) => result.insert(key_string, ValueDefinition::Value(RawValue::Integer(int_value.clone()))),
                Yaml::String(string_value) => result.insert(key_string, ValueDefinition::Value(RawValue::String(string_value.clone()))),
                Yaml::Boolean(bool_value) => result.insert(key_string, ValueDefinition::Value(RawValue::Boolean(bool_value.clone()))),
                Yaml::Real(real_as_string) => result.insert(key_string, ValueDefinition::Value(RawValue::Float(real_as_string.clone()))),
                _ => todo!()
            };
        }

        Ok(result)
    }
}
