# Release publish script: draft lookup can create a duplicate draft

**Status:** open, nit · **Found:** 2026-10-08, independent review of PR #320

`scripts/publish_github_release.sh` decides between "create", "edit draft" and "already published" from `gh release view <tag> --json isDraft`. Two gaps remain:

- It treats a lookup failure whose text contains "release not found" as "no release". gh looks drafts up by a GraphQL query plus the published-tag REST endpoint; if the draft lookup fails in a way that surfaces as a not-found, the script creates a second draft for the same tag.
- Nothing detects two existing drafts for one tag (for example release-please's create racing the script's create). gh then picks one of them and the other is left behind.

Both need an unusual sequence (release-please's draft not yet created when `github-release` runs, which is about 20 minutes after the tag push). Not guarded because the odds are low and the effect is a stray draft, never a visible incomplete release.

Possible fix if it ever happens: list releases with `gh api repos/{owner}/{repo}/releases --paginate`, filter by `tag_name`, fail on more than one match, and operate by release id.
