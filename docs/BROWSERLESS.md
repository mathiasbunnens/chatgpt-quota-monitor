# Browser-free quota monitoring investigation

Investigated 2026-09-24. The direct provider is now implemented on this branch for all three desktop platforms. This document retains the initial investigation and local evidence.

## Local evidence

On Windows, Codex CLI 0.155.0 at the installed OpenAI Codex executable successfully answered a read-only account/rateLimits/read request over stdio. No login, browser launch, thread, or agent turn was requested. The response included one codex bucket, with a 10,080-minute window and 59% used. It did not contain a five-hour window or a Luna bucket. Missing data must not be interpreted as unlimited quota.

Repeat with `node scripts/probe-codex-quota.mjs <path-to-codex-executable>`. The probe outputs only quota fields, suppresses stderr, has a 30-second timeout, and stops its own child process. It never reads credential files directly.

## Official interfaces

[App Server](https://learn.chatgpt.com/docs/app-server) documents stdio JSON-RPC, initialization, account/rateLimits/read, usage windows, optional multiple buckets, login methods, and quota notifications.

[Authentication](https://learn.chatgpt.com/docs/auth) documents cached sessions, automatic token refresh and device-code authorization. Initial authorization can occur in a browser on another device. A saved usable login removes that step on subsequent starts. API-key billing is separate from ChatGPT subscription access.

[Codex CLI](https://learn.chatgpt.com/docs/cli) documents Linux installation. The same stdio integration is applicable there; no Linux runtime test was performed in this workspace.

## Architecture and follow-up

- Launch an installed, compatible Codex binary as a hidden child process on Windows and a normal background child on Linux. Codex is currently a separately installed dependency; bundling a pinned binary remains a future packaging enhancement.
- Let Codex manage credentials. Prefer the existing user session; offer its login flow only when required. Do not scrape browser profiles or copy tokens into the frontend.
- Initialize, inspect account state, then fetch quotas. Poll conservatively with backoff, also handle notifications and sleep/resume. Never start an agent turn for quota monitoring.
- Render returned windows and bucket names dynamically. The dashboard accepts weekly-only responses and no longer requires a five-hour snapshot. Show missing/stale/error states explicitly.
- Browser retrieval, its extension and its local HTTP bridge have been removed on this branch.
- Stop the owned companion process when the monitor exits, isolate logs from credentials, and validate packaged Windows and Linux builds.

## Automation boundary

Normal monitoring can be fully browser-free. New accounts may still require interactive authorization, MFA, or later reauthentication. Device-code availability depends on account/workspace settings. This does not require browser Developer mode or loading an unpacked extension.
