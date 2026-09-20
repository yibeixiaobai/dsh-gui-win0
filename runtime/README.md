# Bundled Harness Runtime

DSH Desktop does not follow a dsh executable on PATH.

The packaged build expects the exact runtime declared in `dsh-runtime.json`:

`runtime/bin/deepseek-harness-sdk-runtime-windows-x64.exe`

Development can override the executable with an absolute `DSH_RUNTIME_PATH`.

The runtime receives:

- `--profile sdk`
- `DSH_HOME=<Desktop AppData>/harness`
- `cwd=<selected Workspace>`

Desktop owns neither Harness session storage nor Workspace files.
