use std::convert::TryFrom;
use std::io::BufRead;

use snafu::{Backtrace, ResultExt, Snafu};
use yaml_rust::{yaml::Hash, Yaml, YamlLoader};

use crate::config::model::{
    GlitterConfig, LoadStatement, RawValue, RenderStatement, TemplateDefinition, TemplateValue,
    ValueDefinition, VariableDefinitionBlock,
};

#[derive(Debug, Snafu)]
pub enum ConfigReadError {
    #[snafu(display("Could not read input"))]
    InputIoError {
        #[snafu(source(from(std::io::Error, Box::new)))]
        source: Box<std::io::Error>,
        backtrace: Backtrace,
    },
    #[snafu(display("Invalid YAML as input"))]
    InvalidYaml {
        #[snafu(source(from(yaml_rust::ScanError, Box::new)))]
        source: Box<yaml_rust::ScanError>,
        backtrace: Backtrace,
    },
    #[snafu(display("Missing injection"))]
    InjectionMissing { backtrace: Backtrace },
    #[snafu(display("Invalid type for injection"))]
    InvalidTypeAtInjection { backtrace: Backtrace },
    #[snafu(display("Missing template"))]
    TemplateMissing { backtrace: Backtrace },
    #[snafu(display("Invalid type for template"))]
    InvalidTypeAtTemplate { backtrace: Backtrace },
    #[snafu(display("Invalid variable definition at template {}", section))]
    InvalidTemplateVarDefinition {
        section: &'static str,
        #[snafu(backtrace)]
        #[snafu(source(from(TemplateDefinitionError, Box::new)))]
        source: Box<TemplateDefinitionError>,
    },
    #[snafu(display("Empty load source"))]
    MissingLoadSource { backtrace: Backtrace },
    #[snafu(display("Wrong YAML type for load source:\n{}", yaml_source))]
    InvalidTypeAtLoadSource {
        yaml_source: String,
        backtrace: Backtrace,
    },
    #[snafu(display("Invalid variable definition block for {}", block))]
    InvalidVarDefinitionBlock {
        block: String,
        #[snafu(backtrace)]
        #[snafu(source(from(ConfigReadError, Box::new)))]
        source: Box<ConfigReadError>,
    },
    #[snafu(display("Variable name {} has an invalid type. Must be a string", key))]
    InvalidTypeAsVarName { key: String, backtrace: Backtrace },
    #[snafu(display("Invalid value definition for variable named {}", key))]
    InvalidValueDefinition {
        key: String,
        #[snafu(backtrace)]
        #[snafu(source(from(ValueDefinitionError, Box::new)))]
        source: Box<ValueDefinitionError>,
    },
}

#[derive(Debug, Snafu)]
pub enum RawValueError {
    #[snafu(display("Unsupported value type for a Raw Value"))]
    UnsupportedRawValueType { backtrace: Backtrace },
    #[snafu(display("Empty value for a Raw Value"))]
    EmptyRawValue { backtrace: Backtrace },
}

#[derive(Debug, Snafu)]
pub enum ValueDefinitionError {
    #[snafu(display("Invalid definition of a raw value:\n{}", yaml_source))]
    InvalidRawValue {
        yaml_source: String,
        #[snafu(backtrace)]
        #[snafu(source(from(RawValueError, Box::new)))]
        source: Box<RawValueError>,
    },
    #[snafu(display("Invalid sub-structure definition for a value of type {}", var_type))]
    InvalidSubDefinition {
        var_type: &'static str,
        #[snafu(backtrace)]
        #[snafu(source(from(ConfigReadError, Box::new)))]
        source: Box<ConfigReadError>,
    },
    #[snafu(display("Unknown/Unsupported Value definition:\n{}", yaml_source))]
    UnknownValueType {
        yaml_source: String,
        backtrace: Backtrace,
    },
}

#[derive(Debug, Snafu)]
pub enum TemplateDefinitionError {
    #[snafu(display("Unknown/Unsupported Variable definition:\n{}", yaml_source))]
    InvalidTemplateSubstructure {
        yaml_source: String,
        backtrace: Backtrace,
    },
}

impl TryFrom<&Yaml> for RawValue {
    type Error = RawValueError;

    fn try_from(value: &Yaml) -> Result<RawValue, Self::Error> {
        match value {
            Yaml::String(string_value) => Ok(RawValue::String(string_value.clone())),
            Yaml::Boolean(bool_value) => Ok(RawValue::Boolean(bool_value.clone())),
            Yaml::Integer(int_value) => Ok(RawValue::Integer(int_value.clone())),
            Yaml::Real(real_as_string) => Ok(RawValue::Float(real_as_string.clone())),
            Yaml::Null => EmptyRawValue {}.fail(),
            _ => UnsupportedRawValueType {}.fail(),
        }
    }
}

impl TryFrom<&Yaml> for ValueDefinition {
    type Error = ValueDefinitionError;

    fn try_from(var_declaration: &Yaml) -> Result<ValueDefinition, Self::Error> {
        let mut yaml_source = String::new();
        let mut emitter = yaml_rust::emitter::YamlEmitter::new(&mut yaml_source);
        if emitter.dump(&var_declaration).is_err() {
            yaml_source = String::from("(INVALID YAML)");
        }

        let value_declaration = &var_declaration["value"];

        if !value_declaration.is_null() && !value_declaration.is_badvalue() {
            let raw_value = RawValue::try_from(&var_declaration["value"])
                .context(InvalidRawValue { yaml_source })?;
            return Ok(ValueDefinition::Value(raw_value));
        }

        if let Yaml::Hash(var_hash) = &var_declaration["children"] {
            return Ok(ValueDefinition::Object(
                ConfigReader::read_var_declarations(var_hash)
                    .context(InvalidSubDefinition { var_type: "Object" })?,
            ));
        }

        if let Yaml::String(var_path) = &var_declaration["variable"] {
            return Ok(ValueDefinition::Variable(var_path.clone()));
        }

        if let Yaml::String(file_path) = &var_declaration["load"] {
            let parameter = if let Yaml::Hash(var_hash) = &var_declaration["parameter"] {
                ConfigReader::read_var_declarations(var_hash).context(InvalidSubDefinition {
                    var_type: "Load->Parameter",
                })?
            } else {
                VariableDefinitionBlock::new()
            };

            return Ok(ValueDefinition::Load(LoadStatement {
                file: file_path.clone(),
                parameter,
            }));
        }

        if let Yaml::String(file_path) = &var_declaration["render"] {
            let parameter = if let Yaml::Hash(var_hash) = &var_declaration["parameter"] {
                ConfigReader::read_var_declarations(var_hash).context(InvalidSubDefinition {
                    var_type: "Render->Parameter",
                })?
            } else {
                VariableDefinitionBlock::new()
            };

            return Ok(ValueDefinition::Render(RenderStatement {
                file: file_path.clone(),
                parameter,
            }));
        }

        if let Yaml::String(file_path) = &var_declaration["quote"] {
            return Ok(ValueDefinition::Quote(file_path.clone()));
        }

        if let Yaml::String(file_path) = &var_declaration["import"] {
            return Ok(ValueDefinition::Import(file_path.clone()));
        }

        // condition: uff ...

        UnknownValueType { yaml_source }.fail()
    }
}

impl TryFrom<&Yaml> for TemplateValue {
    type Error = TemplateDefinitionError;

    fn try_from(value: &Yaml) -> Result<TemplateValue, Self::Error> {
        if let Yaml::String(raw_value) = &value["value"] {
            return Ok(TemplateValue::RawValue(raw_value.clone()));
        }

        if let Yaml::String(file_path) = &value["quote"] {
            return Ok(TemplateValue::Quote(file_path.clone()));
        }

        let mut yaml_source = String::new();
        let mut emitter = yaml_rust::emitter::YamlEmitter::new(&mut yaml_source);
        if emitter.dump(&value).is_err() {
            yaml_source = String::from("(INVALID YAML)");
        }

        InvalidTemplateSubstructure { yaml_source }.fail()
    }
}

pub struct ConfigReader {}

impl ConfigReader {
    pub fn new() -> ConfigReader {
        ConfigReader {}
    }

    pub fn read<T: BufRead>(&self, input: &mut T) -> Result<GlitterConfig, ConfigReadError> {
        let mut buffer = String::new();
        input.read_to_string(&mut buffer).context(InputIoError)?;

        let yaml_stream = YamlLoader::load_from_str(&buffer).context(InvalidYaml)?;
        let yaml_content = &yaml_stream[0];

        let global: VariableDefinitionBlock =
            if let Yaml::Hash(global_hash) = &yaml_content["global"] {
                Self::read_var_declarations(global_hash)
                    .context(InvalidVarDefinitionBlock { block: "global" })?
            } else {
                VariableDefinitionBlock::new()
            };

        let local: VariableDefinitionBlock = if let Yaml::Hash(local_hash) = &yaml_content["local"]
        {
            Self::read_var_declarations(local_hash)
                .context(InvalidVarDefinitionBlock { block: "local" })?
        } else {
            VariableDefinitionBlock::new()
        };

        let injection: Vec<VariableDefinitionBlock> = match &yaml_content["injection"] {
            Yaml::Array(array) => Self::read_injections(array)?,
            Yaml::Null => return InjectionMissing {}.fail(),
            _ => return InvalidTypeAtInjection {}.fail(),
        };

        let template = match &yaml_content["template"] {
            Yaml::String(simple_template) => {
                TemplateDefinition::simple_template(simple_template.to_owned())
            }
            Yaml::Hash(_) => Self::read_hbf_template(&yaml_content["template"])?,
            Yaml::Null => return TemplateMissing {}.fail(),
            _ => InvalidTypeAtTemplate {}.fail()?,
        };

        Ok(GlitterConfig {
            global,
            local,
            injection,
            template,
        })
    }

    fn read_injections(
        injections: &Vec<Yaml>,
    ) -> Result<Vec<VariableDefinitionBlock>, ConfigReadError> {
        let mut variable_block_list = Vec::<VariableDefinitionBlock>::new();

        for single_injection in injections.iter() {
            if let Yaml::Hash(hash) = single_injection {
                variable_block_list.push(
                    Self::read_var_declarations(hash)
                        .context(InvalidVarDefinitionBlock { block: "injection" })?,
                );
            } else {
                InvalidTypeAtInjection {}.fail()?
            }
        }

        Ok(variable_block_list)
    }

    pub(crate) fn read_var_declarations(
        var_declaration_block: &Hash,
    ) -> Result<VariableDefinitionBlock, ConfigReadError> {
        let mut variable_block = VariableDefinitionBlock::new();

        for hash_key in var_declaration_block.keys() {
            let key_as_string = if let Yaml::String(string_value) = hash_key {
                string_value.clone()
            } else {
                return InvalidTypeAsVarName {
                    key: String::from(hash_key.as_str().unwrap_or("🤷")),
                }
                .fail();
            };

            let var_declaration = ValueDefinition::try_from(&var_declaration_block[hash_key])
                .context(InvalidValueDefinition {
                    key: key_as_string.clone(),
                })?;
            variable_block.insert(key_as_string, var_declaration);
        }

        Ok(variable_block)
    }

    fn read_hbf_template(template: &Yaml) -> Result<TemplateDefinition, ConfigReadError> {
        let header = if template["header"].is_null() {
            None
        } else {
            Some(
                TemplateValue::try_from(&template["header"])
                    .context(InvalidTemplateVarDefinition { section: "header" })?,
            )
        };

        let footer = if template["footer"].is_null() {
            None
        } else {
            Some(
                TemplateValue::try_from(&template["footer"])
                    .context(InvalidTemplateVarDefinition { section: "footer" })?,
            )
        };

        let body = TemplateValue::try_from(&template["body"])
            .context(InvalidTemplateVarDefinition { section: "body" })?;

        Ok(TemplateDefinition {
            header,
            body,
            footer,
        })
    }

    pub fn load<T: BufRead>(
        &self,
        input: &mut T,
    ) -> Result<VariableDefinitionBlock, ConfigReadError> {
        let mut buffer = String::new();
        input.read_to_string(&mut buffer).context(InputIoError)?;

        let yaml_stream = YamlLoader::load_from_str(&buffer).context(InvalidYaml)?;
        let yaml_content = &yaml_stream[0];

        match yaml_content {
            Yaml::Hash(hash) => Ok(Self::read_var_declarations(hash).context(
                InvalidVarDefinitionBlock {
                    block: "Load Source",
                },
            )?),
            Yaml::Null => MissingLoadSource {}.fail(),
            _ => InvalidTypeAtLoadSource {
                yaml_source: buffer,
            }
            .fail(),
        }
    }
}
