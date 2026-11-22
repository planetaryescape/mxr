# The undo log stores a mutation kind by stripping and re-adding JSON quotes

`crates/store/src/undo.rs` encodes `UndoableMutationKind` with serde, trims the quotes off the result for a plain TEXT column (`kind_json.trim_matches('"')`), and on read wraps the value in quotes again (`from_row`). The deleted comments said this keeps the column grep-friendly. A kind that serialised with a quote or as a non-unit variant would not round-trip.

Smallest fix: give `UndoableMutationKind` an explicit string form (`as_str` and `FromStr`) and use it in both places.
