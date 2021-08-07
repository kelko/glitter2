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
    #[snafu(display("Could not access file: {}", source))]
    FileIOError {
        source: std::io::Error,
        backtrace: Backtrace,
    },
    #[snafu(display("Input is not valid YAML: {}", source))]
    YamlError {
        source: yaml_rust::ScanError,
        backtrace: Backtrace,
    },
    #[snafu(display("Input is not valid glitter configuration"))]
    ConfigError { backtrace: Backtrace },
    #[snafu(display("Invalid var declaration: {}", yaml))]
    InvalidVarDeclaration { yaml: String, backtrace: Backtrace },
    #[snafu(display("Template Definition invalid"))]
    InvalidTemplateError { backtrace: Backtrace },
    #[snafu(display("Injection missing or invalid"))]
    InvalidInjectionError { backtrace: Backtrace },
}

impl TryFrom<&Yaml> for RawValue {
    type Error = ();

    fn try_from(value: &Yaml) -> Result<RawValue, Self::Error> {
        match value {
            Yaml::String(string_value) => Ok(RawValue::String(string_value.clone())),
            Yaml::Boolean(bool_value) => Ok(RawValue::Boolean(bool_value.clone())),
            Yaml::Integer(int_value) => Ok(RawValue::Integer(int_value.clone())),
            Yaml::Real(real_as_string) => Ok(RawValue::Float(real_as_string.clone())),
            _ => Err(()),
        }
    }
}

impl TryFrom<&Yaml> for ValueDefinition {
    type Error = ConfigReadError;

    fn try_from(var_declaration: &Yaml) -> Result<ValueDefinition, Self::Error> {
        if let Ok(raw_value) = RawValue::try_from(&var_declaration["value"]) {
            return Ok(ValueDefinition::Value(raw_value));
        }

        if let Yaml::Hash(var_hash) = &var_declaration["children"] {
            return Ok(ValueDefinition::Object(
                ConfigReader::read_var_declarations(var_hash)?,
            ));
        }

        if let Yaml::String(var_path) = &var_declaration["variable"] {
            return Ok(ValueDefinition::Variable(var_path.clone()));
        }

        if let Yaml::String(file_path) = &var_declaration["load"] {
            let parameter = if let Yaml::Hash(var_hash) = &var_declaration["parameter"] {
                ConfigReader::read_var_declarations(var_hash)?
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
                ConfigReader::read_var_declarations(var_hash)?
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

        let mut yaml_as_string = String::new();
        let mut emitter = yaml_rust::emitter::YamlEmitter::new(&mut yaml_as_string);
        emitter.dump(&var_declaration).unwrap();
        InvalidVarDeclaration {
            yaml: yaml_as_string,
        }
        .fail()
    }
}

impl TryFrom<&Yaml> for TemplateValue {
    type Error = ConfigReadError;

    fn try_from(value: &Yaml) -> Result<TemplateValue, Self::Error> {
        if let Yaml::String(raw_value) = &value["value"] {
            return Ok(TemplateValue::RawValue(raw_value.clone()));
        }

        if let Yaml::String(file_path) = &value["quote"] {
            return Ok(TemplateValue::Quote(file_path.clone()));
        }

        InvalidTemplateError {}.fail()
    }
}

pub struct ConfigReader {}

impl ConfigReader {
    pub fn new() -> ConfigReader {
        ConfigReader {}
    }

    pub fn read<T: BufRead>(&self, input: &mut T) -> Result<GlitterConfig, ConfigReadError> {
        let mut buffer = String::new();
        input.read_to_string(&mut buffer).context(FileIOError)?;

        let yaml_stream = YamlLoader::load_from_str(&buffer).context(YamlError)?;
        let yaml_content = &yaml_stream[0];

        let global: VariableDefinitionBlock =
            if let Yaml::Hash(global_hash) = &yaml_content["global"] {
                Self::read_var_declarations(global_hash)?
            } else {
                VariableDefinitionBlock::new()
            };

        let local: VariableDefinitionBlock = if let Yaml::Hash(local_hash) = &yaml_content["local"]
        {
            Self::read_var_declarations(local_hash)?
        } else {
            VariableDefinitionBlock::new()
        };

        let injection: Vec<VariableDefinitionBlock> =
            if let Yaml::Array(array) = &yaml_content["injection"] {
                Self::read_injections(array)?
            } else {
                InvalidInjectionError {}.fail()?
            };

        let template = match &yaml_content["template"] {
            Yaml::String(simple_template) => {
                TemplateDefinition::simple_template(simple_template.to_owned())
            }
            Yaml::Hash(_) => Self::read_hbf_template(&yaml_content["template"])?,
            _ => InvalidTemplateError {}.fail()?,
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
                variable_block_list.push(Self::read_var_declarations(hash)?);
            } else {
                InvalidInjectionError {}.fail()?
            }
        }

        Ok(variable_block_list)
    }

    pub(crate) fn read_var_declarations(
        var_declaration_block: &Hash,
    ) -> Result<VariableDefinitionBlock, ConfigReadError> {
        let mut variable_block = VariableDefinitionBlock::new();

        for hash_key in var_declaration_block.keys() {
            let var_declaration = ValueDefinition::try_from(&var_declaration_block[hash_key])?;
            variable_block.insert(hash_key.as_str().unwrap().to_owned(), var_declaration);
        }

        Ok(variable_block)
    }

    fn read_hbf_template(template: &Yaml) -> Result<TemplateDefinition, ConfigReadError> {
        let header = if let Ok(template) = TemplateValue::try_from(&template["header"]) {
            Some(template)
        } else {
            None
        };

        let footer = if let Ok(template) = TemplateValue::try_from(&template["footer"]) {
            Some(template)
        } else {
            None
        };

        let body = TemplateValue::try_from(&template["body"])?;

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
        input.read_to_string(&mut buffer).context(FileIOError)?;

        let yaml_stream = YamlLoader::load_from_str(&buffer).context(YamlError)?;
        let yaml_content = &yaml_stream[0];

        if let Yaml::Hash(hash) = yaml_content {
            return Ok(Self::read_var_declarations(hash)?);
        }

        InvalidInjectionError {}.fail()?
    }
}
