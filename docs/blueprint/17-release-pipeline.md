# mxr — Release Pipeline Addendum

> This document covers the CI/CD pipeline: PR checks, release automation, scoped binary builds, Homebrew, changelog generation, and docs build checks.

> **Current state note (v1 launch)**
> The live release flow is: pushes to `main` run `release-please`, merged release PRs create `vX.Y.Z` tags, and tag pushes run [.github/workflows/release.yml](../../.github/workflows/release.yml). Artifact builds are scoped by [scripts/release_change_scope.sh](../../scripts/release_change_scope.sh): CLI-affecting tags build macOS Apple Silicon and Linux x86_64 archives, create the GitHub Release, and update the `planetaryescape/homebrew-mxr` tap; docs-only or version-only tags create the GitHub Release/changelog but skip binary artifacts and Homebrew. Supported Cargo installs are `cargo install --git ...` and `cargo install --path .`; crates.io publication is no longer part of the current release model. The web app is embedded into the CLI release binary, and docs deploy independently on pushes to `main`. macOS signing and notarization are optional for v1; when Apple secrets are absent, the workflow ships unsigned macOS binaries and users may see Gatekeeper friction. Read the checked-in workflows as the source of truth; the sections below include historical design context and earlier release-shape examples.

---

## CI on PRs and main

The checked-in workflow is [.github/workflows/ci.yml](../../.github/workflows/ci.yml). It uses change detection so docs-only, web-only, and Rust-heavy changes do not pay the same CI cost. `workflow_dispatch` marks every lane as changed.

Always-on checks:

- secret scan over checked-in files and Git history
- release gate scripts: version gate, bundled Gmail OAuth gate, release workflow policy checks, provider smoke workflow checks, CI workflow checks

Rust-affecting changes run `cargo deny`, `cargo check --workspace --all-targets`, architecture-boundary checks, `cargo fmt`, clippy, nextest lanes, SQLx offline checks, and MSRV checks where configured.

Web-affecting changes run npm audit, typecheck, lint, unit tests, and focused Playwright smoke checks.

Docs-affecting changes run the docs site build:

```bash
cd site
npm ci
npm audit --audit-level=moderate
npm run build
```

Legal pages are built as part of the site. There is no separate checked-in `policy-sync` job today, so policy text changes must update both the root policy file and the corresponding `site/src/pages/` page in the same PR.

---

## Release strategy

### Versioning

Semantic versioning (semver). Given `MAJOR.MINOR.PATCH`:

- **PATCH** (0.1.0 → 0.1.1): Bug fixes, dependency updates, docs fixes. No new features. No breaking changes.
- **MINOR** (0.1.1 → 0.2.0): New features, non-breaking additions. New CLI commands, new config options, new keybindings.
- **MAJOR** (0.x → 1.0.0): Breaking changes to CLI interface, config format, IPC protocol, or provider trait API. Pre-1.0, minor versions can include breaking changes (standard Rust convention).

### Release trigger

Releases are triggered by pushing a git tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The tag always creates or updates a GitHub Release. When the scoped diff affects CLI artifacts, the same workflow also builds binaries and updates Homebrew.

For docs-only or version-only tags, `scripts/release_change_scope.sh` sets `cli_changed=false` and `has_artifacts=false`. Those tags still get a GitHub Release and changelog, but they do not build tarballs or update the Homebrew tap.

### Pre-release checklist (manual, before tagging)

1. Update version in root `Cargo.toml` (workspace version)
2. Update `CHANGELOG.md` (or let git-cliff generate it)
3. Verify CI is green on main
4. Commit version bump + changelog
5. Tag and push

---

## Historical crates.io publishing

This section is retained as historical context from the earlier release design. It is not the current `mxr` release model.

### Workspace publish order

Crates have dependencies on each other. They must be published in dependency order (leaf crates first):

```
1. mxr-core           # No internal dependencies
2. mxr-protocol       # Depends on: core
3. mxr-store          # Depends on: core
4. mxr-search         # Depends on: core
5. mxr-reader         # Depends on: core
6. mxr-provider-fake  # Depends on: core
7. mxr-provider-gmail # Depends on: core
8. mxr-provider-imap  # Depends on: core
9. mxr-provider-smtp  # Depends on: core
10. mxr-compose       # Depends on: core, store
11. mxr-rules         # Depends on: core, store
12. mxr-export        # Depends on: core, store, reader
13. mxr-sync          # Depends on: core, store, search
14. mxr-ai            # Depends on: core (behind feature flag)
15. mxr-daemon        # Depends on: most crates
16. mxr-tui           # Depends on: core, protocol
17. mxr-cli           # Depends on: core, protocol (the binary crate)
```

### Why publish individual crates?

- `mxr-core` is the stable API that community adapter authors depend on. It MUST be on crates.io as its own crate.
- Individual crates let users depend on just what they need (e.g., a tool that only needs the search engine can depend on `mxr-search`).
- It's the Rust ecosystem convention for workspace projects.

### crates.io API token

Store as a GitHub Actions secret: `CARGO_REGISTRY_TOKEN`. Generated from your crates.io account settings.

### Publish workflow

```yaml
name: Publish to crates.io
on:
  push:
    tags: ['v*']

jobs:
  publish:
    name: Publish crates
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Verify version matches tag
        run: |
          TAG_VERSION="${GITHUB_REF#refs/tags/v}"
          CARGO_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
          if [ "$TAG_VERSION" != "$CARGO_VERSION" ]; then
            echo "Tag version ($TAG_VERSION) does not match Cargo.toml version ($CARGO_VERSION)"
            exit 1
          fi

      - name: Publish crates in dependency order
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        run: |
          # Publish order: leaf crates first, binary crate last.
          # --no-verify skips rebuild (CI already verified).
          # Sleep between publishes to let crates.io index propagate.
          CRATES=(
            "crates/core"
            "crates/protocol"
            "crates/store"
            "crates/search"
            "crates/reader"
            "crates/providers/fake"
            "crates/providers/gmail"
            "crates/providers/imap"
            "crates/providers/smtp"
            "crates/compose"
            "crates/rules"
            "crates/export"
            "crates/sync"
            "crates/ai"
            "crates/daemon"
            "crates/tui"
            "crates/cli"
          )

          for crate in "${CRATES[@]}"; do
            echo "Publishing $crate..."
            cargo publish --manifest-path "$crate/Cargo.toml" --no-verify
            echo "Waiting for crates.io index to update..."
            sleep 30
          done
```

The `sleep 30` between publishes is necessary because crates.io needs time to index each crate before dependent crates can reference it. 30 seconds is conservative but safe. Some workspace publish tools (like `cargo-workspaces` or `cargo-release`) handle this automatically.

### Alternative: use `cargo-workspaces`

Instead of the manual publish script, use `cargo-workspaces` which handles dependency ordering and index propagation automatically:

```yaml
      - name: Install cargo-workspaces
        run: cargo install cargo-workspaces

      - name: Publish all crates
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        run: cargo workspaces publish --from-git --yes
```

`cargo-workspaces` resolves the publish order from the dependency graph and waits for index propagation. Simpler and less error-prone than the manual script. Recommended.

---

## Cross-compiled binary builds

The checked-in workflow currently builds release archives only for Linux x86_64 and macOS Apple Silicon. The older four-target launch plan is superseded; resurrect it only if Linux ARM64 or Intel macOS packaging comes back.

### Target matrix

```
linux-x86_64        # x86_64-unknown-linux-gnu
macos-aarch64       # aarch64-apple-darwin (Apple Silicon)
```

No Windows target for v1. mxr depends on Unix sockets, XDG paths, and Unix-native tooling. Windows support is a future consideration, not a launch requirement.

### Binary naming

```
mxr-v0.1.0-linux-x86_64.tar.gz
mxr-v0.1.0-macos-aarch64.tar.gz
```

Each archive contains:
- `mxr` binary
- `mxr-chime-player` helper binary
- `LICENSE-MIT`
- `LICENSE-APACHE`
- `README.md`

### Build workflow

```yaml
name: Release binaries
on:
  push:
    tags: ['v*']

permissions:
  contents: write

jobs:
  build:
    name: Build ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
            archive: linux-x86_64.tar.gz
          - target: aarch64-apple-darwin
            os: macos-15
            archive: macos-aarch64.tar.gz

    steps:
      - uses: actions/checkout@v6

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      # For Linux native build dependencies
      - name: Install system dependencies
        if: matrix.os == 'ubuntu-latest'
        run: |
          sudo apt-get update
          sudo apt-get install -y libssl-dev libasound2-dev pkg-config

      - uses: Swatinem/rust-cache@v2
        with:
          key: ${{ matrix.target }}

      - name: Build
        run: cargo build --release --locked --target ${{ matrix.target }} -p mxr --features semantic-local,web-ui

      - name: Package
        run: |
          VERSION="${GITHUB_REF#refs/tags/v}"
          ARCHIVE="mxr-v${VERSION}-${{ matrix.archive }}"
          mkdir -p release
          cp target/${{ matrix.target }}/release/mxr release/
          cp target/${{ matrix.target }}/release/mxr-chime-player release/
          cp LICENSE-MIT LICENSE-APACHE README.md release/
          cd release
          tar czf "../${ARCHIVE}" *
          cd ..
          echo "ARCHIVE=${ARCHIVE}" >> $GITHUB_ENV

      - name: Generate SHA256 checksum
        run: |
          sha256sum "${{ env.ARCHIVE }}" > "${{ env.ARCHIVE }}.sha256"

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.target }}
          path: |
            ${{ env.ARCHIVE }}
            ${{ env.ARCHIVE }}.sha256

  release:
    name: Create GitHub Release
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts
          merge-multiple: true

      - name: Generate changelog for this release
        id: changelog
        run: |
          # Get commits since last tag
          PREV_TAG=$(git describe --tags --abbrev=0 HEAD^ 2>/dev/null || echo "")
          if [ -n "$PREV_TAG" ]; then
            CHANGES=$(git log --pretty=format:"- %s (%h)" "${PREV_TAG}..HEAD")
          else
            CHANGES=$(git log --pretty=format:"- %s (%h)")
          fi
          echo "changes<<EOF" >> $GITHUB_OUTPUT
          echo "$CHANGES" >> $GITHUB_OUTPUT
          echo "EOF" >> $GITHUB_OUTPUT

      - name: Create release
        uses: softprops/action-gh-release@v2
        with:
          files: artifacts/*
          generate_release_notes: false
          body: |
            ## Installation

            **Cargo (from source):**
            ```bash
            cargo install --git https://github.com/planetaryescape/mxr --tag vX.Y.Z --locked mxr
            ```

            **Pre-built binaries:**
            Download the appropriate binary for your platform below, extract, and place `mxr` in your `$PATH`.

            **Homebrew (macOS/Linux):**
            ```bash
            brew install planetaryescape/mxr/mxr
            ```

            ## Checksums
            SHA256 checksums are provided alongside each binary. Verify with:
            ```bash
            sha256sum -c mxr-v*.sha256
            ```

            ## What's changed
            ${{ steps.changelog.outputs.changes }}
```

---

## Changelog generation

### Option 1: git-cliff (recommended)

`git-cliff` generates changelogs from conventional commits. Add a `cliff.toml` config:

```toml
# cliff.toml
[changelog]
header = "# Changelog\n\n"
body = """
{% for group, commits in commits | group_by(attribute="group") %}
### {{ group | upper_first }}
{% for commit in commits %}
- {{ commit.message | upper_first }} ({{ commit.id | truncate(length=7, end="") }})\
{% endfor %}
{% endfor %}
"""
trim = true

[git]
conventional_commits = true
filter_unconventional = true
commit_parsers = [
    { message = "^feat", group = "Features" },
    { message = "^fix", group = "Bug Fixes" },
    { message = "^doc", group = "Documentation" },
    { message = "^perf", group = "Performance" },
    { message = "^refactor", group = "Refactoring" },
    { message = "^ci", group = "CI" },
    { message = "^chore", skip = true },
    { message = "^style", skip = true },
]
```

Generate changelog before tagging:

```bash
git cliff --output CHANGELOG.md
git add CHANGELOG.md
git commit -m "chore: update changelog for v0.1.0"
git tag v0.1.0
git push origin main v0.1.0
```

### Option 2: GitHub's auto-generated release notes

The `softprops/action-gh-release` action can use GitHub's built-in release notes generation. Simpler but less control over formatting. Set `generate_release_notes: true` and remove the manual changelog step.

### Commit convention

Use conventional commits for clean changelog generation:

```
feat: add IMAP adapter
fix: handle expired OAuth token gracefully
docs: add search syntax reference
refactor: simplify sync engine delta tracking
perf: batch Tantivy index commits
ci: add aarch64 Linux build target
chore: update dependencies
```

This should be documented in `CONTRIBUTING.md` and enforced with a commit message lint in CI (optional but recommended):

```yaml
  commit-lint:
    name: Commit Message Lint
    runs-on: ubuntu-latest
    if: github.event_name == 'pull_request'
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: wagoid/commitlint-github-action@v5
        with:
          configFile: .commitlintrc.yml
```

---

## Homebrew

### Homebrew formula

Create a formula that installs from the pre-built binary.

The formula template lives at [packaging/homebrew/mxr.rb](../../packaging/homebrew/mxr.rb). The release workflow renders it with the current version and checksums, then pushes the result to `planetaryescape/homebrew-mxr`.

```ruby
# Formula/mxr.rb
class Mxr < Formula
  desc "Local-first terminal email client"
  homepage "https://github.com/planetaryescape/mxr"
  version "__VERSION__"
  license "MIT OR Apache-2.0"

  on_macos do
    depends_on arch: :arm64

    on_arm do
      url "https://github.com/planetaryescape/mxr/releases/download/v#{version}/mxr-v#{version}-macos-aarch64.tar.gz"
      sha256 "__SHA256_MACOS_AARCH64__"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/planetaryescape/mxr/releases/download/v#{version}/mxr-v#{version}-linux-x86_64.tar.gz"
      sha256 "__SHA256_LINUX_X86_64__"
    end
  end

  def install
    bin.install "mxr"
    bin.install "mxr-chime-player"
    prefix.install "LICENSE-MIT"
    prefix.install "LICENSE-APACHE"
    prefix.install "README.md"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/mxr version")
  end
end
```

### Auto-update formula on release

The checked-in release workflow updates the Homebrew tap only when CLI artifacts changed:

```yaml
  update-homebrew:
    name: Homebrew Formula
    needs: [plan, github-release]
    if: needs.plan.outputs.cli_changed == 'true' && needs.github-release.result == 'success'
    runs-on: macos-15
    steps:
      - uses: actions/checkout@v6
      - uses: actions/download-artifact@v8
        with:
          path: artifacts
          merge-multiple: true
      - name: Render formula
        run: |
          VERSION="${GITHUB_REF#refs/tags/v}"
          bash ./scripts/render_homebrew_formula.sh "$VERSION" artifacts Formula/mxr.rb
      - name: Update tap formula
        env:
          COMMITTER_TOKEN: ${{ secrets.HOMEBREW_TAP_TOKEN }}
        run: |
          VERSION="${GITHUB_REF#refs/tags/v}"
          git clone "https://x-access-token:${COMMITTER_TOKEN}@github.com/planetaryescape/homebrew-mxr.git" tap
          cp Formula/mxr.rb tap/Formula/mxr.rb
          git -C tap add Formula/mxr.rb
          git -C tap commit -m "mxr ${VERSION}"
          git -C tap push origin HEAD:main
```

This pushes updated URLs and SHA256 checksums to the tap repo whenever a CLI-affecting release is published.

### User installation

```bash
# Add tap (one time)
brew tap planetaryescape/mxr

# Install
brew install mxr

# Update
brew upgrade mxr
```

---

## Other package managers (future)

### AUR (Arch Linux)

Create an `mxr-bin` AUR package that installs the pre-built binary, and an `mxr` AUR package that builds from source. This is community-maintained territory, but having the PKGBUILD ready in the repo helps:

```
packaging/
├── aur/
│   ├── mxr-bin/PKGBUILD       # Pre-built binary
│   └── mxr/PKGBUILD           # Build from source
├── homebrew/
│   └── mxr.rb                  # Formula template
└── deb/                         # Future: .deb package
```

### Nix

A `flake.nix` in the repo root enables Nix users to install directly:

```bash
nix profile install github:planetaryescape/mxr
```

This is low effort (the Rust build is straightforward) and serves a vocal segment of the target audience.

### cargo-binstall

If the binary release naming follows the `cargo-binstall` convention (which our naming does), users can install pre-built binaries via:

```bash
cargo binstall mxr
```

No extra work needed if the GitHub Release assets follow the naming pattern `{name}-v{version}-{target}.tar.gz`, which ours do.

---

## Docs site deployment

The release workflow does not deploy the docs site. The docs site lives in
`site/`, builds with Astro/Starlight, and deploys through Vercel for
`https://mxr-mail.vercel.app` when `main` changes.

Local proof:

```bash
cd site
npm ci
npm run build
```

---

## Release GitHub secrets

| Secret | Purpose |
|---|---|
| `RELEASE_PLEASE_TOKEN` | PAT with `contents:write` + `workflows` so release-please tag pushes trigger `release.yml`. |
| `HOMEBREW_TAP_TOKEN` | GitHub PAT with push access to the homebrew-tap repo |
| `OUTLOOK_CLIENT_ID` | Optional bundled Outlook OAuth app client id compiled into release artifacts. |
| `APPLE_CERT_P12_BASE64` | Optional Developer ID Application cert exported as `.p12`, then `base64 -i cert.p12 \| pbcopy`. When absent, macOS release artifacts ship unsigned. |
| `APPLE_CERT_PASSWORD` | Optional password used when exporting the `.p12`; required only when `APPLE_CERT_P12_BASE64` is set. |
| `APPLE_KEYCHAIN_PASSWORD` | Optional throwaway password used to unlock the temporary CI keychain; required only when `APPLE_CERT_P12_BASE64` is set. |
| `APPLE_DEVELOPER_ID` | Optional identity name passed to `codesign --sign` — typically `Developer ID Application: Your Name (TEAMID)`. When absent, signing is skipped. |
| `APPLE_ID` | Optional Apple account email for `notarytool submit`; required only for notarization. |
| `APPLE_TEAM_ID` | Optional 10-char alphanumeric team identifier; required only for notarization. |
| `APPLE_APP_SPECIFIC_PASSWORD` | Optional app-specific password generated at appleid.apple.com. Required only for notarization. |
| `GMAIL_CLIENT_ID` | Optional bundled Gmail OAuth client id. For v1 this is an unverified fallback; official Gmail setup advice is user-created OAuth clients. When set with `GMAIL_CLIENT_SECRET`, it is compiled into release artifacts by default. |
| `GMAIL_CLIENT_SECRET` | Optional bundled Gmail OAuth client secret. Must be set with `GMAIL_CLIENT_ID`, or both omitted. |
| `MXR_GMAIL_TEST_CLIENT_ID` | Optional live Gmail E2E smoke test client id for release runs. |
| `MXR_GMAIL_TEST_CLIENT_SECRET` | Optional live Gmail E2E smoke test client secret. |
| `MXR_GMAIL_TEST_REFRESH_TOKEN` | Optional live Gmail E2E smoke refresh token. When any live smoke secret is absent, release skips the live Gmail smoke. |

For CLI-affecting release tags, deterministic CLI/provider tests remain
release-blocking. Live Gmail smoke, macOS signing, and notarization are
opportunistic: they run when their secrets are configured and skip with a
warning when secrets are absent. Unsigned macOS binaries are accepted for v1;
document the Gatekeeper warning in install/release notes instead of blocking
the release. Docs-only or version-only tags still skip binary artifacts via
`scripts/release_change_scope.sh`. Every release run first verifies that
the tag (for example `v0.5.47`) matches `workspace.package.version` in
`Cargo.toml` via `scripts/release_version_gate.sh`.

---

## Complete release flow (end to end)

```
1. Developer finishes work, merges to main
2. CI runs on main: secret scan, release gate scripts, and changed Rust/web/docs lanes
3. Developer updates version in Cargo.toml
4. Developer runs: git cliff --output CHANGELOG.md
5. Developer commits: git commit -m "chore: release v0.1.0"
6. Developer tags: git tag v0.1.0
7. Developer pushes: git push origin main v0.1.0
8. Tag triggers release pipeline:
   a. Verify tag version matches Cargo.toml version
   b. If CLI artifacts changed, build Linux x86_64 and macOS Apple Silicon binaries
   c. Generate SHA256 checksums
   d. Create GitHub Release with binaries, checksums, and changelog
   e. If CLI artifacts changed, update Homebrew formula
9. Done. Users can now:
   - cargo install --git https://github.com/planetaryescape/mxr --tag vX.Y.Z --locked mxr
   - brew install planetaryescape/mxr/mxr
   - Download binary from GitHub Releases
```

Docs deploy separately through Vercel when the merged `main` commit changes the site.

---

## Decision records

**D066: Semantic versioning with conventional commits**

**Chosen**: Semver for versions. Conventional commits for changelog generation via git-cliff.

**Why**: Semver is the Rust ecosystem standard. Conventional commits enable automated changelog generation, which saves manual effort on every release. git-cliff is a Rust-native tool that parses conventional commits into grouped changelogs. The alternative (manually writing changelogs) doesn't scale and is error-prone.

**D067: Native release builds for current binary targets**

**Chosen**: Build Linux x86_64 natively on Linux runners and macOS Apple Silicon natively on macOS runners. Keep `cross` available only if future Linux ARM64 or musl targets return.

**Why**: This matches `.github/workflows/release.yml`: the current matrix is `x86_64-unknown-linux-gnu` and `aarch64-apple-darwin`, both with `semantic-local,web-ui` enabled. The older musl/cross plan was useful launch thinking, but it is not the present release contract.

**D068: Workspace publish via cargo-workspaces**

**Chosen**: Use `cargo-workspaces` to publish all crates in dependency order with automatic index propagation handling.

**Why**: Manual publish scripts are fragile (wrong order, insufficient sleep between publishes, partial failures). `cargo-workspaces` resolves the dependency graph automatically, waits for crates.io index propagation, and handles the workspace publish as a single atomic-ish operation. It's the standard tool for Rust workspace releases.

**D069: Homebrew tap with auto-update on release**

**Chosen**: Maintain a separate `homebrew-mxr` tap repository. Auto-update the formula via `bump-homebrew-formula-action` on every release.

**Why**: Homebrew is the standard package manager for macOS power users (our target audience). A tap is simpler than getting into homebrew-core (which requires popularity thresholds). Auto-updating the formula eliminates a manual step from the release process. The formula installs pre-built binaries for speed.

**D070: No Windows builds in v1**

**Chosen**: No Windows target for initial releases.

**Why**: mxr depends on Unix sockets for daemon IPC, XDG directory conventions, `xdg-open` for browser launching, and Unix-native tools. Windows support would require significant platform abstraction work (named pipes instead of Unix sockets, different directory conventions, different process management). The target audience (terminal power users running vim) is overwhelmingly on Linux and macOS. Windows support can be added later if demand justifies the effort, or via WSL which works today.

**D071: cargo-binstall compatibility for free**

**Chosen**: Name binary release assets following the cargo-binstall naming convention.

**Why**: Zero extra work. If the GitHub Release assets are named `{name}-v{version}-{target}.tar.gz` (which ours are), cargo-binstall can install pre-built binaries automatically. This gives users a fast install path (`cargo binstall mxr`) without the 5+ minute source build, and it requires no infrastructure beyond what we already build.
