# GitHub setup

The branch includes issue forms, a pull-request template, generated-release-note categories, Dependabot, desktop checks, and dependency checks. README badges follow multi_platform_support and do not imply that an older release contains these features.

## Activation

Push the branch to run its push workflows. The default branch is currently main. Issue forms, Dependabot configuration, security-policy discovery, scheduled workflows, and release-note configuration generally need to be present on the default branch to become repository-wide features. Merge or selectively bring these files to main when appropriate; no default-branch change is required by this setup.

Dependabot version-update PRs explicitly target multi_platform_support. Update target-branch when development moves. Security updates and alerts remain tied to the default branch. See [GitHub documentation](https://docs.github.com/en/code-security/tutorials/secure-your-dependencies/customizing-dependabot-prs).

Dependency checks run npm audit and cargo-audit. RustSec informational warnings remain visible but do not fail the auditor by default. Each summary includes the security review; a passing run does not resolve the documented Linux glib issue.

## Repository About metadata

repository-metadata.json contains the proposed description and topics. repository-topics.json is the matching GitHub API payload. Topics are discovery tags, not release tags. The connected account has write access but cannot edit repository settings.

An administrator can use the About panel or authenticated GitHub CLI:

~~~powershell
$metadata = Get-Content .github/repository-metadata.json -Raw | ConvertFrom-Json
gh repo edit mathiasbunnens/chatgpt-quota-monitor --description $metadata.description
gh api --method PUT repos/mathiasbunnens/chatgpt-quota-monitor/topics --input .github/repository-topics.json
~~~

An administrator can also enable private vulnerability reporting. SECURITY.md includes a fallback until a private channel is available.

This setup does not change branch protection, ownership, or the default branch. No release tag is created; releases continue to require intentional v* tags.
