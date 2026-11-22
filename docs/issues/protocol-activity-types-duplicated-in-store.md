# Activity `Tier` and `ActivityFilter` exist twice, in mxr-protocol and mxr-store

`crates/protocol/src/types.rs` (the activity log section) and `crates/store` each define `Tier` and `ActivityFilter`, with boundary converters at the bottom of the protocol section. The deleted comment justified it as keeping `protocol` a leaf crate with a light dependency graph. Every field added to one must be added to the other and to the converters.

Smallest fix: move the two types and their derives into `mxr-core` (already a dependency of both) and delete the converters.
