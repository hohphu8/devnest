# Windows Native Prerequisites for DevNest

## Goal

This document defines the minimum native prerequisites required to build the `src-tauri` part of DevNest on Windows.

## Current environment assumptions

Based on recent local verification, a working Windows machine should have:

- Node.js
- npm
- cargo
- rustc
- MSVC C++ Build Tools
- Windows SDK libraries

One known Windows-specific problem is that a normal shell may still resolve `link.exe` to a Git or Laragon path instead of the MSVC linker.

Example of a bad linker path:

```text
D:\laragon\bin\git\usr\bin\link.exe
```

The repository already includes a linker wrapper to reduce this problem when the machine has the real MSVC and Windows SDK components installed.

## Required components

### Required

1. Rust toolchain: `x86_64-pc-windows-msvc`
2. Visual Studio Build Tools with `Desktop development with C++`
3. Windows 10 or Windows 11 SDK libraries

### Native pieces that must exist

- `link.exe` from MSVC
- `kernel32.lib`
- `ntdll.lib`
- `userenv.lib`
- `ws2_32.lib`
- `dbghelp.lib`

## How to verify the machine

Run:

```powershell
npm run check:env
```

The script should not report `MISSING` for:

- `MSVC toolchain root`
- `Windows SDK lib root`
- `PATH linker`

## Common failure modes

### Case 1: PATH resolves to Git's linker

Example:

```text
D:\laragon\bin\git\usr\bin\link.exe
```

That linker is not valid for the Rust target `x86_64-pc-windows-msvc`.

### Case 2: Rust exists but Windows SDK libraries do not

In that case `cargo check` usually fails with errors like:

- `could not open 'kernel32.lib'`
- `could not open 'ntdll.lib'`

## Repo-level workaround already included

The repository already contains:

- `src-tauri/.cargo/config.toml`
- `src-tauri/.cargo/msvc-linker.cmd`

The purpose of this workaround is:

- bypass a Git or Laragon `link.exe` in a normal shell
- locate the real MSVC `link.exe`
- set `LIB` to the correct MSVC and Windows SDK library paths before linking

Notes:

- This workaround does not replace missing MSVC or SDK installations.
- If the machine already has MSVC and the Windows SDK, but the shell PATH is dirty, the wrapper should allow `cargo check` to run successfully.

## When the machine is native-ready

The environment should only be considered ready when:

1. `npm run check:env` reports `OK` for `MSVC toolchain root`, `Windows SDK lib root`, and `Repo linker wrapper`
2. `cargo check` in `src-tauri` passes from a normal shell
3. Sample Tauri commands such as `ping` and `get_boot_state` build successfully

## Recommended setup flow after installing build tools

1. Run `npm run check:env`
2. Run `cargo check` inside `src-tauri`
3. Run `npm run build`
4. If all three pass, the machine is ready for normal DevNest native development work
