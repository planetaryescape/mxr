//! Hand-written recursive-descent parser for Gmail-style email queries.
//!
//! The grammar (informal):
//!
//! ```text
//! query    = or ( (AND | implicit) or )*
//! or       = near ( OR near )*
//! near     = unary ( AROUND <n> unary )*
//! unary    = ('-' | NOT)? atom
//! atom     = '(' query ')'
//!          | '{' group '}'                ( OR-joined )
//!          | field ':' value
//!          | field ':' '(' field_group ')'
//!          | PHRASE
//!          | '+' WORD                     ( -> Exact )
//!          | WORD
//! ```

use crate::ast::{
    DateBound, DateValue, FilterKind, QueryField, QueryNode, RelativeUnit, SizeOp,
};
use crate::error::ParseError;
use crate::options::ParserOptions;

// -- Tokens -------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Word(String),
    Phrase(String),
    Colon,
    Minus,
    LParen,
    RParen,
    LBrace,
    RBrace,
    And,
    Or,
    Not,
    Around,
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
            '{' => {
                chars.next();
                tokens.push(Token::LBrace);
            }
            '}' => {
                chars.next();
                tokens.push(Token::RBrace);
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
                        Some('\\') => match chars.next() {
                            // Escaped inner double-quote: `"foo \"bar\""`.
                            Some('"') => s.push('"'),
                            Some(c) => {
                                s.push('\\');
                                s.push(c);
                            }
                            None => break,
                        },
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
                    if c.is_whitespace()
                        || c == '('
                        || c == ')'
                        || c == '{'
                        || c == '}'
                        || c == ':'
                        || c == '"'
                    {
                        break;
                    }
                    word.push(c);
                    chars.next();
                }
                if word.eq_ignore_ascii_case("AND") {
                    tokens.push(Token::And);
                } else if word.eq_ignore_ascii_case("OR") {
                    tokens.push(Token::Or);
                } else if word.eq_ignore_ascii_case("NOT") {
                    tokens.push(Token::Not);
                } else if word.eq_ignore_ascii_case("AROUND") {
                    tokens.push(Token::Around);
                } else {
                    tokens.push(Token::Word(word));
                }
            }
        }
    }

    Ok(tokens)
}

// -- Parser -------------------------------------------------------------------

struct Parser<'a> {
    tokens: Vec<Token>,
    pos: usize,
    options: &'a ParserOptions,
}

impl<'a> Parser<'a> {
    fn new(tokens: Vec<Token>, options: &'a ParserOptions) -> Self {
        Self {
            tokens,
            pos: 0,
            options,
        }
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

    /// Top-level entry: parse a full expression. OR has lower precedence
    /// than (implicit/explicit) AND, matching Gmail and Lucene
    /// conventions.
    fn parse_expression(&mut self) -> Result<QueryNode, ParseError> {
        self.parse_or()
    }

    /// `parse_or` chains OR over AND-groups.
    fn parse_or(&mut self) -> Result<QueryNode, ParseError> {
        let mut left = self.parse_and()?;

        while matches!(self.peek(), Some(Token::Or)) {
            self.next();
            let right = self.parse_and()?;
            left = QueryNode::Or(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    /// `parse_and` chains implicit and explicit AND between adjacent
    /// terms. Stops at OR or any closing delimiter.
    fn parse_and(&mut self) -> Result<QueryNode, ParseError> {
        let mut left = self.parse_near()?;

        while !self.at_end() {
            if matches!(
                self.peek(),
                Some(Token::RParen | Token::RBrace | Token::Or)
            ) {
                break;
            }
            if matches!(self.peek(), Some(Token::And)) {
                self.next();
            }
            if self.at_end()
                || matches!(
                    self.peek(),
                    Some(Token::RParen | Token::RBrace | Token::Or)
                )
            {
                break;
            }
            let right = self.parse_near()?;
            left = QueryNode::And(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_near(&mut self) -> Result<QueryNode, ParseError> {
        let mut left = self.parse_unary()?;

        while matches!(self.peek(), Some(Token::Around)) {
            self.next();
            let distance = match self.next() {
                Some(Token::Word(w)) => w
                    .parse::<u32>()
                    .map_err(|_| ParseError::UnexpectedToken(w.to_string()))?,
                Some(tok) => return Err(ParseError::UnexpectedToken(format!("{:?}", tok))),
                None => return Err(ParseError::UnexpectedEnd),
            };
            let right = self.parse_unary()?;
            let left_term = near_term(&left)?;
            let right_term = near_term(&right)?;
            left = QueryNode::Near {
                left: left_term,
                right: right_term,
                distance,
            };
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
                self.next();
                let node = self.parse_expression()?;
                match self.next() {
                    Some(Token::RParen) => Ok(node),
                    _ => Err(ParseError::UnmatchedParen),
                }
            }
            Some(Token::LBrace) => self.parse_brace_group(),
            Some(Token::Phrase(s)) => {
                let s = s.clone();
                self.next();
                phrase_node(s)
            }
            Some(Token::Word(_)) => {
                // Is this a field:value pattern?
                if self.pos + 1 < self.tokens.len()
                    && matches!(self.tokens[self.pos + 1], Token::Colon)
                {
                    return self.parse_field_value();
                }
                let word = match self.next() {
                    Some(Token::Word(w)) => w,
                    _ => unreachable!(),
                };
                // `+word` => Exact (no stemming).
                if let Some(rest) = word.strip_prefix('+') {
                    if !rest.is_empty() {
                        return Ok(QueryNode::Exact(rest.to_string()));
                    }
                }
                Ok(QueryNode::Text(word))
            }
            Some(tok) => Err(ParseError::UnexpectedToken(format!("{:?}", tok))),
            None => Err(ParseError::UnexpectedEnd),
        }
    }

    /// Brace groups OR-join their contents regardless of separators.
    /// Each iteration grabs one `parse_unary` (a single atom, possibly
    /// negated). We deliberately do *not* recurse into `parse_or` or
    /// `parse_and` here — the brace's implicit OR would conflict with
    /// parse_and's implicit AND if we did.
    fn parse_brace_group(&mut self) -> Result<QueryNode, ParseError> {
        self.next();
        let mut node: Option<QueryNode> = None;

        while !self.at_end() {
            if matches!(self.peek(), Some(Token::RBrace)) {
                self.next();
                return node.ok_or(ParseError::UnexpectedEnd);
            }
            if matches!(self.peek(), Some(Token::And | Token::Or)) {
                self.next();
                continue;
            }

            let part = self.parse_unary()?;
            node = Some(match node {
                Some(left) => QueryNode::Or(Box::new(left), Box::new(part)),
                None => part,
            });
        }

        Err(ParseError::UnmatchedBrace)
    }

    fn parse_field_value(&mut self) -> Result<QueryNode, ParseError> {
        let field_name = match self.next() {
            Some(Token::Word(w)) => w,
            _ => return Err(ParseError::UnexpectedEnd),
        };

        match self.next() {
            Some(Token::Colon) => {}
            _ => return Err(ParseError::ExpectedValue),
        }

        if matches!(self.peek(), Some(Token::LParen)) {
            return self.parse_field_group(&field_name);
        }

        let value = match self.next() {
            Some(Token::Word(w)) => w,
            Some(Token::Phrase(p)) => p,
            _ => return Err(ParseError::ExpectedValue),
        };

        self.build_field_value(&field_name, normalize_value(value))
    }

    fn parse_field_group(&mut self, field_name: &str) -> Result<QueryNode, ParseError> {
        self.next();
        let mut node: Option<QueryNode> = None;
        let mut use_or = false;

        while !self.at_end() {
            match self.peek() {
                Some(Token::RParen) => {
                    self.next();
                    return node.ok_or(ParseError::ExpectedValue);
                }
                Some(Token::And) => {
                    self.next();
                    use_or = false;
                }
                Some(Token::Or) => {
                    self.next();
                    use_or = true;
                }
                Some(Token::Minus | Token::Not) => {
                    self.next();
                    let value = self.next_field_group_value(field_name)?;
                    node = Some(combine_group_node(
                        node,
                        QueryNode::Not(Box::new(value)),
                        use_or,
                    ));
                    use_or = false;
                }
                _ => {
                    let value = self.next_field_group_value(field_name)?;
                    node = Some(combine_group_node(node, value, use_or));
                    use_or = false;
                }
            }
        }

        Err(ParseError::UnmatchedParen)
    }

    fn next_field_group_value(&mut self, field_name: &str) -> Result<QueryNode, ParseError> {
        let value = match self.next() {
            Some(Token::Word(w)) => normalize_value(w),
            Some(Token::Phrase(p)) => normalize_value(p),
            Some(tok) => return Err(ParseError::UnexpectedToken(format!("{:?}", tok))),
            None => return Err(ParseError::UnexpectedEnd),
        };
        self.build_field_value(field_name, value)
    }

    fn build_field_value(&self, field_name: &str, value: String) -> Result<QueryNode, ParseError> {
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
            "list" => Ok(QueryNode::Field {
                field: QueryField::List,
                value,
            }),
            "deliveredto" => Ok(QueryNode::Field {
                field: QueryField::DeliveredTo,
                value,
            }),
            "rfc822msgid" => Ok(QueryNode::Field {
                field: QueryField::Rfc822MsgId,
                value,
            }),
            "label" => Ok(QueryNode::Label(value)),
            "category" => category_label(&value)
                .map(|label| QueryNode::Label(label.to_string()))
                .ok_or_else(|| ParseError::UnknownFilter(value.to_lowercase())),
            "is" => self.build_is_filter(value),
            "in" => self.build_in_filter(value),
            "has" => self.build_has_filter(value),
            "size" => {
                let (op, bytes) = parse_size_value(&value)?;
                Ok(QueryNode::Size { op, bytes })
            }
            "larger" => {
                let bytes = parse_size_bytes(&value)?;
                Ok(QueryNode::Size {
                    op: SizeOp::GreaterThan,
                    bytes,
                })
            }
            "smaller" => {
                let bytes = parse_size_bytes(&value)?;
                Ok(QueryNode::Size {
                    op: SizeOp::LessThan,
                    bytes,
                })
            }
            "after" => Ok(QueryNode::DateRange {
                bound: DateBound::After,
                date: parse_date_value(&value)?,
            }),
            "before" => Ok(QueryNode::DateRange {
                bound: DateBound::Before,
                date: parse_date_value(&value)?,
            }),
            "date" => Ok(QueryNode::DateRange {
                bound: DateBound::Exact,
                date: parse_date_value(&value)?,
            }),
            "older" | "older_than" => Ok(QueryNode::DateRange {
                bound: DateBound::Before,
                date: parse_relative_duration(&value)?,
            }),
            "newer" | "newer_than" => Ok(QueryNode::DateRange {
                bound: DateBound::After,
                date: parse_relative_duration(&value)?,
            }),
            other => Err(ParseError::UnknownFilter(other.to_string())),
        }
    }

    fn build_is_filter(&self, value: String) -> Result<QueryNode, ParseError> {
        match value.to_lowercase().as_str() {
            "unread" => Ok(QueryNode::Filter(FilterKind::Unread)),
            "read" => Ok(QueryNode::Filter(FilterKind::Read)),
            "starred" => Ok(QueryNode::Filter(FilterKind::Starred)),
            "important" => Ok(QueryNode::Label("IMPORTANT".to_string())),
            "muted" => Ok(QueryNode::Label("MUTED".to_string())),
            "draft" | "drafts" => Ok(QueryNode::Filter(FilterKind::Draft)),
            "sent" => Ok(QueryNode::Filter(FilterKind::Sent)),
            "trash" | "deleted" => Ok(QueryNode::Filter(FilterKind::Trash)),
            "spam" | "junk" => Ok(QueryNode::Filter(FilterKind::Spam)),
            "answered" | "replied" => Ok(QueryNode::Filter(FilterKind::Answered)),
            "inbox" => Ok(QueryNode::Filter(FilterKind::Inbox)),
            "archived" | "archive" => Ok(QueryNode::Filter(FilterKind::Archived)),
            other => self.resolve_custom_filter(other),
        }
    }

    fn build_in_filter(&self, value: String) -> Result<QueryNode, ParseError> {
        match value.to_lowercase().as_str() {
            "inbox" => Ok(QueryNode::Filter(FilterKind::Inbox)),
            "anywhere" | "all" | "allmail" | "all_mail" => {
                Ok(QueryNode::Filter(FilterKind::Anywhere))
            }
            "draft" | "drafts" => Ok(QueryNode::Filter(FilterKind::Draft)),
            "sent" => Ok(QueryNode::Filter(FilterKind::Sent)),
            "trash" | "deleted" => Ok(QueryNode::Filter(FilterKind::Trash)),
            "spam" | "junk" => Ok(QueryNode::Filter(FilterKind::Spam)),
            "archived" | "archive" => Ok(QueryNode::Filter(FilterKind::Archived)),
            "snoozed" => Ok(QueryNode::Label("SNOOZED".to_string())),
            other => self.resolve_custom_filter(other),
        }
    }

    fn build_has_filter(&self, value: String) -> Result<QueryNode, ParseError> {
        match value.to_lowercase().as_str() {
            "attachment" | "attachments" => Ok(QueryNode::Filter(FilterKind::HasAttachment)),
            "calendar" | "invite" | "invites" => Ok(QueryNode::Filter(FilterKind::HasCalendar)),
            "userlabels" => Ok(QueryNode::Filter(FilterKind::HasUserLabels)),
            "nouserlabels" => Ok(QueryNode::Filter(FilterKind::NoUserLabels)),
            "drive" => Ok(QueryNode::Filter(FilterKind::HasDrive)),
            "document" => Ok(QueryNode::Filter(FilterKind::HasDocument)),
            "spreadsheet" => Ok(QueryNode::Filter(FilterKind::HasSpreadsheet)),
            "presentation" => Ok(QueryNode::Filter(FilterKind::HasPresentation)),
            "youtube" => Ok(QueryNode::Filter(FilterKind::HasYoutube)),
            "inline" | "image" | "inline-image" | "inline-images" => {
                Ok(QueryNode::Filter(FilterKind::HasInlineImage))
            }
            "link" | "links" => Ok(QueryNode::Filter(FilterKind::HasLink)),
            "link-heavy" | "links-heavy" | "linkheavy" => {
                Ok(QueryNode::Filter(FilterKind::HasLinkHeavy))
            }
            "link-none" | "no-link" | "no-links" | "linkfree" | "link-free" => {
                Ok(QueryNode::Filter(FilterKind::NoLinks))
            }
            "yellow-star" | "orange-star" | "red-star" | "purple-star" | "blue-star"
            | "green-star" | "red-bang" | "orange-guillemet" | "yellow-bang" | "green-check"
            | "blue-info" | "purple-question" => Ok(QueryNode::Filter(FilterKind::Starred)),
            other => self.resolve_custom_filter(other),
        }
    }

    /// Filters not in the built-in set route through
    /// `FilterKind::Custom(canonical_name)` when the caller has
    /// registered them via `ParserOptions::custom_filters`. Names are
    /// normalised to lowercase + hyphenated (`reply_later` → `reply-later`).
    fn resolve_custom_filter(&self, value: &str) -> Result<QueryNode, ParseError> {
        let canonical = canonical_filter_name(value);
        if self.options.has_custom_filter(&canonical) {
            return Ok(QueryNode::Filter(FilterKind::Custom(canonical)));
        }
        Err(ParseError::UnknownFilter(value.to_string()))
    }
}

fn combine_group_node(left: Option<QueryNode>, right: QueryNode, use_or: bool) -> QueryNode {
    match left {
        Some(left) if use_or => QueryNode::Or(Box::new(left), Box::new(right)),
        Some(left) => QueryNode::And(Box::new(left), Box::new(right)),
        None => right,
    }
}

/// Strip a leading `+` from field values (the no-stemming hint is
/// meaningless once you've narrowed to a specific field).
fn normalize_value(value: String) -> String {
    value.strip_prefix('+').unwrap_or(&value).to_string()
}

/// Lowercase + replace underscores with hyphens.
/// `reply_later` and `Reply-Later` both canonicalise to `reply-later`.
pub(crate) fn canonical_filter_name(value: &str) -> String {
    value.to_lowercase().replace('_', "-")
}

fn near_term(node: &QueryNode) -> Result<String, ParseError> {
    match node {
        QueryNode::Text(value) | QueryNode::Phrase(value) | QueryNode::Exact(value) => {
            Ok(value.clone())
        }
        other => Err(ParseError::UnexpectedToken(format!("{:?}", other))),
    }
}

fn phrase_node(value: String) -> Result<QueryNode, ParseError> {
    let parts = value.split_whitespace().collect::<Vec<_>>();
    if parts.len() == 4 && parts[1].eq_ignore_ascii_case("AROUND") {
        if let Ok(distance) = parts[2].parse::<u32>() {
            return Ok(QueryNode::Near {
                left: parts[0].to_string(),
                right: parts[3].to_string(),
                distance,
            });
        }
    }
    Ok(QueryNode::Phrase(value))
}

fn category_label(value: &str) -> Option<&'static str> {
    match value.to_lowercase().as_str() {
        "primary" | "personal" => Some("CATEGORY_PERSONAL"),
        "social" => Some("CATEGORY_SOCIAL"),
        "promotions" => Some("CATEGORY_PROMOTIONS"),
        "updates" => Some("CATEGORY_UPDATES"),
        "forums" => Some("CATEGORY_FORUMS"),
        "reservations" => Some("CATEGORY_RESERVATIONS"),
        "purchases" => Some("CATEGORY_PURCHASES"),
        _ => None,
    }
}

fn parse_date_value(s: &str) -> Result<DateValue, ParseError> {
    use chrono::NaiveDate;
    match s.to_lowercase().as_str() {
        "today" => Ok(DateValue::Today),
        "yesterday" => Ok(DateValue::Yesterday),
        "this-week" => Ok(DateValue::ThisWeek),
        "this-month" => Ok(DateValue::ThisMonth),
        _ => {
            for format in ["%Y-%m-%d", "%Y/%m/%d", "%m/%d/%Y"] {
                if let Ok(date) = NaiveDate::parse_from_str(s, format) {
                    return Ok(DateValue::Specific(date));
                }
            }
            Err(ParseError::InvalidDate(s.to_string()))
        }
    }
}

/// `older_than:5d` → `Relative { amount: 5, unit: Day }`. We deliberately
/// do not resolve to a concrete `NaiveDate` here — backends do that at
/// query-execution time using the configured `now_provider` (lets the
/// same AST be valid yesterday and tomorrow, and lets `Display` round-
/// trip without embedding a date).
fn parse_relative_duration(s: &str) -> Result<DateValue, ParseError> {
    let input = s.trim().to_lowercase();
    if input.len() < 2 {
        return Err(ParseError::InvalidDate(s.to_string()));
    }

    let (amount_str, unit_str) = input.split_at(input.len() - 1);
    let amount = amount_str
        .parse::<u32>()
        .map_err(|_| ParseError::InvalidDate(s.to_string()))?;
    let unit = match unit_str {
        "d" => RelativeUnit::Day,
        "w" => RelativeUnit::Week,
        "m" => RelativeUnit::Month,
        "y" => RelativeUnit::Year,
        _ => return Err(ParseError::InvalidDate(s.to_string())),
    };
    Ok(DateValue::Relative { amount, unit })
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

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let bytes = (value * multiplier).round() as u64;
    Ok((op, bytes))
}

fn parse_size_bytes(s: &str) -> Result<u64, ParseError> {
    parse_size_value(s).map(|(_, bytes)| bytes)
}

// -- Public API ---------------------------------------------------------------

/// Parse an email query string into a [`QueryNode`] AST.
///
/// Equivalent to [`parse_with`] with [`ParserOptions::default()`].
pub fn parse(input: &str) -> Result<QueryNode, ParseError> {
    parse_with(input, &ParserOptions::default())
}

/// Parse with caller-provided options. Use this to register custom
/// filter names (so e.g. `is:owed-reply` parses to
/// `FilterKind::Custom("owed-reply")` instead of returning
/// [`ParseError::UnknownFilter`]).
pub fn parse_with(input: &str, options: &ParserOptions) -> Result<QueryNode, ParseError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ParseError::UnexpectedEnd);
    }
    let tokens = tokenize(input)?;
    if tokens.is_empty() {
        return Err(ParseError::UnexpectedEnd);
    }
    let mut parser = Parser::new(tokens, options);
    let node = parser.parse_expression()?;
    if !parser.at_end() {
        match parser.peek() {
            Some(Token::RParen) => return Err(ParseError::UnmatchedParen),
            Some(Token::RBrace) => return Err(ParseError::UnmatchedBrace),
            _ => {}
        }
    }
    Ok(node)
}
