# Contributing to DevNest

Thanks for helping improve DevNest.

## Ground rules

- Keep changes inside the active implementation phase.
- Read [BEFORE_CODING.md](./BEFORE_CODING.md) before making larger changes.
- Prefer project-first UX and human-readable errors.
- Do not mix unrelated cleanup or future-phase features into the same change.

## Setup

```powershell
npm install
```

For the public website:

```powershell
npm --prefix src-webdocs install
```

## Verify before opening a PR

Use the fast path first:

```powershell
npm run verify:fast
```

This is the default verification path for most frontend and integration work because it avoids unnecessary full Rust rebuild churn.

Only run heavier Cargo checks when:

- you are working on release hardening
- your change is deep in Rust core and `verify:fast` is not enough
- or a maintainer explicitly asks for it

## Pull request expectations

- Explain the problem being solved.
- Keep write scope small.
- Mention any deferred items explicitly.
- Include manual verification notes when UI behavior changes.
- Update docs when public contracts, release behavior, or user-facing workflows change.

## Release and secrets

- Never commit updater private keys, tokens, certificates, or local release paths.
- Use `.env.example` as a reference only.
- `scripts/release-windows.ps1` is designed so forks can build without publishing to the official release feed.

## Communication

- Issues are welcome for bugs, regressions, and feature proposals.
- Keep reports concrete: expected behavior, actual behavior, reproduction steps, and screenshots/logs when useful.
