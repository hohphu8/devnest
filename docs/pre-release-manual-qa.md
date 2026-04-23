# Pre-Release Manual QA

## Goal

Use this checklist before calling a DevNest desktop build release-ready.

This is a manual QA gate for:

- packaged Windows builds
- first-run setup
- core project-first flows
- service/runtime reliability
- tray/background lifecycle

Run this after the code-level gate passes:

```powershell
npm run check:release
```

## Test Environment

Prepare at least these environments:

1. Main dev machine with known-good runtimes and existing projects
2. Cleaner Windows machine or VM with no pre-linked runtimes
3. Optional conflict machine with IIS, another MySQL, or port 80/3306 already occupied

Record for each run:

- date
- build identifier
- machine name
- Windows version
- whether app is unpackaged dev run or packaged release build

## Release Gate

All items below should be marked as one of:

- `pass`
- `fail`
- `defer`

Do not call the release ready if any `fail` item blocks:

- import -> provision -> open-site
- service start/stop/restart
- close-to-tray / restore / exit
- managed runtime linking or install
- readable error handling on common failures

## Build Gate

- `npm run check:release` passes
- production frontend build emits no unexpected chunk warnings
- packaged Tauri app launches without missing-resource errors
- tray icon is visible in packaged build
- app icon, window title, and title bar controls render correctly

## First-Run Checks

- first launch works on a machine with an empty app workspace
- Settings loads without crashing when no runtimes are linked yet
- empty states stay readable and actionable
- no user-facing copy mentions internal phase, MVP, roadmap, or builder notes
- command palette opens and routes correctly

## Project Import Flow

Use at least one Laravel project and one plain PHP or WordPress project.

- pick folder works
- scan result detects framework and document root correctly
- suggested domain looks valid
- project create succeeds
- managed config generates successfully
- hosts update succeeds when permission is available
- if selected web server is already running, project finalization refreshes the site without extra manual restart
- project appears correctly in Dashboard and Projects
- `Open site`, `Open folder`, `Open terminal`, and logs actions work

## Service Lifecycle

Check Apache, Nginx, and MySQL at minimum.

- service status loads correctly after app boot
- start works from stopped state
- stop works from running state
- restart works from running state
- service PID and port refresh correctly after state changes
- service logs can be opened and read
- readable error is shown when runtime path is missing
- readable error is shown when port is already in use

## Startup Automation

- launching the app auto-starts MySQL if MySQL was stopped
- launching the app auto-starts the preferred web server if it was stopped
- app does not try to start both Apache and Nginx together
- startup auto-start failure does not stop the app from opening
- after auto-start, Services page reflects actual running state

## Tray And Background Behavior

- clicking the window close button hides the app instead of exiting
- app remains running in the tray after close
- left-click or double-click tray icon restores the main window
- right-click tray menu shows `Open DevNest` and `Exit`
- `Exit` from tray fully terminates the app
- reopening the app after tray exit starts cleanly
- minimize button still minimizes normally
- maximize / restore still works

## Diagnostics And Errors

- diagnostics run for a healthy project without crashing
- healthy project result uses readable success wording
- port conflict returns a human-readable explanation
- missing runtime returns a human-readable explanation
- MySQL startup failure returns a human-readable explanation
- no raw stderr is dumped into primary UI surfaces

## Runtime Management

Check at least one runtime in each family already supported by the build under test.

- link local PHP runtime works
- set active PHP runtime works
- web server runtime link/import works
- MySQL runtime link/import works
- removing a runtime reference updates UI state cleanly
- runtime inventory survives app restart

## PHP Tooling

- PHP extensions list loads for an active PHP runtime
- enable/disable extension persists correctly
- function enable/disable persists correctly
- invalid or missing extension package surfaces a readable error
- phpMyAdmin install flow completes when package source is available
- `phpmyadmin.test` opens when phpMyAdmin is active and web server is running

## Reliability / Recovery

- reliability screen loads on a normal workspace
- runtime repair / inspection actions do not corrupt existing linked runtimes
- backup metadata flow completes
- restore metadata flow completes on a disposable test workspace
- app restart after restore still shows valid project and runtime state

## Persistence Checks

- linked runtimes remain linked after restart
- imported projects remain visible after restart
- service status rehydrates correctly after restart
- optional tools remain visible after restart
- tunnel and hostname state still loads for existing records

## Packaged Windows Checks

- packaged app starts outside the dev repo
- resources are found correctly in packaged build
- managed download/install flows still resolve resource paths correctly
- no obvious SmartScreen / antivirus false positive on first launch
- app can read and write its app-data workspace correctly

## Final Sign-Off

Record before ship:

- build identifier:
- QA owner:
- test machines:
- known deferred items:
- release blockers:
- ship decision:

Recommended ship decision values:

- `ship`
- `ship with known issues`
- `hold`
