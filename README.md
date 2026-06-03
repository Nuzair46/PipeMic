<center>
  <h1 align="center">PipeMic</h1>
  <h4 align="center">Route microphones and selected app audio into one virtual microphone.</h4>
  <h5 align="center">Built for Windows stream, call, and voice-chat setups that need a controllable mixed mic feed</h5>
  <p align="center">
    <a href="https://github.com/Nuzair46/PipeMic/releases">
      <img src="src-tauri/icons/icon.svg" alt="PipeMic logo" width="180" />
    </a>
  </p>
</center>

<p align="center">
  <a href="https://github.com/Nuzair46/PipeMic/actions/workflows/ci-build-release.yml"><img alt="Release Build and Publish" src="https://github.com/Nuzair46/PipeMic/actions/workflows/ci-build-release.yml/badge.svg" /></a>
  <img alt="Downloads" src="https://img.shields.io/github/downloads/Nuzair46/PipeMic/total.svg" />
  <img alt="Latest Release" src="https://img.shields.io/github/v/release/Nuzair46/PipeMic?display_name=tag" />
  <img alt="Platform" src="https://img.shields.io/badge/Platform-Windows%2010%2F11-0078D4?logo=windows&logoColor=white" />
</p>

<p align="center">
  <a href="https://github.com/Nuzair46/PipeMic/releases"><strong>Download Latest Release</strong></a>
  ·
  <a href="#quick-start"><strong>Quick Start</strong></a>
  ·
  <a href="#troubleshooting"><strong>Troubleshooting</strong></a>
</p>

## What Is PipeMic?

PipeMic is a voicemeter alternative for routing multiple physical microphones plus selected application audio into a virtual microphone device.

PipeMic lets you:

- Add one or more physical microphones
- Add selected application sources such as games, music players, or chat apps
- Adjust gain and mute state per source
- Control master output gain
- Downmix the final route to mono
- Send the mixed output to `CABLE Input (VB-Audio Virtual Cable)`
- Use global hotkeys for mic mute, app mute, and start/stop routing
- Start with Windows and minimize to the system tray

## Download & Install (End Users)

1. Open the [Releases page](https://github.com/Nuzair46/PipeMic/releases)
2. Download the latest `PipeMic_*_x64-setup.exe` installer
3. Run the installer
4. If prompted, follow the VB-CABLE installer prompts.
5. Reboot Windows if the VB-CABLE installer asks you to
6. Launch `PipeMic` from Start Menu or Desktop

PipeMic bundles the official VB-CABLE Windows driver package from VB-Audio. VB-CABLE is donationware; if it is useful to you, please consider supporting VB-Audio.

VB-CABLE is not necessarily required to run PipeMic but it is required to route audio into a virtual microphone device. If you want to use PipeMic without VB-CABLE, you can skip the VB-CABLE installer and just use the physical microphone routing features. Or you can use some other virtual audio cable driver, but you will need to configure PipeMic to use that driver's input and output devices instead of the bundled VB-CABLE ones.

VB-CABLE: https://www.vb-cable.com

## Quick Start

1. Open `PipeMic`
2. In `Physical Microphones`, add the microphone devices you want in the mix
3. In `Application Sources`, add any running apps you want to route into the mix
4. In `Output`, set `Virtual Mic` to `CABLE Input (VB-Audio Virtual Cable)`
5. In your target app, choose `CABLE Output (VB-Audio Virtual Cable)` as the microphone/input device
6. Click `Start`
7. Adjust source gain, mute buttons, and master gain as needed

## Notes (Important)

- Windows only
- Application capture requires Windows 10 build 20348 or later
- Older Windows builds can still run the UI and route microphones, but selected-app capture may fail
- PipeMic filters application sessions to clear `.exe` sources and hides PipeMic itself to avoid feedback loops
- Config is stored at `%APPDATA%/PipeMic/config.json` on Windows

## Troubleshooting

### I do not see CABLE Input or CABLE Output

- Reboot Windows after the VB-CABLE installer finishes
- Open Windows sound settings and check both input and output device lists
- If the devices are still missing, install VB-CABLE manually from https://www.vb-cable.com

### PipeMic says to select a virtual cable output

- In `Output`, choose `CABLE Input (VB-Audio Virtual Cable)`
- In your chat, recording, or streaming app, choose `CABLE Output (VB-Audio Virtual Cable)` as the microphone

### An application is not listed

- Make sure the application is running and has an active audio session
- Play audio in the application for a moment, then refocus PipeMic
- PipeMic only lists clean application executable sessions, not system paths or expired sessions

### A saved application source is silent after restarting the app

- Start playback in that application again
- PipeMic reconnects by executable name when the session becomes active

### Selected-app capture fails

- Confirm Windows is build 20348 or later
- If you are on an older Windows build, use microphone-only routing or update Windows

## FAQ

### Does PipeMic create a virtual microphone by itself?

No. PipeMic routes audio into a virtual cable device. The installer bundles VB-CABLE and opens its driver installer when VB-CABLE is not already present.

### Which PipeMic output should I select?

Select `CABLE Input (VB-Audio Virtual Cable)` in PipeMic. Then select `CABLE Output (VB-Audio Virtual Cable)` as the microphone in the app that should receive the mixed feed.

### Can PipeMic route multiple apps at once?

Yes. Add each running application in `Application Sources`, then adjust gain and mute state per source.

## For Developers

<details>
  <summary>Build / Dev / CI details</summary>

### Project Layout

- `src/` React UI
- `src-tauri/` Tauri desktop app, Windows backend, installer config, and Rust tests
- `src-tauri/vendor/vb-cable/` vendored VB-CABLE driver package used by the NSIS installer
- `.github/workflows/` manual Windows release workflow
- `tools/version-bump.mjs` version synchronization helper

### Build Locally (Windows)

Requirements:

- Node.js 20+
- `yarn`
- Rust stable
- Visual Studio Build Tools 2022 + Windows SDK

Commands:

```bash
yarn install
rustup target add x86_64-pc-windows-msvc
yarn tauri dev
```

Build NSIS installer:

```bash
yarn tauri build --bundles nsis
```

Output:

```text
src-tauri/target/release/bundle/nsis/*setup.exe
```

### Checks

```bash
yarn typecheck
yarn build
cargo test --manifest-path src-tauri/Cargo.toml
```

For core Rust checks without the full Tauri app feature:

```bash
cd src-tauri
cargo test --lib --no-default-features
cargo check --target x86_64-pc-windows-msvc --lib --no-default-features
```

The full Tauri app target needs the platform runtime toolchain:

- Linux host builds need GTK/WebKit development packages
- Windows cross-checks need a Windows resource compiler such as `llvm-rc`

### CI / Release

- Workflow: `.github/workflows/ci-build-release.yml`
- Manual release workflow runs via `workflow_dispatch` and takes a version input
- Release pipeline updates these files together before building:
  - `package.json`
  - `src-tauri/Cargo.toml`
  - `src-tauri/tauri.conf.json`
- Release pipeline commits the version bump, creates tag `vX.Y.Z`, builds the Windows NSIS installer, and publishes the GitHub Release
- Release artifacts are limited to `src-tauri/target/release/bundle/nsis/*setup.exe`

Release process:

1. Make sure your release commit is on `main`.
2. Open `Actions` -> `Release Build and Publish` -> `Run workflow`.
3. Enter a version such as `0.2.0`, or a bump kind: `patch`, `minor`, `major`.
4. Run the workflow.
5. The workflow bumps all version files, commits the change, creates the tag, builds the Windows NSIS installer, and publishes the GitHub Release.

### Version Helper

Check that version files are synchronized:

```bash
yarn version:bump --check
```

Bump versions locally:

```bash
yarn version:bump patch
yarn version:bump minor
yarn version:bump major
yarn version:bump 0.2.0
```

</details>

## License

PipeMic is released under the MIT License. See [LICENSE](LICENSE).

Third-party notices are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
