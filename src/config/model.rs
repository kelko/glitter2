use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug)]
pub enum RawValue {
    Boolean(bool),
    Integer(i64),
    Float(String),
    String(String),
}

impl Display for RawValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RawValue::Boolean(bool) => bool.fmt(f),
            RawValue::Integer(int) => int.fmt(f),
            RawValue::Float(string) | RawValue::String(string) => string.fmt(f),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LoadStatement {
    pub file: String,
    pub parameter: VariableDefinitionBlock,
}

#[derive(Clone, Debug)]
pub struct RenderStatement {
    pub file: String,
    pub parameter: VariableDefinitionBlock,
}

#[derive(Clone, Debug)]
pub struct ExecuteStatement {
    pub executable: String,
    pub arguments: ValueDefinitionList,
}

#[derive(Clone, Debug)]
pub struct CaseClause {
    pub case: Option<String>,
    pub definition: ValueDefinition,
}

#[derive(Clone, Debug)]
pub enum ValueDefinition {
    Value(RawValue),
    Object(VariableDefinitionBlock),
    Variable(String),
    Load(LoadStatement),
    Render(RenderStatement),
    //execute a binary file and return the stdout:
    Execute(ExecuteStatement),
    //import a YAML file and return the content:
    Import(String),
    Quote(String),
    //TODO: Select(Vec<CaseClause>),
}

pub type VariableDefinitionBlock = BTreeMap<String, ValueDefinition>;
pub type ValueDefinitionList = Vec<ValueDefinition>;

#[derive(Clone, Debug)]
pub enum TemplateValue {
    RawValue(String),
    Quote(String),
}

#[derive(Clone, Debug)]
pub struct TemplateDefinition {
    pub header: Option<TemplateValue>,
    pub body: TemplateValue,
    pub footer: Option<TemplateValue>,
}

impl TemplateDefinition {
    pub fn simple_template(template: String) -> Self {
        Self {
            header: None,
            body: TemplateValue::RawValue(template),
            footer: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GlitterConfig {
    pub global: VariableDefinitionBlock,
    pub local: VariableDefinitionBlock,
    pub injection: Vec<VariableDefinitionBlock>,
    pub template: TemplateDefinition,
}
