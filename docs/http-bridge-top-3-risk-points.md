# HTTP Bridge 最可能出错的 3 个点

## 结论先行

基于 `CodexManager` 与 `codex` 的源码对比，如果现在要在你们自己的 HTTP bridge 里找“最容易导致各种 HTTP 错误”的位置，我认为优先级最高的是这 3 个点：

1. `会话亲和头重写`，也就是 `session_id` / `x-client-request-id` / `x-codex-turn-state` / `prompt_cache_key` 的重新推导与透传。
2. `streaming 请求体 zstd 压缩`，也就是你们对 `/v1/responses` 流式请求额外引入的 `Content-Encoding: zstd`。
3. `阻塞式 SSE bridge + synthetic keepalive`，也就是上游流先被线程拆帧，再由 reader 注入 keepalive、判定 EOF/timeout/terminal 的这一整层。

这 3 个点都属于：**`codex` 直连 HTTP 没有这么重，而你们网关多做了一层或多做了多层的地方。**

## 1. 会话亲和头重写

### 为什么这是高风险点

`codex` 客户端的 HTTP 路径虽然也会传 `prompt_cache_key`、`x-client-request-id`、`x-codex-turn-state`，但它基本是“直接带着自己的上下文去请求上游”。

你们这里不是直接透传，而是先重新推导一遍，再拼成新的上游请求头。只要这里和上游真实线程状态略有错位，就可能出现：

- 403 / 409 / 422 一类会话状态错误
- 某些请求突然掉上下文
- 流能连上，但中途被上游提前结束
- 同一个会话偶发命中错误线程

### 关键代码

会话亲和推导：

- [session_affinity.rs:96](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\request\session_affinity.rs:96)
- [session_affinity.rs:104](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\request\session_affinity.rs:104)
- [session_affinity.rs:106](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\request\session_affinity.rs:106)
- [session_affinity.rs:107](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\request\session_affinity.rs:107)
- [session_affinity.rs:108](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\request\session_affinity.rs:108)
- [session_affinity.rs:115](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\request\session_affinity.rs:115)
- [session_affinity.rs:122](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\request\session_affinity.rs:122)
- [session_affinity.rs:130](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\request\session_affinity.rs:130)

上游头构造：

- `x-client-request-id` 在 [codex_headers.rs:95](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\headers\codex_headers.rs:95)
- `session_id` 在 [codex_headers.rs:134](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\headers\codex_headers.rs:134) 和 [codex_headers.rs:140](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\headers\codex_headers.rs:140)
- `x-codex-window-id` 在 [codex_headers.rs:142](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\headers\codex_headers.rs:142) 和 [codex_headers.rs:326](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\headers\codex_headers.rs:326)
- `x-codex-turn-state` 在 [codex_headers.rs:158](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\headers\codex_headers.rs:158)
- `resolve_optional_session_id()` 在 [codex_headers.rs:287](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\headers\codex_headers.rs:287)
- `resolve_window_id()` 在 [codex_headers.rs:310](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\headers\codex_headers.rs:310)

请求发送前的亲和计算入口：

- [transport.rs:382](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:382)
- [transport.rs:385](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:385)
- [transport.rs:399](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:399)

### 我为什么怀疑它

这里做了几件很“有攻击面”的事：

1. 会优先用 `conversation_id` 覆盖 `incoming_client_request_id` 和 `incoming_session_id`，见 [session_affinity.rs:107](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\request\session_affinity.rs:107) 和 [session_affinity.rs:108](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\request\session_affinity.rs:108)。
2. 当 `prompt_cache_key` 和 `conversation_id` 冲突时，会直接清掉 `turn_state`，见 [session_affinity.rs:120](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\request\session_affinity.rs:120) 到 [session_affinity.rs:122](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\request\session_affinity.rs:122)。
3. `window_id` 还会根据 `session_id` 派生为 `{session_id}:0`，见 [codex_headers.rs:323](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\headers\codex_headers.rs:323) 到 [codex_headers.rs:326](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\headers\codex_headers.rs:326)。

这类逻辑一旦和真实线程状态不一致，最容易出现“不是每次都报错，但一部分请求会莫名失败”的症状。

### 优先排查方式

建议优先补日志，记录每次请求的：

- incoming `conversation_id`
- body 里的 `prompt_cache_key`
- incoming `session_id`
- incoming `x-codex-turn-state`
- 最终发出去的 `session_id`
- 最终发出去的 `x-client-request-id`
- 最终发出去的 `x-codex-turn-state`
- 是否发生了 `thread_anchor_conflict`

## 2. streaming 请求体 zstd 压缩

### 为什么这是高风险点

这部分是你们相对 `codex` 主 HTTP 路径一个非常醒目的额外变量。

你们会对 ChatGPT backend 的流式 `/v1/responses` 请求启用 `zstd` 压缩，而 `codex` 主 HTTP 路径里没有看到这一层额外处理。

这类差异容易引入：

- 上游边缘节点、代理链路、反向代理对 `Content-Encoding: zstd` 兼容不一致
- 请求能在某些区域/某些账号正常，在另一些区域/节点失败
- 首包阶段就失败，看起来像“随机网络错误”或“HTTP 4xx/5xx”

### 关键代码

压缩决策：

- [transport.rs:171](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:171)
- [transport.rs:189](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:189)

压缩执行：

- [transport.rs:235](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:235)
- [transport.rs:250](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:250)
- `Content-Encoding: zstd` 在 [transport.rs:254](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:254)
- 压缩日志在 [transport.rs:256](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:256)
- 压缩失败回退在 [transport.rs:265](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:265)

### 我为什么怀疑它

这里的可疑点不是“代码一定错”，而是“它比 `codex` 多了一层网络兼容变量”。

而且触发条件正好是：

- `is_stream == true`
- `/v1/responses`
- ChatGPT backend

这和你现在排查的 HTTP 流式链路高度重合。

### 优先排查方式

最简单的验证方式不是猜，而是做一次对照实验：

1. 对同一类失败请求，强制走 `send_upstream_request_without_compression()`。
2. 对比失败率、状态码、首包耗时、流中断率。

如果关闭压缩后错误率明显下降，这里基本就坐实了。

## 3. 阻塞式 SSE bridge + synthetic keepalive

### 为什么这是高风险点

`codex` 主 HTTP 路径基本是“请求上游 -> 直接解析 SSE”。

你们这里是：

1. `reqwest::blocking::Response`
2. 单独起线程
3. `BufReader::read_line()`
4. 通过 `sync_channel(32)` 把帧送给 reader
5. reader 再决定是吐真实帧、吐 keepalive、还是直接结束流

这是整个 HTTP bridge 最像“复杂度放大器”的地方。

### 关键代码

上游拆帧泵：

- `sync_channel(32)` 在 [common.rs:87](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\common.rs:87)
- `read_line()` 在 [common.rs:93](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\common.rs:93)
- EOF 在 [common.rs:102](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\common.rs:102)
- read error 在 [common.rs:116](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\common.rs:116)
- `recv_timeout()` 在 [common.rs:136](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\common.rs:136)

keepalive 注入：

- `: keep-alive` 在 [common.rs:48](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\common.rs:48)
- `event: ping` 在 [common.rs:57](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\common.rs:57)
- keepalive 选择在 [delivery.rs:1674](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\delivery.rs:1674)

passthrough 读流终止条件：

- 读帧等待在 [passthrough.rs:114](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\passthrough.rs:114)
- EOF 后若未看到 terminal 则补“stream incomplete”在 [passthrough.rs:123](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\passthrough.rs:123) 到 [passthrough.rs:126](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\passthrough.rs:126)
- timeout 进入 idle timeout 判定在 [passthrough.rs:144](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\passthrough.rs:144)
- 否则直接吐 keepalive 在 [passthrough.rs:153](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\passthrough.rs:153)
- disconnected 直接结束在 [passthrough.rs:159](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\passthrough.rs:159)

### 我为什么怀疑它

这里的风险不是单点 bug，而是整个状态机更复杂：

1. `sync_channel(32)` 容量有限，消费慢时可能放大阻塞与时序问题。
2. `read_line()` 是按行拆帧，不是按 SSE 事件对象直接解析。
3. 上游没来数据时，bridge 会主动输出 keepalive，这会让“上游已卡住”和“只是暂时没帧”在外部看起来很像。
4. EOF、read error、timeout、disconnected 都会被折叠成几类 fallback message，真实根因容易被掩盖。

### 优先排查方式

建议优先补这几类日志：

- 每次 `recv_timeout` 是拿到真实帧、超时、还是 disconnected
- 从上游拿到最后一帧的时间
- 吐给下游的最后一帧时间
- keepalive 连续发送次数
- 流结束原因到底是 `Eof`、`Error`、`Timeout` 还是 `Disconnected`
- 最后一个真实 SSE `event` 类型

## 额外补充：发送重试路径有一个诊断盲点

这个点我不把它列进前三根因，但它很值得一提。

在 [transport.rs:465](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:465) 到 [transport.rs:473](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:473)：

- 第一次 `send()` 失败后会 fresh client 重试一次
- 如果第二次也失败，代码返回的是 `first_err`

也就是：

- 你日志里看到的未必是最终失败原因
- fresh client 失败的真实错误被丢掉了

这不一定制造错误，但会显著降低排障效率。

## 排查优先级建议

如果现在只做最小化排查，我建议顺序是：

1. 先查 `会话亲和头重写`
2. 再做 `关闭 zstd 压缩` 对照实验
3. 最后给 SSE bridge 补“结束原因 + keepalive 次数 + 最后一帧时间”日志

## 一句话结论

最像根因的不是“没有 keepalive”，而是：

- 你们在 HTTP 链路里做了更多会话重写
- 你们对 streaming 请求额外做了压缩
- 你们有一整层阻塞式 SSE bridge 和 synthetic keepalive

这三层，任何一层都比 `codex` 的直连 HTTP 更容易引入边界错误。
