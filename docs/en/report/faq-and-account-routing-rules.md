# FAQ and account hit rules

## FAQ
- Authorization callback failure: Prioritize checking whether `CODEXMANAGER_LOGIN_ADDR` is occupied, or use manual callback resolution in the UI.
- The model list or request is intercepted by the challenge: Prioritize checking the proxy exit, request header differences and account status, and no longer troubleshoot through old fallback or upstream cookies.
- Still blocked by Cloudflare/WAF: Do not troubleshoot along the old compatibility path, and only proceed according to the semantics of `Codex-First` in the future.
- "Some data refresh failed, available data has been displayed" appears frequently: the automatic refresh scenario has been changed to only record logs; manual refresh will prompt failed items and sample errors. Prioritize checking the "Background Task" interval/switch on the settings page, and the failed task name in the service log.
- Independently run service/Web: If the directory is not writable (such as the installation directory), please set `CODEXMANAGER_DB_PATH` to a writable path.
- macOS Requests in a proxy environment `502/503`: Prioritize to confirm that the system proxy does not take over the local loopback request (`localhost/127.0.0.1` go to `DIRECT`), and ensure that the address uses lowercase `localhost:§§0§§`.

## Migration instructions

### Codex-First direction

The current warehouse has determined the follow-up direction as `Codex-First`:

- No longer use the old compatible behavior as the mainline path
- Only one active account can be bound to the same session
- Manually switching accounts will cut threads, and automatically switching accounts will also cut threads.

Corresponding design documents:


### The difference between current behavior and target behavior

Current behavior:

- `balanced` Still strict polling as per `Key + 模型`
- The binding semantics of sessions and accounts are still migrating from "polling by request" to "binding by session"

Target behavior:

- Bound sessions will first hit the bound account and will no longer participate in each round of ordinary polling.
- Automatically switching accounts will synchronously switch the upstream thread generation
- The old compatibility switch has completely exited the main path and is no longer used as a recommended configuration

## Account hit rules
- In `ordered` (order priority) mode, the gateway builds candidates in ascending order of account `sort` and tries them in sequence, for example `0 -> 1 -> 2 -> 3`.
- This means "try in order", not "always hit number 0"; if the previous account is unavailable or fails, it will automatically switch to the next one.

### Common reasons why pre-order accounts are not hit
- Account status is not `active`
- The account is missing token
- Usage determination is not available, for example, the main window is exhausted and the usage field is missing
- The account is in cooldown or the concurrent soft cap triggers skipping

### `balanced` Mode
- `balanced` (Balanced Round Robin) mode will by default rotate strictly among all available accounts by `Key + 模型` dimension, with no guarantee of starting from a minimum `sort`.
- Health swaps will only be added to the balanced poll head if you explicitly increase `CODEXMANAGER_ROUTE_HEALTH_P2C_BALANCED_WINDOW`.

## Troubleshooting log
You can view the same directory of the database `gateway-trace.log`:
- `CANDIDATE_POOL`: Candidate order for this request
- `CANDIDATE_START` / `CANDIDATE_SKIP`: Actual attempt vs. skip reason
- `REQUEST_FINAL`: Final hit account

## Related documents
- Environment variables and running configuration: [Environment variables and running configuration instructions.md](environment-and-runtime-config.md)
- Minimum Troubleshooting Manual: [Minimum Troubleshooting Manual.md](minimal-troubleshooting-guide.md)