# Build release and script instructions

## Local development

### Front-end
```bash
pnpm -C apps install
pnpm -C apps run dev
pnpm -C apps run test
pnpm -C apps run test:ui
pnpm -C apps run build
```

### Rust
```bash
cargo test --workspace
cargo build -p codexmanager-service --release
cargo build -p codexmanager-web --release
cargo build -p codexmanager-start --release

# Enter the front-end static resources into codexmanager-web (binary single file)
pnpm -C apps run build
cargo build -p codexmanager-web --release --features embedded-ui
```

## Tauri Packaging

### Windows
```powershell
pwsh -NoLogo -NoProfile -File scripts/rebuild.ps1 -Bundle nsis -CleanDist -Portable
```

### Linux / macOS
```bash
./scripts/rebuild-linux.sh --bundles "appimage,deb" --clean-dist
./scripts/rebuild-macos.sh --bundles "dmg" --clean-dist
```

## GitHub Actions
The current unified publishing entrance is `.github/workflows/release-all.yml`, the triggering method is `workflow_dispatch`, and it will not be triggered automatically.

### `release-all.yml`
- Purpose: One-click publishing Desktop + Service full platform products
- Build platform: `Windows`, `macOS（dmg）`, `Linux`
- The front-end `dist` is first built separately and then packaged and reused by each platform job.
- Input:
  - `tag`: required
  - `ref`: Default `main`
  - `prerelease`: default `auto`, optional `auto|true|false`
- Behavior: Only perform packaging and publishing, no more server-side test gates

## Release product list

### Desktop
- Windows: `CodexManager_§§0§§_x64-setup.exe`, `CodexManager-portable.exe`
- macOS: `CodexManager_<version>_aarch64.dmg`, `CodexManager_§§2§§_x64.dmg`
- Linux: `CodexManager_<version>_amd64.AppImage`, `CodexManager_§§4§§_amd64.deb`, `CodexManager-linux-portable.zip`

### Service
- Windows: `CodexManager-service-windows-x86_64.zip`
- macOS: `CodexManager-service-macos-arm64.zip`, `CodexManager-service-macos-x64.zip`
- Linux：`CodexManager-service-linux-x86_64.zip`
- Linux (web test package): `CodexManager-web-linux-x86_64.zip`

### Release type
- When `prerelease=auto`, `tag` containing `-` is published as pre-release.
- When `prerelease=auto` is included, any version that does not include `-` will be released as an official version.
- When `prerelease=true|false` is used, the automatic judgment based on tag will be overridden.
- When rerunning the same `tag`, Release metadata will be synchronized according to the current input.
- GitHub still automatically comes with `Source code (zip/tar.gz)`.

## `scripts/rebuild.ps1`
Defaults to local Windows packaging; `-AllPlatforms` mode calls the GitHub workflow.

### Common examples
```powershell
# Local Windows Build
pwsh -NoLogo -NoProfile -File scripts/rebuild.ps1 -Bundle nsis -CleanDist -Portable

# Trigger release workflow (and download artifacts)
pwsh -NoLogo -NoProfile -File scripts/rebuild.ps1 `
  -AllPlatforms `
  -GitRef main `
  -ReleaseTag v0.1.9 `
  -GithubToken <token>

# Force publishing as pre-release
pwsh -NoLogo -NoProfile -File scripts/rebuild.ps1 `
  -AllPlatforms -GitRef main -ReleaseTag v0.1.9-beta.1 -GithubToken <token> -Prerelease true
```

### Main parameters
- `-Bundle nsis|msi`: Default `nsis`
- `-NoBundle`: Only compilation, no installation package
- `-CleanDist`: Pre-build cleanup `apps/out`
- `-Portable`: Extra output portable version
- `-PortableDir §§7§§`: Portable version output directory, default `portable/`
- `-AllPlatforms`: Trigger the specified release workflow
- `-GithubToken §§8§§`: GitHub token; try when not passed `GITHUB_TOKEN`/`GH_TOKEN`
- `-WorkflowFile §§9§§`: Default `release-all.yml`
- `-GitRef <ref>`: workflow build ref; default current branch or current tag
- `-ReleaseTag §§11§§`: Publish tag; it is recommended to pass it in explicitly when `-AllPlatforms`
- `-Prerelease <auto|true|false>`: Default `auto`
- `-DownloadArtifacts <bool>`: Default `true`
- `-ArtifactsDir <path>`: Artifact download directory, default `artifacts/`
- `-PollIntervalSec <n>`: Polling interval, default `10`
- `-TimeoutMin <n>`: Timeout minutes, default `60`
- `-DryRun`: Print execution plan only

## `scripts/bump-version.ps1`
```powershell
pwsh -NoLogo -NoProfile -File scripts/bump-version.ps1 -Version 0.1.9
```

Will be updated simultaneously:
- workspace version of root `Cargo.toml`
- `apps/src-tauri/Cargo.toml`
- `apps/src-tauri/tauri.conf.json`

## Protocol regression probe
```powershell
pwsh -NoLogo -NoProfile -File scripts/tests/gateway_regression_suite.ps1 `
  -Base http://localhost:48760 -ApiKey <key> -Model gpt-5.3-codex
```

It will execute serially:
- `chat_tools_hit_probe.ps1`
- `chat_tools_hit_probe.ps1 -Stream`
- `codex_stream_probe.ps1`

## Related documents
- Release and Product Description: [Release and Product Description.md](release-and-artifacts.md)
- 脚本与发布职责对照：[../report/script-and-release-responsibility-matrix.md](../report/script-and-release-responsibility-matrix.md)