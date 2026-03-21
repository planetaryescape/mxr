# Why local-first and daemon-backed still matters

Most email software makes you pick a lane.

You either get a terminal client with years of keyboard muscle memory and not much system underneath it, or you get a hosted layer that is easy to plug into automation but moves the trust boundary somewhere else. Neither of those is wrong. They solve real problems. They just are not the same problem.

mxr is built around the idea that the useful thing is not the UI and it is not the hosted connector. The useful thing is the local mail runtime.

Once you start there, a few decisions stop feeling optional.

SQLite becomes the source of truth because local state has to survive the network.

A daemon becomes the system because sync, indexing, snooze, and automation should not depend on whether the TUI is open.

Search stops being a feature bullet and turns into the navigation model because folder trees are not enough once the data is local and queryable.

The CLI stops being a side interface because it is the cleanest way to expose the runtime to scripts and agents.

That is the piece I keep coming back to: local-first is not only about privacy. It changes the shape of the product. It changes which parts matter. It changes which shortcuts stop being acceptable.

You can feel that most clearly with agents.

The current wave of email-for-agents tooling mostly starts from connectors, hosted auth, and remote action layers. That makes sense. It is the fastest way to give a model access to a mailbox. But it is also a different bet. It says the center of gravity is the integration layer.

mxr makes the opposite bet. Put the mail runtime on the machine first. Make the CLI broad. Let the agent use the same local system a human uses. That choice does not make hosted layers obsolete. It just makes them a different category.

That difference is enough to build around.
