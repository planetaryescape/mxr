# S08c candidate evidence

Candidate `4b2d19d621d0fba90eb9c5f0dbef570dd68ad28f` (tree `764486c7068745e07e263843e31ca0b1d82edb5b`) passed root review and PR398 CI. The reviewed tree merged as `b6b88deff57e0ab1b6f523844e1708e63d2121a7`. Release metadata PR410 merged as `98bc13f574da5cfc93fec1dfc1dddab4d6e996b9`, producing v0.6.84. At the time this candidate packet was captured, publication and installed acceptance were pending. This packet remains the candidate-era record; the later published release and installed acceptance are in the adjacent [installed packet](../s08c-installed-v0.6.84/README.md).

- `root-review.json` preserves the review disposition, bounded delta, binary hash, pending checks, limitations and hashes for original runtime evidence. Independent Claude review was waived by the user until Anthropic is available.
- `ci-summary.json` records CI run 38190844230 by job. The workspace test job also passed its `Verify supervised Linux ownership and optional model diagnostics` step.
- `pr398-merge-gate.json` and `pr410-release-gate.json` retain the reviewed patch/tree comparisons and integrated commits.
- `macos-smoke.json` records the synthetic supervised restart result: four persisted search results and three controlled model requests.
- `supervised-tui-acceptance.json` records compatible-daemon success, incompatible replacement rejection, cleanup and the 130-column limit. `reconnect-frames.txt` keeps only the repeated reconnect status excerpt from the captured frames.
- `manifest.json` binds the retained candidate files to their source artifacts with SHA-256 values. The published acceptance state belongs to the adjacent installed packet.

The fixture used synthetic mail, an owned foreground daemon and a controlled local model endpoint. It did not install a service or touch a personal daemon. Candidate receipts and PR CI remain distinct from published-release acceptance. The adjacent installed packet records the release URL, asset checksums, Homebrew/install results and published acceptance.
