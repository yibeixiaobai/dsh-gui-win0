# DSH Desktop Architecture

DSH Desktop is a native host around the existing DeepSeek Harness runtime. It does not import Harness internals and does not modify Agent, Cordis, Session, Sandbox, Storage, or Shell logic.

## Process boundary

Renderer
  -> Tauri IPC
Desktop Core
  -> stdio newline-delimited JSON-RPC
dsh --profile sdk
  -> existing Harness runtime

The current SDK wire contract used by this host is:
- client -> runtime: initialize, session/prompt, shutdown
- runtime -> client: session.event, session.status, subagent.started, subagent.finished

## File ownership

Desktop-owned:
- desktop/settings.json
- temp/
- logs/
- future updater downloads and crash artifacts

Harness-owned:
- harness/ is passed as DSH_HOME

Workspace-owned:
- the selected Workspace is referenced only by path
- Desktop never creates its own metadata files inside Workspace

## Temporary files

Desktop temporary directories use an ownership lease. Startup only removes stale directories containing a valid Desktop lease marker. Unknown directories are never recursively deleted.

## Runtime version policy

runtime/dsh-runtime.json pins the exact Harness runtime artifact.

A release is treated as a tested tuple:
desktop version + exact Harness runtime version + protocol contract

The app never follows dsh on PATH automatically. DSH_RUNTIME_PATH exists only for local development.

## Lifecycle

Start:
1. validate Workspace
2. resolve pinned runtime
3. verify runtime artifact identity
4. launch dsh with --profile sdk
5. set DSH_HOME to the Desktop-owned Harness root
6. use stdin/stdout JSON-RPC

Stop:
1. mark runtime stopping
2. send protocol shutdown
3. wait up to five seconds
4. kill the direct child only as a last resort

## Non-goals

Desktop does not duplicate session transcripts, copy Workspace files, expose a localhost HTTP service, create project-directory metadata, silently upgrade the runtime, or maintain a second Harness plugin/package system.
