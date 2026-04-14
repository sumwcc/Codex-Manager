# CodexManager 与 codex HTTP 链路对比

## 结论先行

这次对比的核心结论很明确：

1. `codex` 的 HTTP 主链路里，没有看到比我们更强的显式 keepalive / 连接池复用配置。
2. 反过来，`CodexManager` 的上游 HTTP 客户端已经显式配置了连接池、`pool_idle_timeout` 和 `tcp_keepalive`。
3. `codex` 真正更强的是 WebSocket 连接复用，不是 HTTP keepalive。
4. 如果你现在遇到的“各种错误”主要发生在 HTTP 链路，更可疑的点不是“我们没开 keepalive”，而是“我们是网关桥接链路，链路更长，重写头、会话亲和、SSE 桥接、失败重建连接这些逻辑更多”。  

换句话说：

`codex HTTP 更稳` 这件事，从源码上看，**不能归因于它的 HTTP keepalive 比我们做得更强**。

## 一、我们项目的 HTTP 实现

### 1. 上游客户端是常驻缓存的，不是每次现建

你们项目在 [crates/service/src/gateway/core/runtime_config.rs](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\core\runtime_config.rs) 里维护了全局和按账号分组的客户端缓存：

- `UPSTREAM_CLIENT` 在 [runtime_config.rs:9](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\core\runtime_config.rs:9)
- `UPSTREAM_CLIENT_POOL` 在 [runtime_config.rs:10](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\core\runtime_config.rs:10)
- `upstream_client_for_account()` 在 [runtime_config.rs:177](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\core\runtime_config.rs:177)

主上游请求在 [candidate_flow.rs:54](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\candidate_flow.rs:54) 直接取 `upstream_client_for_account(account.id.as_str())`，说明默认路径就是复用缓存客户端。

### 2. 我们明确配置了连接池和 TCP keepalive

客户端构建位置在 [runtime_config.rs:248](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\core\runtime_config.rs:248)，关键参数如下：

- `.pool_max_idle_per_host(32)` 在 [runtime_config.rs:254](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\core\runtime_config.rs:254)
- `.pool_idle_timeout(Some(Duration::from_secs(90)))` 在 [runtime_config.rs:255](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\core\runtime_config.rs:255)
- `.tcp_keepalive(Some(Duration::from_secs(30)))` 在 [runtime_config.rs:256](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\core\runtime_config.rs:256)

这已经是非常直接的 HTTP 长连接/空闲连接池配置了。

### 3. 只有本地回环地址会被强制 `Connection: close`

在 [transport.rs:47](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:47) 的 `should_force_connection_close()` 里，代码只对 `localhost` / `127.0.0.1` / `::1` 这种目标做特殊处理。

真正改头的位置在：

- [transport.rs:65](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:65)
- [transport.rs:435](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:435)
- [transport.rs:438](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:438)

也就是说：

- 对真实上游目标默认不是 `close`
- 默认仍然走连接复用
- 强制关闭只是为了避免本地代理/本地 mock 的失效 keep-alive 连接

### 4. 我们还有失败后重建客户端再试一次

上游发送主逻辑在 [transport.rs:367](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:367)。

首次发送失败后，会在 [transport.rs:470](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:470) 调用 `fresh_upstream_client_for_account()`，然后在 [transport.rs:471](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:471) 再发一次。

这说明你们已经考虑过“缓存连接失效”的场景。

### 5. 我们还有显式 SSE 保活帧

在 [stream_readers/common.rs:8](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\common.rs:8) 定义了默认 SSE keepalive 间隔 `15_000ms`。

相关实现：

- `SseKeepAliveFrame` 在 [common.rs:26](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\common.rs:26)
- 注释型保活帧 `: keep-alive` 在 [common.rs:48](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\common.rs:48)
- `event: ping` 在 [common.rs:57](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\common.rs:57)
- 等待窗口计算在 [common.rs:181](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\common.rs:181)
- 空闲超时判断在 [common.rs:197](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\common.rs:197)

这说明你们不仅有 HTTP keepalive，还有流层的 SSE keepalive。

## 二、codex 的 HTTP 实现

### 1. codex 的 HTTP 主链路是 POST + SSE

`codex` 的主 HTTP 请求入口在 [codex-rs/core/src/client.rs:1143](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\core\src\client.rs:1143) `stream_responses_api()`。

这里会新建 transport：

- [client.rs:1171](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\core\src\client.rs:1171) `ReqwestTransport::new(build_reqwest_client())`

HTTP 请求本身在 [codex-rs/codex-api/src/endpoint/responses.rs:69](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\codex-api\src\endpoint\responses.rs:69) `stream_request()`，并设置：

- `x-client-request-id` 在 [responses.rs:90](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\codex-api\src\endpoint\responses.rs:90)
- `Accept: text/event-stream` 在 [responses.rs:137](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\codex-api\src\endpoint\responses.rs:137)

这说明它的 HTTP 也是 SSE 长流，但不是 WebSocket。

### 2. 没看到主 HTTP 路径里的显式 keepalive 参数

目前在 `codex` 的主 HTTP `/responses` 路径里，没有看到类似下面这种显式配置：

- `.pool_idle_timeout(...)`
- `.tcp_keepalive(...)`
- 明确的 session 级共享 `reqwest::Client`

静态代码里最明显的是：每次流请求都会出现 `ReqwestTransport::new(build_reqwest_client())`，例如：

- [client.rs:425](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\core\src\client.rs:425)
- [client.rs:491](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\core\src\client.rs:491)
- [client.rs:522](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\core\src\client.rs:522)
- [client.rs:1171](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\core\src\client.rs:1171)

从源码表象看，`codex` 的 HTTP 重点不在“显式 keepalive 调优”，而在“请求上下文和流处理”。

### 3. codex 的 HTTP 有应用层会话粘性

虽然没有看到更强的 keepalive 参数，但 `codex` 的 HTTP 不是纯裸请求。

它会携带：

- `prompt_cache_key` 在 [client.rs:864](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\core\src\client.rs:864)
- `stream: true` 在 [client.rs:874](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\core\src\client.rs:874)
- `prompt_cache_key` 回填在 [client.rs:881](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\core\src\client.rs:881)

SSE 响应处理里还会读取服务器返回的 turn state：

- `x-codex-turn-state` 在 [codex-rs/codex-api/src/sse/responses.rs:81](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\codex-api\src\sse\responses.rs:81)
- `eventsource()` 在 [responses.rs:363](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\codex-api\src\sse\responses.rs:363)
- 空闲超时等待在 [responses.rs:369](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\codex-api\src\sse\responses.rs:369)

所以它更像是：

- HTTP 传输层普通 SSE
- 应用层靠 `prompt_cache_key` / `x-client-request-id` / `x-codex-turn-state` 保持上下文

## 三、codex 真正明显更强的是 WebSocket，不是 HTTP

WebSocket 相关实现非常明显：

- `stream_request()` 在 [responses_websocket.rs:214](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\codex-api\src\endpoint\responses_websocket.rs:214)
- `connection_reused` 在 [responses_websocket.rs:217](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\codex-api\src\endpoint\responses_websocket.rs:217)
- 握手在 [responses_websocket.rs:364](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\codex-api\src\endpoint\responses_websocket.rs:364)
- `permessage_deflate` 在 [responses_websocket.rs:415](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\codex-api\src\endpoint\responses_websocket.rs:415)
- `Ping` / `Pong` 在 [responses_websocket.rs:92](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\codex-api\src\endpoint\responses_websocket.rs:92) 和 [responses_websocket.rs:93](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\codex-api\src\endpoint\responses_websocket.rs:93)
- `previous_response_id` 复用在 [client.rs:991](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\core\src\client.rs:991)
- `prewarm_websocket()` 在 [client.rs:1377](D:\MyComputer\own\GPTTeam\CodexManager\codex\codex-rs\core\src\client.rs:1377)

这部分才是 `codex` 最明显的“长连接优势”。

## 四、直接对比

### 1. 连接池 / keepalive

`CodexManager`

- 有全局和按账号缓存客户端
- 有 `pool_idle_timeout(90s)`
- 有 `tcp_keepalive(30s)`
- 默认复用连接

`codex`

- 在主 HTTP `/responses` 路径里没看到比我们更强的显式 keepalive 配置
- 更像是“每次请求创建一个 transport，并走 SSE 流”

### 2. SSE 流保活

`CodexManager`

- 有明确的 SSE keepalive 间隔
- 会发送注释帧和 `ping` 事件帧

`codex`

- 会解析 SSE、维护 turn state、做 idle timeout
- 但没有看到像我们这样明确的下游保活帧桥接逻辑

### 3. 失败后的恢复策略

`CodexManager`

- 首次失败后会 fresh client 重试

`codex`

- HTTP 主路径里主要看到的是普通 SSE 流处理
- 更强的恢复和复用能力在 WS 分支

## 五、因此最可能的问题不在 keepalive

如果你现在要解释“为什么 codex 不容易出错，而我们会出现各种 HTTP 错误”，从源码差异上看，更可能的原因是下面这些：

1. `codex` 的 HTTP 链路更短，是客户端直接对上游发起 SSE 请求。
2. 我们是网关桥接链路，请求会经过头重写、会话亲和、桥接读流、下游保活、失败重建客户端等多个环节。
3. `codex` 在可用时更偏向 WebSocket，这条链路天然更接近“一个连接内持续对话”；而我们分析的问题集中在 HTTP。
4. 我们虽然已经配置了 keepalive，但 blocking `reqwest` + 自定义 SSE bridge 的组合，本身就比“直接把上游 SSE 往前透”更容易出现边界问题。

## 六、建议下一步怎么查

如果你下一步要找“各种错误”的真实原因，我建议优先查这三类证据，而不是继续盯着 keepalive：

1. 在 [transport.rs:465](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:465) 和 [transport.rs:471](D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\attempt_flow\transport.rs:471) 周围补日志，区分：
   - 首次 `send()` 失败
   - fresh client 重试后仍失败
   - 失败发生在“建连/发首包”还是“流中途”
2. 在 SSE bridge 里补一类日志，记录：
   - 上游最后一帧时间
   - 下游最后一帧时间
   - 是否是 `stream_idle_timed_out()`
3. 统计错误是否只发生在 HTTP，而 WS 基本没有。如果是，就说明真正应该对比的是“我们 HTTP bridge”对“codex WS”，而不是“我们 HTTP keepalive”对“codex HTTP keepalive”。

## 最终判断

这次源码对比的最终判断是：

**`codex` 的 HTTP 主链路没有显示出比 `CodexManager` 更强的 keepalive 设计。**

如果 `codex` 更稳，更大的概率来自：

- 它更多依赖 WebSocket 长连接复用
- 它的 HTTP 链路更直接
- 我们的网关桥接层逻辑更复杂，故障面更大

所以后续排查方向应该从“keepalive 缺失”转向“桥接层、重试层、读流层、会话亲和层”。

## 附：三条链路的直观对照图

### 1. 我们的 HTTP

```text
调用方
  -> CodexManager 网关
     -> 复用按账号缓存的 reqwest::blocking::Client
        -> POST /responses 或其他上游接口
           -> 上游返回 SSE 流
              -> 网关读取上游流
                 -> 网关做桥接/保活/头处理/超时控制
                    -> 下游客户端
```

特点：

- 是 HTTP `POST`
- 底层 TCP 连接会复用
- 有显式连接池
- 有显式 `tcp_keepalive`
- 有 SSE 桥接和保活帧
- 链路更长，网关逻辑更多

### 2. codex 的 HTTP

```text
codex 客户端
  -> 创建本次请求的 transport
     -> POST /responses
        -> 服务端返回 SSE 流
           -> 客户端直接解析 SSE 事件
```

特点：

- 也是 HTTP `POST`
- 也不是短请求，而是 SSE 长流
- 看到应用层会话粘性
- 但主 HTTP 路径里没看到比我们更强的显式 keepalive 参数
- 链路更直接，没有你们这种网关桥接层

### 3. codex 的 WebSocket

```text
codex 客户端
  -> 建立一次 WebSocket 连接
     -> 同一条连接里连续发送多个 turn
        -> Ping/Pong 保活
           -> previous_response_id 续上下文
```

特点：

- 这才是真正的连接级长连接
- 一个连接里可以连续跑多个请求
- 有连接复用、保活、压缩、prewarm
- 这是 `codex` 最像“不会老是重新建连”的地方

### 4. 一句话总结

```text
我们 HTTP      = POST + SSE + 显式连接池/keepalive + 网关桥接
codex HTTP     = POST + SSE + 应用层会话粘性
codex WS       = 真正复用同一条长连接
```

如果你要找“为什么 codex 不容易出错”，最应该怀疑的是：

- `codex` 更多时候走 WS
- 你们的问题出在 HTTP bridge 复杂度

而不是：

- `codex HTTP` 开了某种我们没有的 keepalive 黑科技
