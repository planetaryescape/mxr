use chrono::NaiveDate;

#[derive(Debug, Clone, PartialEq)]
pub enum QueryNode {
    Text(String),
    Phrase(String),
    Field { field: QueryField, value: String },
    Filter(FilterKind),
    Label(String),
    DateRange { bound: DateBound, date: DateValue },
    Size {
        op: SizeOp,
        bytes: u64,
    },
    And(Box<QueryNode>, Box<QueryNode>),
    Or(Box<QueryNode>, Box<QueryNode>),
    Not(Box<QueryNode>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum QueryField {
    From,
    To,
    Cc,
    Bcc,
    Subject,
    Body,
    Filename,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FilterKind {
    Unread,
    Read,
    Starred,
    Draft,
    Sent,
    Trash,
    Spam,
    Answered,
    Inbox,
    Archived,
    HasAttachment,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DateBound {
    After,
    Before,
    Exact,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DateValue {
    Specific(NaiveDate),
    Today,
    Yesterday,
    ThisWeek,
    ThisMonth,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SizeOp {
    LessThan,
    LessThanOrEqual,
    Equal,
    GreaterThan,
    GreaterThanOrEqual,
}
