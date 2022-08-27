use peg::parser;

use super::{Expression, MatchExpression, RangeExpression, ValueExpression};
use crate::config::model::RawValue;
use crate::processing::ValuePath;

parser! {
  pub(crate) grammar grammar() for str {
        // basic types
        rule whitespace()
            = quiet!{[' ' | '\t']+}
        rule integer() -> i64
            = n:$(("-" / "+")? ['0'..='9']+) { n.parse().unwrap() }
        rule float_string() -> &'input str
            = f:$(("-" / "+")? ['0'..='9']+ "." ['0'..='9']+ ("E"/"e" "-"? ['0'..='9']+)) { f }
        rule float_numeric() -> f64
            = n:$(("-" / "+")? ['0'..='9']+) { n.parse().unwrap() }
            / s:(float_string()) { s.parse().unwrap() }
        rule identifier() -> &'input str
            = i:$(['a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' ]+) { i }
        rule string_value() -> &'input str
            = "\"" whitespace()? s:$([^'"']+) "\"" { s.trim() }
            / "'" whitespace()? s:$([^'\'']+) "'" { s.trim() }
            / "?" whitespace()? s:$([^'?']+) "?" { s.trim() }

        // compound type, but potentially general purpose
        pub(crate) rule range_expression() -> RangeExpression
            = f_i:$(['['|'(']) whitespace()? f:(float_numeric()) whitespace()? "," whitespace()? t:(float_numeric()) whitespace()? t_i:$([']'|')']) { RangeExpression { include_from: f_i == "[", from: f, include_to: f_i == "]", to: t} }

        // specific for "valma"
        rule raw_value() -> RawValue
            = i:integer() { RawValue::Integer(i) }
            / "true" { RawValue::Boolean(true) }
            / "false" { RawValue::Boolean(false)}
            / ("null"/"None"/"␀"/"∅") { RawValue::None }
            / f:(float_string()) { RawValue::Float(String::from(f)) }
            / s:(string_value()) { RawValue::String(String::from(s)) }
        pub(crate) rule variable_path() -> ValuePath
            = p:(identifier() ++ ".") { todo!() }
        rule value_expression() -> ValueExpression
            = ("GET"/"$") "{" whitespace() v:(variable_path()) whitespace()? "}" { ValueExpression::Get(v) }
            / ("ADD"/"+"/"➕") "{" l:(value_expression() ++ whitespace()) whitespace()? "}" { ValueExpression::Add(l) }
            / ("SUB"/"-"/"➖") "{" l:(value_expression() ++ whitespace()) whitespace()? "}" { ValueExpression::Subtract(l)  }
            / ("MUL"/"*"/"×"/"✖️") "{" l:(value_expression() ++ whitespace()) whitespace()? "}" { ValueExpression::Multiply(l)  }
            / ("DIV"/"/"/"÷"/"∕"/"➗") "{" l:(value_expression() ++ whitespace()) whitespace()? "}" { ValueExpression::Divide(l) }
            / ("PWR"/"🔌"/"🔋") "{" f:(value_expression()) whitespace() s:(value_expression()) whitespace()? "}" { ValueExpression::Power(Box::new(f), Box::new(s)) }
            / ("ROOT"/"√") "{" f:(value_expression()) whitespace() s:(value_expression()) whitespace()? "}" { ValueExpression::Root(Box::new(f), Box::new(s)) }
            / r:(raw_value()) { ValueExpression::Raw(r) }
        rule match_expression() -> MatchExpression
            = ("EQUAL"/"=") "{" whitespace() f:(valma_expression()) whitespace() s:(valma_expression()) whitespace()? "}" { MatchExpression::Equal(Box::new(f), Box::new(s)) }
            / ("DIFFERENT"/"!="/"≠") "{" whitespace() f:(valma_expression()) whitespace() s:(valma_expression()) whitespace()? "}" { MatchExpression::NotEqual(Box::new(f), Box::new(s)) }
            / ("GREATER"/">") "{" whitespace() f:(valma_expression()) whitespace() s:(valma_expression()) whitespace()?"}" { MatchExpression::Greater(Box::new(f), Box::new(s)) }
            / ("GREATER-THEN"/">="/"≥") "{" whitespace() f:(valma_expression()) whitespace() s:(valma_expression()) whitespace()? "}" { MatchExpression::GreaterEqual(Box::new(f), Box::new(s)) }
            / ("LESS"/"<") "{" whitespace() f:(valma_expression()) whitespace() s:(valma_expression()) whitespace()? "}" { MatchExpression::Less(Box::new(f), Box::new(s)) }
            / ("LESS-THEN"/"<="/"≤") "{" whitespace() f:(valma_expression()) whitespace() s:(valma_expression()) whitespace()? "}" { MatchExpression::LessEqual(Box::new(f), Box::new(s)) }
            / ("INSIDE"/"∈") "{" whitespace() f:(value_expression()) whitespace() r:(range_expression()) whitespace()? "}" { MatchExpression::Inside(Box::new(f), r) }
            / ("OUTSIDE"/"∉") "{" whitespace() f:(value_expression()) whitespace() r:(range_expression()) whitespace()? "}" { MatchExpression::Outside(Box::new(f), r) }
            / ("LIKE"/"👍") "{" whitespace() f:(value_expression()) whitespace() s:(string_value()) whitespace()? "}" { MatchExpression::Like(Box::new(f), String::from(s)) }
            / ("UNLIKE"/"👎") "{" whitespace() f:(value_expression()) whitespace() s:(string_value()) whitespace()? "}" { MatchExpression::Unlike(Box::new(f), String::from(s)) }
            / ("NOT"/"!"/"¬") "{" whitespace() m:(match_expression()) whitespace()? "}" { MatchExpression::Not(Box::new(m)) }
            / ("AND"/"∧") "{" l:(match_expression() ++ whitespace()) whitespace()? "}" { MatchExpression::And(l) }
            / ("OR"/"∨") "{" l:(match_expression() ++ whitespace()) whitespace()? "}" {MatchExpression::Or(l) }
            / ("EXIST"/"?"/"∃") "{" whitespace() v:(variable_path()) whitespace()? "}" { MatchExpression::Exist(v) }
            / ("NONE"/"∄") "{" whitespace() v:(variable_path()) whitespace()? "}" { MatchExpression::DoesntExist(v) }
        pub(crate) rule valma_expression() -> Expression
            = v:(value_expression()) { Expression::Value(v) }
            / m:(match_expression()) { Expression::Match(m) }
  }
}
