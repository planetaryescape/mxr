use crate::ast::*;
use chrono::NaiveDate;
use thiserror::Error;

// -- Tokens -------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Word(String),
    Phrase(String),
    Colon,
    Minus,
    LParen,
    RParen,
    And,
    Or,
    Not,
}

// -- Errors -------------------------------------------------------------------

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("unexpected end of input")]
    UnexpectedEnd,
    #[error("unexpected token: {0:?}")]
    UnexpectedToken(String),
    #[error("unmatched parenthesis")]
    UnmatchedParen,
    #[error("expected value after field")]
    ExpectedValue,
    #[error("unknown filter: {0}")]
    UnknownFilter(String),
    #[error("invalid date: {0}")]
    InvalidDate(String),
}

// -- Tokenizer ----------------------------------------------------------------

fn tokenize(input: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }
            '(' => {
                chars.next();
                tokens.push(Token::LParen);
            }
            ')' => {
                chars.next();
                tokens.push(Token::RParen);
            }
            ':' => {
                chars.next();
                tokens.push(Token::Colon);
            }
            '-' => {
                chars.next();
                tokens.push(Token::Minus);
            }
            '"' => {
                chars.next();
                let mut s = String::new();
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some(c) => s.push(c),
                        None => break,
                    }
                }
                tokens.push(Token::Phrase(s));
            }
            _ => {
                let mut word = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || c == '(' || c == ')' || c == ':' || c == '"' {
                        break;
                    }
                    word.push(c);
                    chars.next();
                }
                match word.as_str() {
                    "AND" => tokens.push(Token::And),
                    "OR" => tokens.push(Token::Or),
                    "NOT" => tokens.push(Token::Not),
                    _ => tokens.push(Token::Word(word)),
                }
            }
        }
    }

    Ok(tokens)
}

// -- Parser -------------------------------------------------------------------

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let tok = self.tokens.get(self.pos).cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    /// Top-level: parse_expression handles implicit AND between atoms
    fn parse_expression(&mut self) -> Result<QueryNode, ParseError> {
        let mut left = self.parse_or()?;

        while !self.at_end() {
            // Stop if we see a closing paren (handled by caller)
            if matches!(self.peek(), Some(Token::RParen)) {
                break;
            }
            // Stop if next is OR (handled by parse_or caller)
            if matches!(self.peek(), Some(Token::Or)) {
                break;
            }
            // Consume optional AND keyword
            if matches!(self.peek(), Some(Token::And)) {
                self.next();
            }
            if self.at_end() || matches!(self.peek(), Some(Token::RParen | Token::Or)) {
                break;
            }
            let right = self.parse_or()?;
            left = QueryNode::And(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_or(&mut self) -> Result<QueryNode, ParseError> {
        let mut left = self.parse_unary()?;

        while matches!(self.peek(), Some(Token::Or)) {
            self.next(); // consume OR
            let right = self.parse_unary()?;
            left = QueryNode::Or(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<QueryNode, ParseError> {
        match self.peek() {
            Some(Token::Minus) => {
                self.next();
                let node = self.parse_atom()?;
                Ok(QueryNode::Not(Box::new(node)))
            }
            Some(Token::Not) => {
                self.next();
                let node = self.parse_atom()?;
                Ok(QueryNode::Not(Box::new(node)))
            }
            _ => self.parse_atom(),
        }
    }

    fn parse_atom(&mut self) -> Result<QueryNode, ParseError> {
        match self.peek() {
            Some(Token::LParen) => {
                self.next(); // consume (
                let node = self.parse_expression()?;
                match self.next() {
                    Some(Token::RParen) => Ok(node),
                    _ => Err(ParseError::UnmatchedParen),
                }
            }
            Some(Token::Phrase(s)) => {
                let s = s.clone();
                self.next();
                Ok(QueryNode::Phrase(s))
            }
            Some(Token::Word(_)) => {
                // Check if this is a field:value pattern
                if self.pos + 1 < self.tokens.len()
                    && matches!(self.tokens[self.pos + 1], Token::Colon)
                {
                    return self.parse_field_value();
                }
                let word = match self.next() {
                    Some(Token::Word(w)) => w,
                    _ => unreachable!(),
                };
                Ok(QueryNode::Text(word))
            }
            Some(tok) => Err(ParseError::UnexpectedToken(format!("{:?}", tok))),
            None => Err(ParseError::UnexpectedEnd),
        }
    }

    fn parse_field_value(&mut self) -> Result<QueryNode, ParseError> {
        let field_name = match self.next() {
            Some(Token::Word(w)) => w,
            _ => return Err(ParseError::UnexpectedEnd),
        };

        // consume colon
        match self.next() {
            Some(Token::Colon) => {}
            _ => return Err(ParseError::ExpectedValue),
        }

        let value = match self.next() {
            Some(Token::Word(w)) => w,
            Some(Token::Phrase(p)) => p,
            _ => return Err(ParseError::ExpectedValue),
        };

        match field_name.to_lowercase().as_str() {
            "from" => Ok(QueryNode::Field {
                field: QueryField::From,
                value,
            }),
            "to" => Ok(QueryNode::Field {
                field: QueryField::To,
                value,
            }),
            "subject" => Ok(QueryNode::Field {
                field: QueryField::Subject,
                value,
            }),
            "label" => Ok(QueryNode::Label(value)),
            "is" => match value.to_lowercase().as_str() {
                "unread" => Ok(QueryNode::Filter(FilterKind::Unread)),
                "read" => Ok(QueryNode::Filter(FilterKind::Read)),
                "starred" => Ok(QueryNode::Filter(FilterKind::Starred)),
                other => Err(ParseError::UnknownFilter(other.to_string())),
            },
            "has" => match value.to_lowercase().as_str() {
                "attachment" | "attachments" => Ok(QueryNode::Filter(FilterKind::HasAttachment)),
                other => Err(ParseError::UnknownFilter(other.to_string())),
            },
            "after" => {
                let date = parse_date_value(&value)?;
                Ok(QueryNode::DateRange {
                    bound: DateBound::After,
                    date,
                })
            }
            "before" => {
                let date = parse_date_value(&value)?;
                Ok(QueryNode::DateRange {
                    bound: DateBound::Before,
                    date,
                })
            }
            "date" => {
                let date = parse_date_value(&value)?;
                Ok(QueryNode::DateRange {
                    bound: DateBound::Exact,
                    date,
                })
            }
            other => Err(ParseError::UnknownFilter(other.to_string())),
        }
    }
}

fn parse_date_value(s: &str) -> Result<DateValue, ParseError> {
    match s.to_lowercase().as_str() {
        "today" => Ok(DateValue::Today),
        "yesterday" => Ok(DateValue::Yesterday),
        "this-week" => Ok(DateValue::ThisWeek),
        "this-month" => Ok(DateValue::ThisMonth),
        _ => {
            let date = NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map_err(|_| ParseError::InvalidDate(s.to_string()))?;
            Ok(DateValue::Specific(date))
        }
    }
}

// -- Public API ---------------------------------------------------------------

pub fn parse_query(input: &str) -> Result<QueryNode, ParseError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ParseError::UnexpectedEnd);
    }
    let tokens = tokenize(input)?;
    if tokens.is_empty() {
        return Err(ParseError::UnexpectedEnd);
    }
    let mut parser = Parser::new(tokens);
    let node = parser.parse_expression()?;
    if !parser.at_end() && matches!(parser.peek(), Some(Token::RParen)) {
        return Err(ParseError::UnmatchedParen);
    }
    Ok(node)
}

// -- Tests --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn parse_single_word() {
        let result = parse_query("deployment").unwrap();
        assert_eq!(result, QueryNode::Text("deployment".to_string()));
    }

    #[test]
    fn parse_phrase() {
        let result = parse_query("\"deployment plan\"").unwrap();
        assert_eq!(result, QueryNode::Phrase("deployment plan".to_string()));
    }

    #[test]
    fn parse_from_field() {
        let result = parse_query("from:alice@example.com").unwrap();
        assert_eq!(
            result,
            QueryNode::Field {
                field: QueryField::From,
                value: "alice@example.com".to_string(),
            }
        );
    }

    #[test]
    fn parse_to_field() {
        let result = parse_query("to:bob").unwrap();
        assert_eq!(
            result,
            QueryNode::Field {
                field: QueryField::To,
                value: "bob".to_string(),
            }
        );
    }

    #[test]
    fn parse_subject_field() {
        let result = parse_query("subject:invoice").unwrap();
        assert_eq!(
            result,
            QueryNode::Field {
                field: QueryField::Subject,
                value: "invoice".to_string(),
            }
        );
    }

    #[test]
    fn parse_is_unread() {
        let result = parse_query("is:unread").unwrap();
        assert_eq!(result, QueryNode::Filter(FilterKind::Unread));
    }

    #[test]
    fn parse_is_starred() {
        let result = parse_query("is:starred").unwrap();
        assert_eq!(result, QueryNode::Filter(FilterKind::Starred));
    }

    #[test]
    fn parse_has_attachment() {
        let result = parse_query("has:attachment").unwrap();
        assert_eq!(result, QueryNode::Filter(FilterKind::HasAttachment));
    }

    #[test]
    fn parse_label() {
        let result = parse_query("label:work").unwrap();
        assert_eq!(result, QueryNode::Label("work".to_string()));
    }

    #[test]
    fn parse_date_after() {
        let result = parse_query("after:2026-01-01").unwrap();
        assert_eq!(
            result,
            QueryNode::DateRange {
                bound: DateBound::After,
                date: DateValue::Specific(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
            }
        );
    }

    #[test]
    fn parse_date_before() {
        let result = parse_query("before:2026-03-15").unwrap();
        assert_eq!(
            result,
            QueryNode::DateRange {
                bound: DateBound::Before,
                date: DateValue::Specific(NaiveDate::from_ymd_opt(2026, 3, 15).unwrap()),
            }
        );
    }

    #[test]
    fn parse_date_today() {
        let result = parse_query("date:today").unwrap();
        assert_eq!(
            result,
            QueryNode::DateRange {
                bound: DateBound::Exact,
                date: DateValue::Today,
            }
        );
    }

    #[test]
    fn parse_implicit_and() {
        let result = parse_query("invoice unread").unwrap();
        assert_eq!(
            result,
            QueryNode::And(
                Box::new(QueryNode::Text("invoice".to_string())),
                Box::new(QueryNode::Text("unread".to_string())),
            )
        );
    }

    #[test]
    fn parse_explicit_and() {
        let result = parse_query("invoice AND unread").unwrap();
        assert_eq!(
            result,
            QueryNode::And(
                Box::new(QueryNode::Text("invoice".to_string())),
                Box::new(QueryNode::Text("unread".to_string())),
            )
        );
    }

    #[test]
    fn parse_or() {
        let result = parse_query("invoice OR receipt").unwrap();
        assert_eq!(
            result,
            QueryNode::Or(
                Box::new(QueryNode::Text("invoice".to_string())),
                Box::new(QueryNode::Text("receipt".to_string())),
            )
        );
    }

    #[test]
    fn parse_not() {
        let result = parse_query("-spam").unwrap();
        assert_eq!(
            result,
            QueryNode::Not(Box::new(QueryNode::Text("spam".to_string())))
        );

        let result = parse_query("NOT spam").unwrap();
        assert_eq!(
            result,
            QueryNode::Not(Box::new(QueryNode::Text("spam".to_string())))
        );
    }

    #[test]
    fn parse_parentheses() {
        let result = parse_query("(from:alice OR from:bob) is:unread").unwrap();
        assert_eq!(
            result,
            QueryNode::And(
                Box::new(QueryNode::Or(
                    Box::new(QueryNode::Field {
                        field: QueryField::From,
                        value: "alice".to_string(),
                    }),
                    Box::new(QueryNode::Field {
                        field: QueryField::From,
                        value: "bob".to_string(),
                    }),
                )),
                Box::new(QueryNode::Filter(FilterKind::Unread)),
            )
        );
    }

    #[test]
    fn parse_compound() {
        let result = parse_query("from:alice subject:invoice is:unread after:2026-01-01").unwrap();
        // Should be nested And: And(And(And(from, subject), filter), date)
        assert_eq!(
            result,
            QueryNode::And(
                Box::new(QueryNode::And(
                    Box::new(QueryNode::And(
                        Box::new(QueryNode::Field {
                            field: QueryField::From,
                            value: "alice".to_string(),
                        }),
                        Box::new(QueryNode::Field {
                            field: QueryField::Subject,
                            value: "invoice".to_string(),
                        }),
                    )),
                    Box::new(QueryNode::Filter(FilterKind::Unread)),
                )),
                Box::new(QueryNode::DateRange {
                    bound: DateBound::After,
                    date: DateValue::Specific(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
                }),
            )
        );
    }
}
