# The spec is the innovation

There is a certain kind of software that gets harder to trust the moment it starts moving fast.

Email is one of those categories.

You can fake a lot in a demo. You cannot fake provider boundaries, draft handling, rule behavior, sync correctness, search semantics, or mutation safety for very long. The shortcuts show up later, usually after someone has already started relying on the tool.

That is why the blueprint matters so much in mxr.

The design docs are not there to make the repo look serious. They are there because the shape of the system is the product. If the provider boundary is wrong, the adapters rot. If the local store is not clearly the source of truth, search and sync drift apart. If rules are not deterministic first, nobody should trust them with a real mailbox.

The code still matters, obviously. But the code is downstream from a bunch of decisions that are easy to wave away until they are expensive to change.

I do not think that makes mxr unusual because it has docs. Plenty of projects have docs. The interesting part is which things got specified early:

- SQLite as canonical state
- Tantivy as rebuildable search layer
- daemon over monolith
- provider-specific code forced behind adapters
- `$EDITOR` compose instead of editor replacement
- rules as inspectable data before scripts

That kind of spec work looks slower right up until the moment it saves you from a bad architecture groove.

It also makes contribution easier in a less flashy way. People can disagree with a document. They can trace a choice. They can spot drift. That is much better than discovering the real contract by reverse-engineering five crates and a pile of side effects.

So yes, the implementation is important. But in a project like this, the spec is not paperwork around the product. The spec is a big part of the product.
