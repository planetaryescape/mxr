#!/usr/bin/env bash
set -euo pipefail

workflow=".github/workflows/release.yml"

live_gmail_block="$(sed -n '/name: Run live Gmail release smoke/,/cargo test -p mxr --test integration live_gmail_e2e::/p' "$workflow")"
if ! grep -Fq 'skipping live provider smoke' <<<"$live_gmail_block"; then
    echo "Release workflow must skip live Gmail smoke when test credentials are missing." >&2
    exit 1
fi
if grep -Fq 'exit 1' <<<"$live_gmail_block"; then
    echo "Release workflow must not fail the release when live Gmail smoke credentials are missing." >&2
    exit 1
fi

apple_import_block="$(sed -n '/name: Import Apple Developer ID certificate/,/name: Sign CLI binaries/p' "$workflow")"
if ! grep -Fq 'macOS binary will be unsigned' <<<"$apple_import_block"; then
    echo "Release workflow must degrade to unsigned macOS binaries when Apple signing secrets are absent." >&2
    exit 1
fi
if grep -Fq 'required for macOS releases' <<<"$apple_import_block"; then
    echo "Release workflow must not require Apple signing secrets to publish release artifacts." >&2
    exit 1
fi

sign_block="$(sed -n '/name: Sign CLI binaries/,/name: Package CLI archive/p' "$workflow")"
if ! grep -Fq 'cannot sign without an identity name' <<<"$sign_block"; then
    echo "Release workflow must skip signing gracefully when APPLE_DEVELOPER_ID is absent." >&2
    exit 1
fi
if grep -Fq 'exit 1' <<<"$sign_block"; then
    echo "Release workflow must not fail when APPLE_DEVELOPER_ID is absent." >&2
    exit 1
fi

notary_block="$(sed -n '/name: Notarize macOS binary/,/name: Generate CLI SHA256 checksum/p' "$workflow")"
if ! grep -Fq "APPLE_CODESIGNED == 'true'" <<<"$notary_block"; then
    echo "Release workflow must only notarize binaries after codesigning succeeded." >&2
    exit 1
fi
if ! grep -Fq 'Notarization secrets not set' <<<"$notary_block"; then
    echo "Release workflow must skip notarization gracefully when Apple notarization secrets are absent." >&2
    exit 1
fi
if grep -Fq 'exit 1' <<<"$notary_block"; then
    echo "Release workflow must not fail when notarization secrets are absent." >&2
    exit 1
fi

# A release must stay a draft until its assets are attached: release-please makes
# the draft and release.yml publishes it only via the upload-then-publish script.
if ! grep -Fq '"draft": true' release-please-config.json || ! grep -Fq '"force-tag-creation": true' release-please-config.json; then
    echo "release-please must create releases as drafts and still create the tag." >&2
    exit 1
fi
if grep -Fq 'softprops/action-gh-release' "$workflow"; then
    echo "Release workflow must publish through scripts/publish_github_release.sh, not an action that publishes before assets exist." >&2
    exit 1
fi
if ! grep -Fq 'scripts/publish_github_release.sh' "$workflow"; then
    echo "Release workflow must publish through scripts/publish_github_release.sh." >&2
    exit 1
fi

publish_job="$(sed -n '/^  github-release:/,/^  homebrew:/p' "$workflow")"
if ! grep -Fq "needs.desktop.result == 'success'" <<<"$publish_job" \
    || ! grep -Fq "needs.plan.outputs.has_artifacts != 'true' && needs.desktop.result == 'skipped'" <<<"$publish_job"; then
    echo "Artifact releases must require the desktop job before publishing; only artifact-free releases may skip it." >&2
    exit 1
fi
# shellcheck disable=SC2016 # literal workflow expression, not a shell variable
if ! grep -Fq 'group: release-publish-${{ github.ref }}' <<<"$publish_job" || ! grep -Fq 'cancel-in-progress: false' <<<"$publish_job"; then
    echo "github-release must serialize publishers per tag without cancelling one mid-upload." >&2
    exit 1
fi

desktop_notary_block="$(sed -n '/name: Notarise and staple the dmg/,/name: Verify the signature/p' .github/workflows/desktop-signed.yml)"
if [[ "$(grep -Fc 'xcrun notarytool submit "$DMG"' <<<"$desktop_notary_block")" -ne 1 ]] \
    || ! grep -Fq "if: env.NOTARIZED == 'true'" <<<"$desktop_notary_block" \
    || ! grep -Fq -- '--key "$NOTARIZE_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER"' <<<"$desktop_notary_block" \
    || ! grep -Fq -- '--wait' <<<"$desktop_notary_block" \
    || ! grep -Fq 'bash scripts/staple_dmg.sh "$DMG"' <<<"$desktop_notary_block"; then
    echo "Desktop notarization must retain one submit/wait and use the bounded stapling helper." >&2
    exit 1
fi
spec_lint_block="$(sed -n '/^  spec-lint:/,$p' .github/workflows/openapi-conformance.yml)"
if ! grep -Fq 'timeout-minutes: 20' <<<"$spec_lint_block" \
    || ! grep -Fq 'run: schemathesis run --dry-run --validate-schema=true' <<<"$spec_lint_block"; then
    echo "OpenAPI spec lint must retain validation with a 20-minute job timeout." >&2
    exit 1
fi
bash scripts/staple_dmg_test.sh

echo "release_workflow_test: ok"
