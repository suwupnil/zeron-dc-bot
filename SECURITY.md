# Security Policy

## Supported Versions

We provide security updates and patches for the following versions of `zeron-dc-bot`:

| Version | Supported          |
| :------ | :----------------- |
| `0.1.x` | :white_check_mark: |
| `< 0.1` | :x:                |

---

## Reporting a Vulnerability

The **zeron-dc-bot** team takes the security of our application, infrastructure, and user data very seriously.

If you believe you have found a security vulnerability in this project, **please do not report it through a public GitHub issue, pull request, or public Discord message.**

Instead, please submit your vulnerability report through one of the following confidential channels:

1. **GitHub Private Vulnerability Reporting:**
   Use the [Report a Vulnerability](https://github.com/suwupnil/zeron-dc-bot/security/advisories/new) button in the repository Security tab.
2. **Direct Security Contact:**
   Email the project maintainer directly at [`sw@pnil.in`](mailto:sw@pnil.in) with the subject line:
   `[SECURITY VULNERABILITY] zeron-dc-bot: <Brief Summary>`.

### What to Include in Your Report

To help us investigate and triage your report quickly, please include:
- A descriptive summary of the vulnerability.
- Detailed step-by-step instructions or proof-of-concept (PoC) demonstrating how to trigger the issue.
- Potential impact (e.g., unauthorized access, denial of service, token exposure).
- Affected version(s) and operating environment (OS, Rust version, Zeron version).
- Any proposed mitigations or patch suggestions, if known.

---

## Response Timeline & Disclosure Process

When a vulnerability is reported:
1. **Initial Acknowledgement:** We will acknowledge receipt of your report within **48 hours**.
2. **Assessment & Confirmation:** We will investigate and provide an initial assessment within **5 business days**.
3. **Remediation & Patching:** Once confirmed, we will prioritize creating a patch and testing the fix across supported platforms.
4. **Coordinated Disclosure:** We will coordinate with the reporter to agree on a public advisory date, typically within **30 days** of confirming the issue, or sooner depending on severity.

---

## Security Best Practices for Operators

When deploying and operating `zeron-dc-bot`, we recommend adhering to the following guidelines:

- **Protect Sensitive Tokens:** Never share or commit your `DISCORD_TOKEN`, `CLOUDFLARE_API_TOKEN`, or any other credentials stored in `.env`.
- **Restrict File System Permissions:** Run the bot with a dedicated non-root user (e.g., `ubuntu` or `zeron`) with minimal necessary file permissions.
- **Isolated Execution Directory:** Restrict `/spawn` usage to directories intended for AI coding workspaces to avoid inadvertent modification of sensitive system files.
- **Subprocess Safety:** Communication with the local Zeron engine operates over stdio JSON-RPC. Keep the Zeron engine executable up to date and verify its origin.
