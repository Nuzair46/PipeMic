# Windows integration checks

Automated tests use fake audio endpoints; Linux cross-compilation cannot execute WASAPI, global shortcuts, the registry, the tray, or NSIS. Run these checks on a Windows machine with VB-CABLE before release, recording Windows build, endpoint/driver versions, negotiated sample rates, and buffer setting.

1. Load an existing config with multiple microphones and applications. Confirm selections, labels, gains, mutes, shortcuts, and layout match the previous release. Start and Stop repeatedly, including changing a source immediately before and after Stop. Routing must remain stopped until explicitly started.
2. While routing a microphone, add an unavailable application and an application whose capture activation fails. The microphone must continue; Stop and window controls must respond immediately. Repeat rapid Start/Stop and check that PipeMic's handle count and memory settle after callbacks complete.
3. Open Settings, toggle mic/app mute with global shortcuts, adjust a source, then save startup/tray options. Verify controls and topology remain current. Check changed shortcuts, launch at sign-in, minimize-to-tray, tray Quit, and closing with minimize disabled.
4. Unplug/replug a microphone while another source is playing. Start with a saved microphone absent, then connect it. Both should reconnect without restarting the mix. Restart a selected application with a new PID. A quiet but live application should retain capture and resume when playback starts.
5. Route 44.1, 48, and 96 kHz endpoints where available, using a recorded tone or timed clip. Confirm pitch, duration, stereo/mono behavior, gain, clipping, and mute. Record a long mix from independent devices to check drift, dropouts, and latency. The default source reservoir is 20 ms; source queues cap at 80 ms, plus driver/output buffering and filter delay. This is a queue bound, not a measured end-to-end latency claim.
6. Induce output backpressure or unplug the output. Confirm an existing error toast reports the failure and Stop remains responsive. Restore the output and explicitly Start. With capture pressure, confirm memory stays bounded and recovered audio is current.
7. With a backed-up test config, make the config directory temporarily unwritable. Mute/gain must still affect audio, and the existing error mechanism must report persistence failure. Restore access, change a control, quit, and confirm the last value on relaunch. Corrupt only the primary config and verify backup recovery. Corrupt both and verify the app reports an error and preserves both files rather than saving defaults.
8. On a disposable Windows VM, run a fresh NSIS install with and without VB-CABLE already present; exercise accepting and skipping the driver prompt. Upgrade/reinstall an existing PipeMic installation and confirm its saved config survives. Uninstall once retaining app data and once selecting deletion; only the latter should remove `%APPDATA%\PipeMic` and its backup. Check Start Menu/Desktop launch and installed version.
9. Start Spotify playback, add it as an application source, and Start routing. Verify its meter and audio reach the virtual output without a proxy-registration/IID error. Repeat Stop/Start and stop immediately during activation, then start again. Repeat with another application to exercise the shared process-loopback path. If activation fails, record the operation name and HRESULT shown in the error.

For a portable processing probe, run:

```sh
cargo run --manifest-path src-tauri/Cargo.toml --release --example audio_bench --no-default-features --locked
```

It reports allocation counts and CPU time for eight sources in 5 ms blocks, including a comparison with the previous mixer's allocation pattern. It excludes native I/O, scheduling, and telemetry; it does not establish Windows latency or real-time guarantees.
