#!/usr/bin/env bash
set -euo pipefail

workflow=".github/workflows/release.yml"

live_gmail_block="$(sed -n '/name: Run live Gmail release smoke/,/cargo test -p mxr --test live_gmail_e2e/p' "$workflow")"
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

echo "release_workflow_test: ok"
