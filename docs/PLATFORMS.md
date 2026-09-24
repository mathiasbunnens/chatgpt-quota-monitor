# Windows, Linux and macOS

All platforms now use the same direct Codex App Server provider. There is no browser retrieval or local HTTP listener. Codex CLI must be installed separately; the monitor discovers it or accepts an explicit executable path. Quota reads never start an agent turn.

## Desktop behavior

- Windows: a normal dashboard and tray menu. Left-click opens the dashboard; right-click shows actions. Closing hides the window when the tray is available. Quit exits and stops the owned Codex process.
- Linux: the dashboard works without an AppIndicator host. Closing exits; minimize to keep monitoring. The tray menu can reopen the window where supported.
- macOS: the app is menu-bar only. AppKit renders the returned quota windows dynamically, and a Connection item appears only while the account is disconnected. No dashboard window is created.

The details action opens the Codex usage page in the default browser. Monitoring does not depend on that page.

## Development prerequisites

Install Node.js 22, npm, Rust stable and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). Windows needs Visual Studio C++ build tools, the Windows SDK and WebView2. macOS needs Xcode command-line tools.

Ubuntu 22.04 dependencies:

```sh
sudo apt-get update
sudo apt-get install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev librsvg2-dev libayatana-appindicator3-dev patchelf
npm ci
npm run tauri dev
```

## Builds and checks

```sh
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml
npm run tauri build
```

Build each package on its target OS. Tauri selects NSIS on Windows, Debian/AppImage on Linux, and app/DMG on macOS. Signed updater artifacts require the signing key. Use `npm run tauri build -- --no-bundle` to validate an executable without packaging. Release workflows cover all three platforms. Linux and macOS builds/runtime must be verified by their runners; development here was on Windows.

If Rust reports hard-link failures on a Windows drive, run `$env:CARGO_INCREMENTAL = "0"` in PowerShell before the dev/build command. This changes caching, not compiler warning visibility.

## Provider behavior

The monitor starts `codex app-server` over private stdio, reads account state and quota windows, and polls dynamically based on plan and estimated open instances (see README). Failed reads remove direct data and retry with backoff of at least the configured interval. Manual refresh retries immediately. Codex owns cached credentials and token refresh. New authorization is started only by an explicit sign-in action; device-code sign-in is available when enabled for the account.

The direct response is authoritative, including an empty list of windows. Quota categories and durations come from the response; missing windows are never fabricated.

## Packaging and updates

Codex itself is not bundled. Standard install directories and PATH are searched; QUOTA_CODEX_BINARY or the saved UI path can select a compatible binary. A saved path takes precedence. Only choose a trusted local executable. Windows requires a native .exe.

Update checks use one dedicated popup. When a release is available, the same popup shows download progress and then offers Quit and restart. The updater still points to the original project. Configure your own endpoint and signing key before distributing a fork. Uninstalling the monitor does not uninstall Codex or sign out its shared account.

## Manual release checks

Verify startup with an existing login, missing executable, fresh login and cancellation, weekly-only and multiple-bucket responses, network failure/recovery, details-page opening, quitting and child-process cleanup, light/dark mode, and resizing. On Linux also test without a tray host. On macOS verify menu-only startup, the conditional Connection item, and the updater popup.

Process detection counts same-user top-level Codex native process trees, excluding this monitor and its descendants. It does not count chats, read command lines, or prove a turn is running. Unrecognized wrappers and remote instances may be missed; permission failures fall back to plan defaults.
