# Before Coding

This file is the public contributor note for DevNest.

## Product direction

- DevNest is a project-first local PHP workspace for Windows.
- The main path is: import project, detect what it needs, apply the local stack shape, then help the user fix problems in plain language.
- The app should prioritize project context over raw service management.

## Practical rules

- Keep changes small and focused.
- Do not bundle unrelated cleanup with feature work.
- Prefer readable UX over exposing raw logs or raw stderr on the main path.
- Preserve Windows-first behavior and assumptions.
- Do not commit secrets, updater keys, certificates, tokens, or machine-specific paths.

## UI and UX

- Desktop-first.
- Dense but breathable layout.
- Clear actions and clear failure states.
- Favor helpful labels and recovery guidance over technical noise.

## Backend and app behavior

- Keep business logic in the native/core layer when it belongs there.
- Avoid spreading native calls directly through the UI.
- Respect existing contracts unless the change intentionally updates them.
- If you change a user-facing flow, update public docs or README where needed.

## Verification

Use the fast verification path first:

```powershell
npm run verify:fast
```

Only move to heavier checks when the change actually needs them.
