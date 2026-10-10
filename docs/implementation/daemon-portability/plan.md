# Daemon portability has one implementation blueprint

The canonical requirements, code evidence, ordered slices and acceptance checks are in [23-daemon-portability.md](../../blueprint/23-daemon-portability.md). Use its P01-P08 package IDs when assigning implementation work. This earlier plan has been consolidated there so requirements cannot drift between two files.

The [iOS architecture assessment](../../research/ios-architecture.md) remains the research background. No product implementation was completed by this documentation task. The next substantive workflow is P01; P06a, P07a and P07b are independent smaller fixes that may land first. No blocker prevents starting those slices.

The primary checkout also contains an uncommitted `docs/issues/openapi-version-drift.md`. Check and reuse it for P04 regeneration work while preserving its existing changes. It has not been copied into this research worktree.
