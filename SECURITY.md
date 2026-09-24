# Security policy

## Scope and known findings

The multi_platform_support branch uses the direct Codex provider and removes browser quota collection. Older releases may use a different architecture. See the dated [security review](docs/SECURITY_REVIEW.md) for tested scope and outstanding dependency findings, including the Linux glib warning. A passing workflow is not a security certification.

## Reporting a vulnerability

Please do not publish credentials, exploit details, or sensitive logs in a public issue.

If the repository's Security tab offers **Report a vulnerability**, use that private reporting channel. Private reporting availability is controlled by repository administrators; this policy does not assume it is enabled. Otherwise, open a minimal issue requesting a private contact channel, without vulnerability details or personal data, and wait for a maintainer to provide one.

Once a private channel is available, include affected versions/platforms, impact, and a minimal sanitized reproduction. Do not send real account tokens, auth.json, OAuth URLs, or device codes. No response-time or supported-release guarantee is implied by this policy.
