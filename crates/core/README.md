# mxr-core

Stable core types and provider traits for mxr.

This crate contains:

- typed ids
- account, message, label, draft, and sync types
- provider traits for sync and send adapters
- shared error types

Adapter authors should depend on `mxr-core`, implement the provider traits, and
run the conformance helpers from `mxr-provider-fake`.

Main repository: <https://github.com/planetaryescape/mxr>
