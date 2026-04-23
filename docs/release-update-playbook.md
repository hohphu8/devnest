# Release Update Playbook

## Goal

Ship a packaged Windows DevNest build that can self-update through:

- GitHub Releases for signed installer/update artifacts
- one stable metadata endpoint for the app updater

## Current contract

- Release channel: `stable`
- Official GitHub release repo: `hohphu8/devnest`
- Default metadata endpoint for GitHub-only delivery: `https://github.com/hohphu8/devnest/releases/latest/download/stable.json`
- Release assets are hosted on GitHub Releases
- Metadata JSON is uploaded as a release asset named `stable.json`
- The release repo must be public if the packaged app is expected to self-update without authentication

## Build-time inputs

These values must stay outside the repo in CI secrets or local release environment variables:

- `DEVNEST_UPDATER_PUBLIC_KEY`
- `DEVNEST_UPDATE_ENDPOINT`
- `DEVNEST_RELEASE_CHANNEL`

Optional for the automation script:

- `DEVNEST_UPDATER_KEY_PATH`
- `DEVNEST_UPDATER_KEY_PASSWORD`
- `DEVNEST_GITHUB_REPO`
- `DEVNEST_RELEASE_ASSET_URL`
- `DEVNEST_METADATA_PUBLISH_PATH`

Recommended values:

- `DEVNEST_RELEASE_CHANNEL=stable`
- `DEVNEST_GITHUB_REPO=hohphu8/devnest`
- `DEVNEST_UPDATE_ENDPOINT=https://github.com/hohphu8/devnest/releases/latest/download/stable.json`

For forks:

- point `DEVNEST_GITHUB_REPO` at your own release repo
- point `DEVNEST_UPDATE_ENDPOINT` at your own `stable.json`
- or leave publishing vars unset if you only want a local packaged build

Prefer environment variables when launching through `npm run release:windows` on Windows. npm can interpret extra `-Flag value` pairs as npm config instead of PowerShell script parameters.

## Signing key setup

Generate the updater keypair once on a secure machine and keep the private key outside the repo.

Example:

```powershell
cargo tauri signer generate -w ~/.tauri/devnest-updater.key
```

Use:

- private key only for signing release artifacts
- public key as `DEVNEST_UPDATER_PUBLIC_KEY` during packaged release builds

## Expected Windows assets

With `bundle.createUpdaterArtifacts=true`, Tauri produces signed updater artifacts alongside the installer bundles.

For the current Windows flow, publish the NSIS updater artifact and its signature:

- `DevNest_<version>_x64-setup.exe`
- `DevNest_<version>_x64-setup.exe.sig`

If the generated file names differ slightly because of Tauri/bundler output, keep the signed artifact and `.sig` pair consistent and update the metadata URL accordingly.

## Metadata shape

The updater endpoint must return JSON in this shape:

```json
{
  "version": "0.2.0",
  "notes": "Short release summary shown in Settings.",
  "pub_date": "2026-04-20T12:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "base64-signature-from-.sig-file",
      "url": "https://github.com/<owner>/<repo>/releases/download/v0.2.0/DevNest_0.2.0_x64-setup.exe"
    }
  }
}
```

Rules:

- `version` must match the packaged app version
- `pub_date` should be RFC3339 UTC
- `signature` must be the exact contents of the `.sig` file
- `url` must be a direct HTTPS download URL
- `windows-x86_64` is the platform key for current packaged DevNest Windows builds

## Release steps

Shortcut:

```powershell
npm run bump:version -- 0.2.0
npm run release:windows
```

The script:

- ensures the updater keypair exists
- injects the updater endpoint and updater public key into the release build override
- signs updater artifacts
- builds the Windows installer
- collects the installer, updater artifact, signature, public key, and metadata into `dist/release/windows/<version>/`
- optionally copies metadata to `DEVNEST_METADATA_PUBLISH_PATH`
- uploads GitHub Release assets when `gh` is installed and `DEVNEST_GITHUB_REPO` is configured:
  - installer/updater artifact
  - `.sig`
  - `stable.json`
- skips metadata publishing automatically when no release repo or asset URL is configured

1. Bump app version in all tracked version files:

```powershell
npm run bump:version -- 0.2.0
```
2. Export release environment variables:

```powershell
$env:DEVNEST_RELEASE_CHANNEL="stable"
$env:DEVNEST_UPDATE_ENDPOINT="https://github.com/hohphu8/devnest/releases/latest/download/stable.json"
$env:DEVNEST_UPDATER_KEY_PATH="$HOME\.tauri\devnest-updater.key"
$env:DEVNEST_UPDATER_KEY_PASSWORD="<optional-password>"
$env:DEVNEST_GITHUB_REPO="hohphu8/devnest"
$env:DEVNEST_METADATA_PUBLISH_PATH="D:\deploy\updates\stable.json"
```

3. Build the packaged release:

```powershell
npm run release:windows
```

4. The script stages the Windows release set in `dist/release/windows/<version>/`
5. If `gh` is installed and authenticated, the script creates or updates the GitHub Release for `v<version>` and uploads:
   - installer/updater artifact
   - `.sig`
   - `stable.json`
6. The script generates the metadata file:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/generate-updater-metadata.ps1 `
  -Version "0.2.0" `
  -AssetUrl "https://github.com/<owner>/<repo>/releases/download/v0.2.0/DevNest_0.2.0_x64-setup.exe" `
  -SignaturePath "src-tauri/target/release/bundle/nsis/DevNest_0.2.0_x64-setup.exe.sig" `
  -NotesPath "release-notes/0.2.0.md" `
  -OutputPath "dist/updates/stable.json"
```

7. If you are not using GitHub Releases as the metadata host, publish `stable.json` to the metadata host at your chosen HTTPS URL.
8. Verify that the metadata URL returns the new JSON over HTTPS

## Verification steps

1. Install version `N` on a clean Windows machine.
2. Publish version `N+1` through the flow above.
3. Open `Settings > General`.
4. Click `Check Updates`.
5. Confirm DevNest reports `N+1`.
6. Click `Download and Install`.
7. Finish the installer handoff if Windows prompts.
8. Reopen DevNest and confirm:
   - app version is `N+1`
   - SQLite app data still exists
   - tracked projects and managed runtime state remain intact

## Failure checklist

- Network error:
  - confirm metadata endpoint is reachable over HTTPS
  - confirm GitHub asset URL is public and direct-download capable
  - confirm the release repo is public when using GitHub Releases as the metadata host
- Signature rejected:
  - confirm `DEVNEST_UPDATER_PUBLIC_KEY` matches the private key used to sign the artifact
  - confirm metadata `signature` matches the `.sig` file exactly
- Unsupported target:
  - confirm metadata includes `platforms.windows-x86_64`
- Update not detected:
  - confirm `version` is newer than the installed version
  - confirm the running build points at the expected `DEVNEST_UPDATE_ENDPOINT`
