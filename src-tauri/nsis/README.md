# Installer template maintenance

The template was introduced in PipeMic commit `f45ab8e` and adjusted in `96dadf0`. Its original upstream revision was not recorded. The current file has been compared against the [Tauri CLI 2.11.2 template](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.11.2/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi), matching the CLI version locked in `yarn.lock`.

That upstream file has SHA-256 `ee84148e405adc4d736a46456dd8345a644751bd1f28a335dd7fd833a32d7c3e`. The complete customization set relative to it is:

- Add the conditional signed-plugin search path before NSIS includes.
- Skip the maintenance page; the existing `NSIS_HOOK_PREINSTALL` performs PipeMic updates in place.
- Insert the optional VB-CABLE page supplied by `installer-hooks.nsh` before Finish.
- Remove `$APPDATA\PipeMic` alongside Tauri's bundle-identifier directories when the user selects the existing uninstall option to delete app data. This matches `config::app_config_dir()` and includes the primary config and backup.

On a Tauri CLI upgrade, fetch the template from that exact release, compare it with this file, and reapply only these changes. Keep driver detection and VB-CABLE installation behavior in the hooks file. Update the comparison version and checksum here, run the Windows NSIS build, and perform the [installer smoke checks](../../tests/WINDOWS_SMOKE.md). Avoid replacing this file wholesale with a template from `dev`.
