use crate::config::model::{RawValue, ValueDefinition, VariableDefinitionBlock};
use snafu::{Backtrace, ResultExt, Snafu};
use std::io::BufRead;
use yaml_rust::{yaml::Hash, Yaml, YamlLoader};

#[derive(Debug, Snafu)]
pub enum YamlImportReadError {
    #[snafu(display("Could not access file"))]
    FileIoError {
        #[snafu(source(from(std::io::Error, Box::new)))]
        source: Box<std::io::Error>,
        backtrace: Backtrace,
    },
    #[snafu(display("Input is not valid YAML"))]
    YamlError {
        #[snafu(source(from(yaml_rust::ScanError, Box::new)))]
        source: Box<yaml_rust::ScanError>,
        backtrace: Backtrace,
    },
    #[snafu(display("Unsupported Input Type {}", value_type))]
    InvalidType {
        value_type: String,
        backtrace: Backtrace,
    },
}

pub(crate) struct YamlImporter {}

impl YamlImporter {
    pub fn new() -> Self {
        YamlImporter {}
    }

    pub fn read<T: BufRead>(
        &self,
        input: &mut T,
    ) -> Result<VariableDefinitionBlock, YamlImportReadError> {
        let mut buffer = String::new();
        input.read_to_string(&mut buffer).context(FileIoSnafu)?;

        let yaml_stream = YamlLoader::load_from_str(&buffer).context(YamlSnafu)?;
        if let Yaml::Hash(yaml_content) = &yaml_stream[0] {
            self.convert_hash(yaml_content)
        } else {
            todo!()
        }
    }

    pub fn convert_hash(
        &self,
        hash: &Hash,
    ) -> Result<VariableDefinitionBlock, YamlImportReadError> {
        let mut result = VariableDefinitionBlock::new();

        for hash_key in hash.keys() {
            let key_string = hash_key.as_str().unwrap().to_owned();

            match &hash[hash_key] {
                Yaml::Hash(sub_hash) => result.insert(
                    key_string,
                    ValueDefinition::Object(self.convert_hash(&sub_hash)?),
                ),
                Yaml::Integer(int_value) => result.insert(
                    key_string,
                    ValueDefinition::Value(RawValue::Integer(int_value.clone())),
                ),
                Yaml::String(string_value) => result.insert(
                    key_string,
                    ValueDefinition::Value(RawValue::String(string_value.clone())),
                ),
                Yaml::Boolean(bool_value) => result.insert(
                    key_string,
                    ValueDefinition::Value(RawValue::Boolean(bool_value.clone())),
                ),
                Yaml::Real(real_as_string) => result.insert(
                    key_string,
                    ValueDefinition::Value(RawValue::Float(real_as_string.clone())),
                ),
                Yaml::Array(_) => {
                    return InvalidTypeSnafu {
                        value_type: String::from("Array"),
                    }
                    .fail()
                }
                Yaml::Alias(_) => {
                    return InvalidTypeSnafu {
                        value_type: String::from("Alias"),
                    }
                    .fail()
                }
                Yaml::Null => {
                    return InvalidTypeSnafu {
                        value_type: String::from("Null"),
                    }
                    .fail()
                }
                Yaml::BadValue => {
                    return InvalidTypeSnafu {
                        value_type: String::from("BadValue"),
                    }
                    .fail()
                }
            };
        }
        Ok(result)
    }
}
