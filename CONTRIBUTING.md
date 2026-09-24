# Contributing

This repository is developing Windows, Linux, and macOS support on multi_platform_support. Please target that branch for changes to the direct Codex provider and desktop interface. The default main branch may have different behavior.

## Local checks

Install Node.js 22, Rust stable, and the platform dependencies in [PLATFORMS.md](docs/PLATFORMS.md).

~~~sh
npm ci
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib
npm run tauri build -- --no-bundle
~~~

The live_codex_account_read test is ignored by default because it requires a trusted local Codex installation and a ChatGPT login. Do not run it in shared CI or upload credentials to enable it. Use the opt-in command in the README for local checks.

Keep changes focused. Include reproduction steps for fixes and tests for changed behavior. Identify which operating systems you actually tested; passing on Windows does not validate macOS or Linux. Preserve manual refresh overrides, quota-source accuracy, and accessibility.

## Security and privacy

Read [SECURITY.md](SECURITY.md). Never include auth.json, tokens, login links, device codes, or private conversation content in issues, commits, screenshots, or test fixtures. Process detection should remain metadata-only, and quota monitoring must not start agent turns.

## Releases

Tagging v* starts the release workflow. Only create a release tag intentionally after version numbers, platform checks, updater endpoints, and signing configuration have been reviewed. This tooling change does not create a release tag.

GitHub activation and repository metadata are described in [.github/README.md](.github/README.md).
