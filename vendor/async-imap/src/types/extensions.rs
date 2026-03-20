#![allow(missing_docs)]

use std::ops::RangeInclusive;

use super::{Fetch, Mailbox, Name};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamespaceEntry {
    pub prefix: String,
    pub delimiter: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Namespace {
    pub personal: Vec<NamespaceEntry>,
    pub other_users: Vec<NamespaceEntry>,
    pub shared: Vec<NamespaceEntry>,
}

#[derive(Debug)]
pub struct ListStatus {
    pub name: Name,
    pub mailbox: Mailbox,
}

#[derive(Debug)]
pub struct QresyncResponse {
    pub mailbox: Mailbox,
    pub vanished: Vec<RangeInclusive<u32>>,
    pub fetches: Vec<Fetch>,
}
