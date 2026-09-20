# DSH Desktop

A Windows-first native desktop host for DeepSeek Harness.

## Design goals

- Keep existing Harness execution and logic unchanged.
- Treat dsh --profile sdk as a black-box runtime process.
- Keep Desktop-owned files under one application data root.
- Keep Workspace files user-owned.
- Pin an exact Harness runtime version per Desktop release.
- Make temporary files owned, lease-backed, and recoverable after crashes.
- Keep renderer privileges minimal.

## Development

Requirements:
- Node.js
- pnpm
- Rust toolchain
- Python when preparing the bundled runtime

Install:
    pnpm install

Run frontend:
    pnpm dev

Run native desktop:
    pnpm tauri:dev

For local Harness development, set DSH_RUNTIME_PATH to an absolute executable path. Packaged builds expect the pinned runtime in runtime/bin.

## Runtime preparation

Windows:
    powershell -ExecutionPolicy Bypass -File scripts/prepare-runtime.ps1

The script reads the exact version from runtime/dsh-runtime.json and prepares the matching runtime artifact under runtime/bin.

## Docs

- docs/ARCHITECTURE.md
- docs/RUNTIME-UPDATES.md
- tests/protocol-contract.md
