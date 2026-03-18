# mxr — Release Pipeline Addendum

> This document covers the full CI/CD pipeline: PR checks, release automation, crates.io publishing, cross-compiled binary builds, Homebrew, changelog generation, and docs deployment.

---

## CI on every PR

Every pull request runs these checks. All must pass before merge.

### Workflow: `ci.yml`

```yaml
name: CI
on:
  pull_request:
  push:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: -Dwarnings

jobs:
  fmt:
    name: Formatting
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all --check

  clippy:
    name: Lint
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --workspace --all-targets --all-features -- -D warnings

  test:
    name: Test
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace --all-features

  build:
    name: Build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --workspace --all-features

  # Verify sqlx compile-time checked queries
  sqlx-check:
    name: SQLx Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo sqlx prepare --check --workspace

  # Docs site build check
  docs:
    name: Docs Build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm
          cache-dependency-path: site/package-lock.json
      - working-directory: site
        run: npm ci
      - working-directory: site
        run: npm run build

  # Privacy/terms sync check
  policy-sync:
    name: Policy Sync
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Check privacy policy sync
        run: diff PRIVACY.md site/src/pages/privacy.md
      - name: Check terms sync
        run: diff TERMS.md site/src/pages/terms.md
```

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

The tag triggers the full release pipeline: build binaries, publish crates, create GitHub Release, update Homebrew, deploy docs.

### Pre-release checklist (manual, before tagging)

1. Update version in root `Cargo.toml` (workspace version)
2. Update `CHANGELOG.md` (or let git-cliff generate it)
3. Verify CI is green on main
4. Commit version bump + changelog
5. Tag and push

---

## Crates.io publishing

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

### Target matrix

```
linux-x86_64        # x86_64-unknown-linux-musl (static binary, runs everywhere)
linux-aarch64       # aarch64-unknown-linux-musl (ARM64, Raspberry Pi, cloud ARM instances)
macos-x86_64        # x86_64-apple-darwin (Intel Macs)
macos-aarch64       # aarch64-apple-darwin (Apple Silicon)
```

We use `musl` for Linux targets to produce fully static binaries with no dynamic library dependencies. This means the binary runs on any Linux distribution regardless of glibc version.

No Windows target for v1. mxr depends on Unix sockets, XDG paths, and Unix-native tooling. Windows support is a future consideration, not a launch requirement.

### Binary naming

```
mxr-v0.1.0-linux-x86_64.tar.gz
mxr-v0.1.0-linux-aarch64.tar.gz
mxr-v0.1.0-macos-x86_64.tar.gz
mxr-v0.1.0-macos-aarch64.tar.gz
```

Each archive contains:
- `mxr` binary
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
          - target: x86_64-unknown-linux-musl
            os: ubuntu-latest
            archive: mxr-linux-x86_64.tar.gz
          - target: aarch64-unknown-linux-musl
            os: ubuntu-latest
            archive: mxr-linux-aarch64.tar.gz
          - target: x86_64-apple-darwin
            os: macos-latest
            archive: mxr-macos-x86_64.tar.gz
          - target: aarch64-apple-darwin
            os: macos-latest
            archive: mxr-macos-aarch64.tar.gz

    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      # For Linux cross-compilation
      - name: Install cross-compilation tools
        if: matrix.os == 'ubuntu-latest'
        run: |
          sudo apt-get update
          sudo apt-get install -y musl-tools
          if [ "${{ matrix.target }}" = "aarch64-unknown-linux-musl" ]; then
            sudo apt-get install -y gcc-aarch64-linux-gnu
          fi

      - uses: Swatinem/rust-cache@v2
        with:
          key: ${{ matrix.target }}

      # Build with cross for Linux targets (handles cross-compilation toolchains)
      - name: Install cross
        if: matrix.os == 'ubuntu-latest'
        run: cargo install cross

      - name: Build (Linux)
        if: matrix.os == 'ubuntu-latest'
        run: cross build --release --target ${{ matrix.target }} --all-features

      - name: Build (macOS)
        if: matrix.os == 'macos-latest'
        run: cargo build --release --target ${{ matrix.target }} --all-features

      - name: Package
        run: |
          VERSION="${GITHUB_REF#refs/tags/v}"
          ARCHIVE="mxr-v${VERSION}-${{ matrix.archive }}"
          mkdir -p release
          cp target/${{ matrix.target }}/release/mxr release/
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
            cargo install mxr
            # With AI features:
            cargo install mxr --features ai
            ```

            **Pre-built binaries:**
            Download the appropriate binary for your platform below, extract, and place `mxr` in your `$PATH`.

            **Homebrew (macOS/Linux):**
            ```bash
            brew install mxr
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

Create a formula that installs from the pre-built binary (faster) or builds from source (for users who prefer it).

The formula lives in a separate tap repository: `homebrew-tap` (e.g., `github.com/USER/homebrew-mxr`).

```ruby
# Formula/mxr.rb
class Mxr < Formula
  desc "A local-first terminal email client for power users"
  homepage "https://mxr.dev"
  license any_of: ["MIT", "Apache-2.0"]
  version "0.1.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/USER/mxr/releases/download/v0.1.0/mxr-v0.1.0-macos-aarch64.tar.gz"
      sha256 "PLACEHOLDER_SHA256"
    else
      url "https://github.com/USER/mxr/releases/download/v0.1.0/mxr-v0.1.0-macos-x86_64.tar.gz"
      sha256 "PLACEHOLDER_SHA256"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/USER/mxr/releases/download/v0.1.0/mxr-v0.1.0-linux-aarch64.tar.gz"
      sha256 "PLACEHOLDER_SHA256"
    else
      url "https://github.com/USER/mxr/releases/download/v0.1.0/mxr-v0.1.0-linux-x86_64.tar.gz"
      sha256 "PLACEHOLDER_SHA256"
    end
  end

  def install
    bin.install "mxr"
  end

  test do
    assert_match "mxr", shell_output("#{bin}/mxr version")
  end
end
```

### Auto-update formula on release

Add a step to the release workflow that updates the Homebrew tap:

```yaml
  update-homebrew:
    name: Update Homebrew formula
    needs: release
    runs-on: ubuntu-latest
    steps:
      - name: Update Homebrew formula
        uses: mislav/bump-homebrew-formula-action@v3
        with:
          formula-name: mxr
          homebrew-tap: USER/homebrew-mxr
          download-url: https://github.com/USER/mxr/releases/download/${{ github.ref_name }}/mxr-${{ github.ref_name }}-macos-aarch64.tar.gz
        env:
          COMMITTER_TOKEN: ${{ secrets.HOMEBREW_TAP_TOKEN }}
```

This automatically creates a PR on the tap repo with updated URLs and SHA256 checksums whenever a new release is published.

### User installation

```bash
# Add tap (one time)
brew tap USER/mxr

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
nix profile install github:USER/mxr
```

This is low effort (the Rust build is straightforward) and serves a vocal segment of the target audience.

### cargo-binstall

If the binary release naming follows the `cargo-binstall` convention (which our naming does), users can install pre-built binaries via:

```bash
cargo binstall mxr
```

No extra work needed if the GitHub Release assets follow the naming pattern `{name}-v{version}-{target}.tar.gz`, which ours do.

---

## Docs site deployment on release

The docs site deploys on every push to main (for content updates), but also on release (to ensure version numbers in docs match the release):

```yaml
  deploy-docs:
    name: Deploy docs site
    needs: release
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
      - working-directory: site
        run: npm ci
      - working-directory: site
        run: npm run build
      - name: Deploy to Cloudflare Pages
        uses: cloudflare/pages-action@v1
        with:
          apiToken: ${{ secrets.CLOUDFLARE_API_TOKEN }}
          accountId: ${{ secrets.CLOUDFLARE_ACCOUNT_ID }}
          projectName: mxr-docs
          directory: site/dist
```

---

## Required GitHub secrets

| Secret | Purpose |
|---|---|
| `CARGO_REGISTRY_TOKEN` | crates.io API token for publishing |
| `HOMEBREW_TAP_TOKEN` | GitHub PAT with push access to the homebrew-tap repo |
| `CLOUDFLARE_API_TOKEN` | Cloudflare Pages deployment |
| `CLOUDFLARE_ACCOUNT_ID` | Cloudflare account identifier |

---

## Complete release flow (end to end)

```
1. Developer finishes work, merges to main
2. CI runs on main: fmt, clippy, test, build, sqlx-check, docs build, policy sync
3. Developer updates version in Cargo.toml
4. Developer runs: git cliff --output CHANGELOG.md
5. Developer commits: git commit -m "chore: release v0.1.0"
6. Developer tags: git tag v0.1.0
7. Developer pushes: git push origin main v0.1.0
8. Tag triggers release pipeline:
   a. Verify tag version matches Cargo.toml version
   b. Build cross-compiled binaries (4 targets)
   c. Generate SHA256 checksums
   d. Publish crates to crates.io (dependency order, with propagation delays)
   e. Create GitHub Release with binaries, checksums, and changelog
   f. Update Homebrew formula (auto-PR to tap repo)
   g. Deploy docs site to Cloudflare Pages
9. Done. Users can now:
   - cargo install mxr
   - cargo binstall mxr
   - brew install mxr
   - Download binary from GitHub Releases
```

---

## Decision records

**D066: Semantic versioning with conventional commits**

**Chosen**: Semver for versions. Conventional commits for changelog generation via git-cliff.

**Why**: Semver is the Rust ecosystem standard. Conventional commits enable automated changelog generation, which saves manual effort on every release. git-cliff is a Rust-native tool that parses conventional commits into grouped changelogs. The alternative (manually writing changelogs) doesn't scale and is error-prone.

**D067: Cross-compilation via `cross` for Linux, native for macOS**

**Chosen**: Use `cross` (containerized cross-compilation) for Linux musl targets. Native compilation for macOS targets on macOS runners.

**Why**: `cross` handles the complexity of musl toolchains and cross-architecture builds (x86_64 → aarch64) inside Docker containers. macOS targets build natively on GitHub's macOS runners because cross-compiling for macOS is significantly more complex (requires macOS SDK). musl is chosen over glibc for Linux because it produces fully static binaries that run on any Linux distribution.

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

**Why**: Zero extra work. If the GitHub Release assets are named `{name}-v{version}-{target}.tar.gz` (which ours are), cargo-binstall can install pre-built binaries automatically. This gives users a fast install path (`cargo binstall mxr`) without the 5+ minute compile time of `cargo install mxr`, and it requires no infrastructure beyond what we already build.
