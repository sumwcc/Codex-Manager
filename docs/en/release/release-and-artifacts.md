# Release and product description

## Scope of application

This document describes the unified release entrance, Release product list, manual trigger parameters and common troubleshooting paths of the current warehouse.

## Unified publishing entrance

Currently the only publishing workflow:

- `.github/workflows/release-all.yml`
- The front-end `dist` will be built separately first, and then distributed to each platform for packaging and job reuse.

Trigger method:

- Only supports `workflow_dispatch`
- Will not be automatically triggered with push / pull request by default

Core input:

- `tag`: Release tag, required
- `ref`: Build baseline branch or commit, default `main`
- `prerelease`: `auto | true | false`
- Note: workflow is only responsible for packaging and publishing, and no longer includes server-side testing gates

## Product list

### Desktop

- Windows: `CodexManager_§§0§§_x64-setup.exe`
- Windows: `CodexManager-portable.exe`
- macOS: `CodexManager_§§1§§_aarch64.dmg`
- macOS: `CodexManager_§§2§§_x64.dmg`
- Linux: `CodexManager_§§3§§_amd64.AppImage`
- Linux: `CodexManager_§§4§§_amd64.deb`
- Linux: `CodexManager-linux-portable.zip`

### Service

- Windows: `CodexManager-service-windows-x86_64.zip`
- macOS: `CodexManager-service-macos-arm64.zip`
- macOS: `CodexManager-service-macos-x64.zip`
- Linux: `CodexManager-service-linux-x86_64.zip`
- Linux (web test package): `CodexManager-web-linux-x86_64.zip`

### GitHub Default attachment

GitHub Release will still automatically come with:

- `Source code (zip)`
- `Source code (tar.gz)`

## pre-release Rules

- `prerelease=auto` and `tag` includes `-`: published as pre-release
- `prerelease=auto` and `tag` do not include `-`: released as official version
- `prerelease=true|false`: Forced override of automatic judgment
- When rerunning the same `tag`, the Release metadata will be resynchronized based on this input.

## Local trigger entry

Windows Local auxiliary script:

- `scripts/rebuild.ps1`

Common usage:

```powershell
pwsh -NoLogo -NoProfile -File scripts/rebuild.ps1 `
  -AllPlatforms `
  -GitRef main `
  -ReleaseTag v0.1.9 `
  -GithubToken <token>
```

Commonly used parameters:

- `-AllPlatforms`: Trigger `release-all.yml`
- `-ReleaseTag`: Publish tag
- `-GitRef`: Build ref
- `-Prerelease`: Explicitly specify pre-release status
- `-DownloadArtifacts`: Whether to download build artifacts after triggering

## Platform differences description

### Windows

- Produce installation version and portable version at the same time
- The portable version is currently a single `exe`, and no additional layer of zip will be included.

### macOS

- The current product is `dmg`
- Since the Apple Developer account is not notarized, the first startup may still be intercepted by Gatekeeper.
- `dmg` Included:
  - `Open CodexManager.command`
  - `README-macOS-first-launch.txt`

### Linux

- Currently available on desktop are `AppImage` and `deb`
- Service version is released in compressed package form

## Recommended to check before release

1. Confirm that the version number has been synchronized through `scripts/bump-version.ps1`
2. Confirm that `CHANGELOG.md` has been updated
3. Confirm that the desktop front-end build passes: `pnpm -C apps run build`
4. Confirm that the core tests are passed: `pnpm -C apps run test`, `cargo test --workspace`
5. If changes to the gateway protocol are involved, additional `scripts/tests/gateway_regression_suite.ps1`

## Common failure troubleshooting

### Missing front-end artifacts

Priority checks:

- `apps/out/` Whether it can be built normally
- Whether the front-end build step in the workflow is completed successfully

### Release metadata is incorrect

Priority checks:

- Does `tag` include `-`
- `prerelease` Whether automatic judgment is explicitly overridden

### macOS The product can be downloaded but cannot be opened for the first time.

This is an expected limitation in the current unnotarized state and is not a workflow build failure.

Processing method:

1. First drag `CodexManager.app` to "Applications"
2. Double-click `Open CodexManager.command`
3. Or execute:

```bash
xattr -dr com.apple.quarantine /Applications/CodexManager.app
```

## Related documents

- Root Description: [README.md](../README.md)
- English description: [README.en.md](../README.md)
- Test Baseline: [TESTING.md](../TESTING.md)
- Architecture Description: [ARCHITECTURE.md](../ARCHITECTURE.md)