# Security Policy

## Supported builds

Security fixes are prioritized for:

- the latest release on the `stable` channel
- the current `main` branch

Older builds may not receive fixes.

## Reporting a vulnerability

Please do not post exploitable security details in a public issue first.

Preferred flow:

1. Use GitHub private vulnerability reporting for this repository if it is enabled.
2. If private reporting is not available, contact the maintainer directly before publishing details.
3. Include clear reproduction steps, affected version, impact, and any proof-of-concept details needed to validate the report.

## What to include

- DevNest version
- Windows version
- Whether the issue affects development builds, packaged builds, or both
- Reproduction steps
- Expected impact
- Any logs, screenshots, or sample projects that help confirm the issue

## Scope notes

Security-sensitive areas include:

- updater metadata and signing flow
- local server/runtime process management
- hosts file writes and local certificate trust flow
- database backup and restore behavior
- Cloudflare tunnel integration

## Disclosure

After a fix is ready, coordinated disclosure is preferred so users have time to update.
