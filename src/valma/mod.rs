use crate::config::model::RawValue;
use crate::processing::ValuePath;

mod execute;
mod parse;
#[cfg(test)]
mod tests;

pub(crate) struct RangeExpression {
    include_from: bool,
    from: f64,
    include_to: bool,
    to: f64,
}

pub(crate) enum ValueExpression {
    Raw(RawValue),
    Get(ValuePath),
    Add(Vec<ValueExpression>),
    Subtract(Vec<ValueExpression>),
    Multiply(Vec<ValueExpression>),
    Divide(Vec<ValueExpression>),
    Power(Box<ValueExpression>, Box<ValueExpression>),
    Root(Box<ValueExpression>, Box<ValueExpression>),
}

pub(crate) enum MatchExpression {
    Equal(Box<Expression>, Box<Expression>),
    NotEqual(Box<Expression>, Box<Expression>),
    Greater(Box<Expression>, Box<Expression>),
    GreaterEqual(Box<Expression>, Box<Expression>),
    Less(Box<Expression>, Box<Expression>),
    LessEqual(Box<Expression>, Box<Expression>),
    Inside(Box<ValueExpression>, RangeExpression),
    Outside(Box<ValueExpression>, RangeExpression),
    Like(Box<ValueExpression>, String),
    Unlike(Box<ValueExpression>, String),
    Not(Box<MatchExpression>),
    And(Vec<MatchExpression>),
    Or(Vec<MatchExpression>),
}

pub(crate) enum Expression {
    Value(ValueExpression),
    Match(MatchExpression),
}
