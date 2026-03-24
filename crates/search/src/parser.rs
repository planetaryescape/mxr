use crate::mxr_search::ast::*;
use chrono::{Duration, Local, NaiveDate};
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
    #[error("invalid size: {0}")]
    InvalidSize(String),
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
            "cc" => Ok(QueryNode::Field {
                field: QueryField::Cc,
                value,
            }),
            "bcc" => Ok(QueryNode::Field {
                field: QueryField::Bcc,
                value,
            }),
            "subject" => Ok(QueryNode::Field {
                field: QueryField::Subject,
                value,
            }),
            "body" => Ok(QueryNode::Field {
                field: QueryField::Body,
                value,
            }),
            "filename" => Ok(QueryNode::Field {
                field: QueryField::Filename,
                value,
            }),
            "label" => Ok(QueryNode::Label(value)),
            "is" => match value.to_lowercase().as_str() {
                "unread" => Ok(QueryNode::Filter(FilterKind::Unread)),
                "read" => Ok(QueryNode::Filter(FilterKind::Read)),
                "starred" => Ok(QueryNode::Filter(FilterKind::Starred)),
                "draft" | "drafts" => Ok(QueryNode::Filter(FilterKind::Draft)),
                "sent" => Ok(QueryNode::Filter(FilterKind::Sent)),
                "trash" | "deleted" => Ok(QueryNode::Filter(FilterKind::Trash)),
                "spam" | "junk" => Ok(QueryNode::Filter(FilterKind::Spam)),
                "answered" | "replied" => Ok(QueryNode::Filter(FilterKind::Answered)),
                "inbox" => Ok(QueryNode::Filter(FilterKind::Inbox)),
                "archived" | "archive" => Ok(QueryNode::Filter(FilterKind::Archived)),
                other => Err(ParseError::UnknownFilter(other.to_string())),
            },
            "has" => match value.to_lowercase().as_str() {
                "attachment" | "attachments" => Ok(QueryNode::Filter(FilterKind::HasAttachment)),
                other => Err(ParseError::UnknownFilter(other.to_string())),
            },
            "size" => {
                let (op, bytes) = parse_size_value(&value)?;
                Ok(QueryNode::Size { op, bytes })
            }
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
            "older" => {
                let date = parse_relative_duration_date(&value)?;
                Ok(QueryNode::DateRange {
                    bound: DateBound::Before,
                    date: DateValue::Specific(date),
                })
            }
            "newer" => {
                let date = parse_relative_duration_date(&value)?;
                Ok(QueryNode::DateRange {
                    bound: DateBound::After,
                    date: DateValue::Specific(date),
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

fn parse_relative_duration_date(s: &str) -> Result<NaiveDate, ParseError> {
    let input = s.trim().to_lowercase();
    if input.len() < 2 {
        return Err(ParseError::InvalidDate(s.to_string()));
    }

    let (amount, unit) = input.split_at(input.len() - 1);
    let count = amount
        .parse::<i64>()
        .map_err(|_| ParseError::InvalidDate(s.to_string()))?;
    let days = match unit {
        "d" => count,
        "w" => count * 7,
        "m" => count * 30,
        "y" => count * 365,
        _ => return Err(ParseError::InvalidDate(s.to_string())),
    };

    Ok(Local::now().date_naive() - Duration::days(days))
}

fn parse_size_value(s: &str) -> Result<(SizeOp, u64), ParseError> {
    let input = s.trim().to_lowercase();
    if input.is_empty() {
        return Err(ParseError::InvalidSize(s.to_string()));
    }

    let (op, rest) = if let Some(rest) = input.strip_prefix(">=") {
        (SizeOp::GreaterThanOrEqual, rest)
    } else if let Some(rest) = input.strip_prefix("<=") {
        (SizeOp::LessThanOrEqual, rest)
    } else if let Some(rest) = input.strip_prefix('>') {
        (SizeOp::GreaterThan, rest)
    } else if let Some(rest) = input.strip_prefix('<') {
        (SizeOp::LessThan, rest)
    } else if let Some(rest) = input.strip_prefix('=') {
        (SizeOp::Equal, rest)
    } else {
        (SizeOp::Equal, input.as_str())
    };

    let number_end = rest
        .find(|ch: char| !ch.is_ascii_digit() && ch != '.')
        .unwrap_or(rest.len());
    let (number_part, unit_part) = rest.split_at(number_end);
    if number_part.is_empty() {
        return Err(ParseError::InvalidSize(s.to_string()));
    }

    let value = number_part
        .parse::<f64>()
        .map_err(|_| ParseError::InvalidSize(s.to_string()))?;
    let multiplier = match unit_part {
        "" | "b" => 1_f64,
        "k" | "kb" => 1024_f64,
        "m" | "mb" => 1024_f64 * 1024_f64,
        "g" | "gb" => 1024_f64 * 1024_f64 * 1024_f64,
        other => return Err(ParseError::InvalidSize(other.to_string())),
    };

    Ok((op, (value * multiplier).round() as u64))
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
    fn parse_cc_bcc_and_body_fields() {
        assert_eq!(
            parse_query("cc:alice@example.com").unwrap(),
            QueryNode::Field {
                field: QueryField::Cc,
                value: "alice@example.com".to_string(),
            }
        );
        assert_eq!(
            parse_query("bcc:hidden@example.com").unwrap(),
            QueryNode::Field {
                field: QueryField::Bcc,
                value: "hidden@example.com".to_string(),
            }
        );
        assert_eq!(
            parse_query("body:\"deploy canary\"").unwrap(),
            QueryNode::Field {
                field: QueryField::Body,
                value: "deploy canary".to_string(),
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
    fn parse_additional_is_filters() {
        assert_eq!(
            parse_query("is:sent").unwrap(),
            QueryNode::Filter(FilterKind::Sent)
        );
        assert_eq!(
            parse_query("is:draft").unwrap(),
            QueryNode::Filter(FilterKind::Draft)
        );
        assert_eq!(
            parse_query("is:trash").unwrap(),
            QueryNode::Filter(FilterKind::Trash)
        );
        assert_eq!(
            parse_query("is:spam").unwrap(),
            QueryNode::Filter(FilterKind::Spam)
        );
        assert_eq!(
            parse_query("is:answered").unwrap(),
            QueryNode::Filter(FilterKind::Answered)
        );
        assert_eq!(
            parse_query("is:inbox").unwrap(),
            QueryNode::Filter(FilterKind::Inbox)
        );
        assert_eq!(
            parse_query("is:archived").unwrap(),
            QueryNode::Filter(FilterKind::Archived)
        );
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
    fn parse_older_relative_duration() {
        let expected = Local::now().date_naive() - Duration::days(30);
        let result = parse_query("older:30d").unwrap();
        assert_eq!(
            result,
            QueryNode::DateRange {
                bound: DateBound::Before,
                date: DateValue::Specific(expected),
            }
        );
    }

    #[test]
    fn parse_newer_relative_duration() {
        let expected = Local::now().date_naive() - Duration::days(14);
        let result = parse_query("newer:2w").unwrap();
        assert_eq!(
            result,
            QueryNode::DateRange {
                bound: DateBound::After,
                date: DateValue::Specific(expected),
            }
        );
    }

    #[test]
    fn reject_invalid_relative_duration_unit() {
        let result = parse_query("older:30q");
        assert_eq!(result, Err(ParseError::InvalidDate("30q".to_string())));
    }

    #[test]
    fn parse_size_query() {
        assert_eq!(
            parse_query("size:>5mb").unwrap(),
            QueryNode::Size {
                op: SizeOp::GreaterThan,
                bytes: 5 * 1024 * 1024,
            }
        );
        assert_eq!(
            parse_query("size:<=42kb").unwrap(),
            QueryNode::Size {
                op: SizeOp::LessThanOrEqual,
                bytes: 42 * 1024,
            }
        );
    }

    #[test]
    fn reject_invalid_size_unit() {
        let result = parse_query("size:>5tb");
        assert_eq!(result, Err(ParseError::InvalidSize("tb".to_string())));
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
