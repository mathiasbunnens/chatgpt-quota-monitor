# Security review — 2026-09-24

Scope: this branch's current Rust/React source, Tauri capabilities and CSP, process and authentication boundaries, both lockfiles, installer configuration, and GitHub workflows. This is a source/dependency review with Windows tests, not a penetration-test certification or a claim that the software is vulnerability-free.

## Findings fixed

- Removed the unauthenticated loopback HTTP quota bridge, its wildcard CORS, browser scraping extension, extraction code, setup commands, UI fallback, tiny_http dependency, and extension release assets on every platform. The monitor no longer listens on TCP 48721.
- Reduced webview capabilities to event listen/unlisten. The frontend cannot invoke generic opener or updater plugin commands; the Rust backend opens fixed details/install URLs and validated login URLs.
- Added CSP restrictions for objects, frames, base URLs, and form destinations. Scripts remain restricted to the packaged origin; inline styles are retained for progress bars.
- Login destinations require HTTPS, the exact auth.openai.com hostname, port 443, and no URL credentials. Regression tests cover lookalike hosts, userinfo, HTTP, alternate ports, and file URLs.
- Bounded Codex stdio lines to 1 MiB, queued messages to 128, and notification collection. Oversized-message regression coverage added. Requests retain a 30-second response deadline. The standalone probe also caps output and uses the temporary directory to avoid loading project-local configuration.
- Pinned GitHub Actions to resolved upstream commit hashes. Release write permission is limited to the release job. No secrets are exposed to pull-request checks.

## Dependency audit results

- npm audit: **0 known vulnerabilities**, 83 dependencies reported.
- cargo-audit 0.22.2: 532 locked dependencies checked against RustSec database commit ef8244d224cb89be53491e0c55a96c1279d9fdf1 (updated 2026-09-24). **0 entries in its vulnerabilities category**, but **7 informational warnings remain**. These warnings were not ignored or suppressed.

| Outstanding dependency | Finding | Scope / disposition |
| --- | --- | --- |
| glib 0.18.5 | [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html): unsound VariantStrIter, potentially causing undefined behavior / crashes | Linux GTK/WebKit/AppIndicator stack. Patched upstream in glib >=0.20; the current GTK 0.18 dependency graph cannot be fixed by adding a second newer glib. Requires a compatible upstream migration or a maintained, tested backport. No proof of non-reachability; do not treat this Linux finding as resolved. |
| proc-macro-error 1.0.4 | [RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html): unmaintained | Linux glib-macros / gtk3-macros build dependencies. Track the upstream GTK stack. |
| unic-char-range 0.9.0 | [RUSTSEC-2025-0075](https://rustsec.org/advisories/RUSTSEC-2025-0075.html): unmaintained | Tauri utils → urlpattern → Unicode crates. |
| unic-common 0.9.0 | [RUSTSEC-2025-0080](https://rustsec.org/advisories/RUSTSEC-2025-0080.html): unmaintained | Same upstream family. |
| unic-char-property 0.9.0 | [RUSTSEC-2025-0081](https://rustsec.org/advisories/RUSTSEC-2025-0081.html): unmaintained | Same upstream family. |
| unic-ucd-version 0.9.0 | [RUSTSEC-2025-0098](https://rustsec.org/advisories/RUSTSEC-2025-0098.html): unmaintained | Same upstream family. |
| unic-ucd-ident 0.9.0 | [RUSTSEC-2025-0100](https://rustsec.org/advisories/RUSTSEC-2025-0100.html): unmaintained | Same upstream family. |

Dependency paths verified with cargo tree for Windows and x86_64-unknown-linux-gnu. The Linux memory-safety finding is absent from the Windows runtime dependency graph. This does not establish that Windows has no other flaws.

## Trust and privacy boundaries reviewed

- Codex is a separately installed trusted executable. It is launched directly, without shell interpolation, with fixed app-server arguments and hidden console on Windows. A saved path, QUOTA_CODEX_BINARY, and PATH are user-controlled execution choices; this app does not authenticate that executable. Never select an untrusted binary. Same-user filesystem compromise is outside this app's isolation guarantees.
- Codex owns credentials and renewal. The monitor does not scrape browser profiles or read credential files. It calls account endpoints only, never creates a thread or starts a turn. OAuth/device-code authorization still needs user interaction when required.
- Dynamic refresh reads process names, parent relationships, and user identifiers locally using [sysinfo](https://docs.rs/sysinfo/0.39.6/sysinfo/). It does not request command lines, environment variables, window titles, or conversation content. Only the count is sent to the app UI. It excludes this monitor's process tree and collapses helper processes. This estimates open native instances, not busy turns; remote instances and unrecognized wrappers may be missed.
- Process detection failure falls back to plan timing. Manual refresh intervals override adaptation. Normal polling is bounded to at least 15 seconds; explicit refresh and account notifications can request earlier reads.
- Updater transport is HTTPS and signature verification remains enabled with the configured public key. The endpoint/key still trust the original repository's releases. A separately distributed fork needs its own signing identity and release endpoint. The local debug installer is not an OS-code-signed production release.
- React renders API labels as text; no raw HTML injection was found. Native macOS AppKit pointer/lifetime code was reviewed in source only and has not been sanitizer-tested on macOS.
- No tracked .pem/.key/.p12/.pfx/.env files and no tracked-file matches for the private-key, GitHub-token, and OpenAI-key patterns tested. This was not a complete Git-history or entropy-based secret scan.

## Verification and limitations

Frontend production build, Rust unit tests, and a read-only live Codex connection test were run on Windows. The live test also checks local process detection. Regression cases cover instance-tree grouping/exclusion, parent cycles, adaptive intervals and manual override, Plus quota switching, invalid auth URLs, oversized messages, and tray values.

No macOS/Linux runtime, exploitability assessment of glib, independent authentication-provider audit, fuzzing, or rendered-UI inspection was available in this Windows environment. Existing browser extensions and previously extracted files are not removed from user profiles; they are no longer used by this branch and can be removed manually.

Reproduce with npm audit, cargo audit --file src-tauri/Cargo.lock --json, cargo test --locked --manifest-path src-tauri/Cargo.toml --lib, and the ignored live_codex_account_read test when a trusted Codex login is available.

Windows installation verification: the installed executable matched the tested build after accounting for Tauri’s NSIS bundle marker. One installed app and its Codex child were running; TCP port 48721 had zero listening sockets. Final results: 11 unit tests passed, the separate live test passed, frontend production build and NSIS packaging succeeded, and the diagnostic script passed node --check.
