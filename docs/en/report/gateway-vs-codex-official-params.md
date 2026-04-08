# Inconsistencies between the current gateway and the official Codex

Only the differences in request headers that are most needed to continue processing are retained.

## `/v1/responses` Request header

| Fields | Official Codex | Current Gateway | Current Diff |
| --- | --- | --- | --- |
| `Authorization` | `Bearer <官方账号 token>` | `Bearer §§1§§` | The gateway will replace the account token |
| `User-Agent` | `codex_cli_rs/<编译时版本> (<os/version; <arch>) <terminal>` | `codex_cli_rs/<数据库配置版本> (<os/version; <arch>) §§7§§` | The official version number comes from `env!("CARGO_PKG_VERSION")`, we currently changed it to database configurable; the final value can be manually synchronized, but the source is inconsistent |
| `x-client-request-id` | Fixed equal to `conversation_id` | Priority equal to thread anchor point | When switching numbers and threads, it will become a new thread anchor point |
| `session_id` | Fixed equal to `conversation_id` | Priority equal to thread anchor | Normal `/responses` No longer sent when there is no thread anchor |
| `x-codex-turn-state` | Playback within the same turn | Playback when the same thread is stable | Will be actively discarded when switching numbers or thread replacement |

## Current conclusion

1. The most worthwhile differences now are these 5 request headers/transport layer behaviors.
2. `gatewayOriginator` The setting value will still remain in the local configuration, but it will no longer affect the actual outbound `originator`. The actual outbound is fixed to the official default value `codex_cli_rs`.
3. `User-Agent` For the version number, the official source is the compile-time package version; in order to facilitate manual matching with the official version, the current gateway has changed it to a database field that can be configured.

## Source code basis

- Official `codex`
  - `D:\MyComputer\own\GPTTeam相关\CodexManager\codex\codex-rs\core\src\client.rs`
  - `D:\MyComputer\own\GPTTeam相关\CodexManager\codex\codex-rs\codex-api\src\endpoint\responses.rs`
  - `D:\MyComputer\own\GPTTeam相关\CodexManager\codex\codex-rs\codex-api\src\requests\headers.rs`
  - `D:\MyComputer\own\GPTTeam相关\CodexManager\codex\codex-rs\core\src\default_client.rs`
- Current gateway
  - [transport.rs](../../../crates/service/src/gateway/upstream/attempt_flow/transport.rs)
  - [codex_headers.rs](../../../crates/service/src/gateway/upstream/headers/codex_headers.rs)
  - [runtime_config.rs](../../../crates/service/src/gateway/core/runtime_config.rs)