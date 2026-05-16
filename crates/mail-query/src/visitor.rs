//! Visitor pattern for walking a parsed AST.
//!
//! Implementations override the leaf hooks they care about and let the
//! default [`Visitor::visit_node`] handle tree traversal. The pattern
//! mirrors `syn` and `sqlparser-rs`.
//!
//! ```
//! use mail_query::{parse, FilterKind, QueryNode, Visitor};
//!
//! #[derive(Default)]
//! struct CountFilters(usize);
//! impl Visitor for CountFilters {
//!     fn visit_filter(&mut self, _: &FilterKind) {
//!         self.0 += 1;
//!     }
//! }
//!
//! let ast = parse("from:alice is:unread has:attachment").expect("parses");
//! let mut counter = CountFilters::default();
//! counter.walk(&ast);
//! assert_eq!(counter.0, 2);
//! ```

use crate::ast::{DateBound, DateValue, FilterKind, QueryField, QueryNode, SizeOp};

/// AST visitor.
///
/// Implementors override the per-variant hooks they care about. Call
/// [`Visitor::walk`] on a root [`QueryNode`] to start traversal —
/// `walk` recurses into compound nodes (`And`/`Or`/`Not`) and dispatches
/// to leaf hooks.
#[allow(unused_variables)]
pub trait Visitor {
    /// Recursive walk. Visits leaves via the typed hooks below;
    /// recurses into `And`/`Or`/`Not` children. Override only if you
    /// need to short-circuit traversal.
    fn walk(&mut self, node: &QueryNode) {
        match node {
            QueryNode::Text(s) => self.visit_text(s),
            QueryNode::Exact(s) => self.visit_exact(s),
            QueryNode::Phrase(s) => self.visit_phrase(s),
            QueryNode::Field { field, value } => self.visit_field(*field, value),
            QueryNode::Filter(kind) => self.visit_filter(kind),
            QueryNode::Label(name) => self.visit_label(name),
            QueryNode::DateRange { bound, date } => self.visit_date(*bound, date),
            QueryNode::Size { op, bytes } => self.visit_size(*op, *bytes),
            QueryNode::Near {
                left,
                right,
                distance,
            } => self.visit_near(left, right, *distance),
            QueryNode::And(l, r) => {
                self.visit_and_pre(l, r);
                self.walk(l);
                self.walk(r);
                self.visit_and_post(l, r);
            }
            QueryNode::Or(l, r) => {
                self.visit_or_pre(l, r);
                self.walk(l);
                self.walk(r);
                self.visit_or_post(l, r);
            }
            QueryNode::Not(inner) => {
                self.visit_not_pre(inner);
                self.walk(inner);
                self.visit_not_post(inner);
            }
        }
    }

    fn visit_text(&mut self, s: &str) {}
    fn visit_exact(&mut self, s: &str) {}
    fn visit_phrase(&mut self, s: &str) {}
    fn visit_field(&mut self, field: QueryField, value: &str) {}
    fn visit_filter(&mut self, kind: &FilterKind) {}
    fn visit_label(&mut self, name: &str) {}
    fn visit_date(&mut self, bound: DateBound, date: &DateValue) {}
    fn visit_size(&mut self, op: SizeOp, bytes: u64) {}
    fn visit_near(&mut self, left: &str, right: &str, distance: u32) {}

    fn visit_and_pre(&mut self, left: &QueryNode, right: &QueryNode) {}
    fn visit_and_post(&mut self, left: &QueryNode, right: &QueryNode) {}
    fn visit_or_pre(&mut self, left: &QueryNode, right: &QueryNode) {}
    fn visit_or_post(&mut self, left: &QueryNode, right: &QueryNode) {}
    fn visit_not_pre(&mut self, inner: &QueryNode) {}
    fn visit_not_post(&mut self, inner: &QueryNode) {}
}
