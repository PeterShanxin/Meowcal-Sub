#!/usr/bin/env bash
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
stage="${1:-all}"
case "$stage" in
  all|backend|frontend) ;;
  -h|--help)
    echo 'Usage: bash scripts/verify-linux.sh [all|backend|frontend]'
    echo 'Linux development checks only. Native validation requires scripts/verify.ps1 on Windows.'
    exit 0 ;;
  *) echo "Unknown stage: $stage (expected all, backend, or frontend)" >&2; exit 2 ;;
esac
if [[ $# -gt 1 || "$(uname -s)" != Linux ]]; then
  echo 'Use one stage on Linux; see CONTRIBUTING.md for Windows verification.' >&2
  exit 2
fi

require() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing $1. Install the Linux prerequisites in CONTRIBUTING.md and rerun." >&2
    exit 1
  fi
}

run() {
  printf '\n==> %s\n' "$*"
  "$@"
}

if [[ "$stage" == all || "$stage" == backend ]]; then
  for tool in cargo rustc rustfmt pkg-config; do require "$tool"; done
  if ! pkg-config --exists openssl; then
    echo 'OpenSSL development files are missing; install libssl-dev (CONTRIBUTING.md).' >&2
    exit 1
  fi
  run cargo fmt --manifest-path core/Cargo.toml -- --check
  run cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
  run rustfmt --edition 2021 --check --config skip_children=true scripts/linux-app-tests.rs
  run cargo clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings
  run cargo test --manifest-path core/Cargo.toml --locked --all-targets

  app_test_dir="$(mktemp -d)"
  trap 'rm -rf -- "$app_test_dir"' EXIT
  run rustc --edition 2021 --test -D warnings scripts/linux-app-tests.rs -o "$app_test_dir/app-tests"
  run "$app_test_dir/app-tests"
fi

if [[ "$stage" == all || "$stage" == frontend ]]; then
  require node
  require npm
  node -e 'if (process.versions.node.split(".")[0] !== "24") { console.error("Use Node.js 24 (see .node-version)"); process.exit(1); }'
  if [[ "$(npm --version)" != 11.* ]]; then
    echo 'Use npm 11 (see package.json engines).' >&2
    exit 1
  fi
  run npm ci --ignore-scripts
  run node --test scripts/tests/verify-linux.test.mjs
  for check in version:check runners:check docs:check format:check lint typecheck build:web maintainability test:frontend; do
    run npm run "$check"
  done
  run npm exec -- playwright install chromium
  run npm run test:browser:linux
  run npm audit --audit-level=high
fi

printf '\nLinux %s verification passed. Windows native validation remains separate.\n' "$stage"
