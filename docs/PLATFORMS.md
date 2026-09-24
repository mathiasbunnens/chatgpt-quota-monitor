# Windows, Linux and macOS

All platforms now use the same direct Codex App Server provider. The browser extension is optional fallback. Codex CLI must be installed separately; the monitor discovers it or accepts an explicit executable path. Quota reads never start an agent turn.

## Desktop behavior

- Windows: a normal dashboard and tray menu. Left-click opens the dashboard; right-click shows actions. Closing hides the window when the tray is available. Quit exits and stops the owned Codex process.
- Linux: the dashboard works without an AppIndicator host. Closing exits; minimize to keep monitoring. The tray menu can reopen the window where supported.
- macOS: the AppKit menu renders the returned quota windows dynamically and opens the dashboard for account setup. It no longer starts Brave when disconnected. Closing the dashboard leaves the menu active.

The details action opens the Codex usage page in the selected browser, falling back to the default browser. Monitoring does not depend on that page.

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

The monitor starts `codex app-server` over private stdio, reads account state and quota windows, and polls every 60 seconds. Failed reads remove direct data and retry with backoff up to four minutes. Manual refresh retries immediately. Codex owns cached credentials and token refresh. New authorization is started only by an explicit sign-in action; device-code sign-in is available when enabled for the account.

The direct response is authoritative, including an empty list of windows. Browser updates and disconnects never overwrite it. On direct-source failure, available browser data is explicitly marked as fallback. Quota categories and durations come from the response; the dashboard does not require a five-hour limit.

## Packaging and updates

The extension files are embedded and extracted to the stable per-user `browser-extension` directory. Browser installation remains manual and is only needed for fallback. Codex itself is not bundled. Standard install directories and PATH are searched; `QUOTA_CODEX_BINARY` or the saved UI path can select a compatible binary. A saved path takes precedence over the environment variable. Windows requires a native `.exe`.

The updater still points to the original project. Configure your own endpoint and signing key before distributing a fork. Uninstalling the monitor does not uninstall Codex or sign out its shared account.

## Manual release checks

Verify startup with an existing login, missing executable, fresh login and cancellation, weekly-only and multiple-bucket responses, network failure/recovery, details-page opening, browser fallback, quitting and child-process cleanup, light/dark mode, and resizing. On Linux also test without a tray host. On macOS verify dynamic native menus and dashboard reopening.
