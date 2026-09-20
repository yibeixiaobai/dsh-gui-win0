# Runtime Updates

The upstream Harness project is a developer preview and can introduce compatibility-breaking changes. This Desktop project therefore treats the Harness runtime as a pinned embedded component rather than a floating dependency.

## Update flow

1. Change the exact harnessVersion in runtime/dsh-runtime.json.
2. Run scripts/prepare-runtime.ps1 to fetch that exact official runtime artifact.
3. Run the Desktop frontend type check/build.
4. Run native host checks.
5. Verify the SDK initialize handshake and required notification names.
6. Verify persistence recovery using a copied test Harness Home.
7. Only then publish the new Desktop version.

## Rollback

Do not replace the currently working runtime in-place in a production updater. Stage the next application/runtime separately, validate it, switch only after health checks pass, and retain the previous release for rollback.

## Data migration

If an upstream release changes durable Session storage, migration must be tested before switching versions. Desktop must never create a parallel Session database.

## Development override

DSH_RUNTIME_PATH can point to a local dsh executable for development. The override is not persisted in Desktop settings and does not change the packaged runtime.
