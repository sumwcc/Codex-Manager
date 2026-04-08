# Operation and Deployment Guide

## Scope of application
- First time use on desktop
- Service Version runs independently
- Docker Deployment
- macOS First release

## Quick start
1. Start the desktop and click "Start Service".
2. Enter "Account Management", add an account and complete authorization.
3. If the callback fails, paste the callback link to complete the parsing manually.
4. Refresh usage and confirm account status.

## Account import and export
- `批量导入`: Select multiple `.json/.txt` files and import them together.
- `按文件夹导入`: Only available on the desktop; after selecting a directory, recursively scan the `.json` files and import them in batches. Empty files will be automatically skipped.
- `导出用户`: After selecting the directory, click "One JSON file per account" to export to facilitate backup and migration.

## Service Version
1. Download `CodexManager-service-§§0§§-§§1§§.zip` in Release and unzip it.
2. It is recommended to start `codexmanager-start`. A process starts service + web and can be closed directly in the console `Ctrl+C`.
3. You can also just launch `codexmanager-web`, which will automatically pull up `codexmanager-service` in the same directory and open the browser.
4. Or start `codexmanager-service` first, and then `codexmanager-web`.
5. Default address: service `localhost:48760`, Web UI `http://localhost:48761/`.
6. Close: Visit `http://localhost:48761/__quit`; if the web service has been automatically launched, it will try to close it together.
7. If you need to reverse proxy yourself or split-deploy front-end static resources, you must forward `/api/runtime` and `/api/rpc` at the same time; only hosting static files will cause the management page to not work properly.

## Docker Deployment

### GitHub Packages / GHCR
- After Release is released, the `codexmanager-service` and `codexmanager-web` images will be pushed to GitHub Packages (GHCR) at the same time.
- Simply pull the corresponding release tag, for example: `docker pull ghcr.io/qxcnm/codexmanager-service:v0.1.15`
- [`docker/docker-compose.release.yml`](../../../docker/docker-compose.release.yml) in the warehouse also directly references GHCR, set `CODEXMANAGER_RELEASE_TAG` before use.
- Example: `CODEXMANAGER_RELEASE_TAG=v0.1.15 docker compose -f docker/docker-compose.release.yml up -d`

### Method 1: docker compose
```bash
docker compose -f docker/docker-compose.yml up --build
```

Browser opens: `http://localhost:48761/`

### Method 2: Build and run separately
```bash
#service
docker build -f docker/Dockerfile.service -t codexmanager-service .
docker run --rm -p 48760:48760 -v codexmanager-data:/data \
  -e CODEXMANAGER_RPC_TOKEN=replace_with_your_token \
  codexmanager-service

# web (requires access to service)
docker build -f docker/Dockerfile.web -t codexmanager-web .
docker run --rm -p 48761:48761 \
  -v codexmanager-data:/data \
  -e CODEXMANAGER_WEB_NO_SPAWN_SERVICE=1 \
  -e CODEXMANAGER_SERVICE_ADDR=host.docker.internal:48760 \
  -e CODEXMANAGER_RPC_TOKEN=replace_with_your_token \
  codexmanager-web
```

- Note: If you want the Web password, setting items, cache model list and other status to be consistent with the service, `codexmanager-web` and `codexmanager-service` must share the same `/data` volume.

## macOS First startup
- The current macOS Release product has not been notarized using an Apple Developer account, so after downloading from the browser for the first time, Gatekeeper may prompt "Corrupted" or refuse to open.
- macOS `dmg` in Release have built-in `Open CodexManager.command` and `README-macOS-first-launch.txt`.
- It is recommended to drag `CodexManager.app` to "Applications" first, and then double-click the script to complete the first release.
- You can also execute it directly:

```bash
xattr -dr com.apple.quarantine /Applications/CodexManager.app
```

- If it is still blocked, perform "right-click -> open" on `CodexManager.app` again.

## Related documents
- Environment variables and running configuration: [Environment variables and running configuration instructions.md](environment-and-runtime-config.md)
- Minimum Troubleshooting Manual: [Minimum Troubleshooting Manual.md](minimal-troubleshooting-guide.md)