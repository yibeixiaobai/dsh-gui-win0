# Bundled Harness Runtime

DSH Desktop packages the exact runtime declared in `dsh-runtime.json` and never follows an arbitrary `dsh` executable on PATH in release builds.

Windows x64 runtime artifact:

`deepseek-harness-sdk-runtime-win-x64.exe`

The runtime receives:

- `--profile sdk`
- `DSH_HOME=<Desktop AppData>/harness`
- `cwd=<selected Workspace>`

The official runtime wheel also contains ripgrep and Office sidecars; the preparation script stages all three into `runtime/bin`.

For local development only, set `DSH_RUNTIME_PATH` to an absolute executable path. The packaged build does not use this override.
