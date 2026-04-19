# Codex responses async transport差异对照 - 2026-04-19

## 1. 结论先行

当前 `/v1/responses` 这条主链路可以分成两层来看：

- `SSE 解析组件` 这一层，已经和官方对齐到同一个库：`eventsource-stream`
- `上游 transport / 字节流来源 / 执行语义` 这一层，还没有完全一样

更准确地说，当前状态不是“已经完全和官方一样”，而是：

- 官方：`async reqwest bytes_stream() -> eventsource-stream`
- 当前：`GatewayStreamResponse -> GatewayByteStream.recv() -> unfold(Bytes) -> eventsource-stream`

所以它属于“解析器一致，但 transport 语义仍有差异”。

## 2. 对照表

| 维度 | 官方 Codex | 当前网关 | 是否一致 | 影响 |
| --- | --- | --- | --- | --- |
| 上游 HTTP client | `reqwest::Client` | `reqwest::blocking::Client` | 否 | 请求执行模型不同，超时、错误传播、背压行为不可能完全等价 |
| 上游 stream 返回类型 | `StreamResponse { bytes: ByteStream }` | `GatewayStreamResponse { body: GatewayByteStream }` | 否 | 官方天然就是 async byte stream；我们现在先把 `/v1/responses` 的 blocking body 提前抽成 stream 响应对象 |
| 字节流来源 | `resp.bytes_stream()` | transport 层把 `blocking::Response` 包成 `GatewayByteStream` | 否 | chunk 边界、读取时机、EOF/错误暴露时机仍不可能与官方完全一致 |
| SSE 解析组件 | `eventsource-stream` | `eventsource-stream` | 是 | 事件分帧规则已基本一致 |
| SSE 输入来源 | 原始 async byte stream | `GatewayByteStream.recv() -> unfold(Bytes)` 生成的伪 async stream | 否 | 解析器虽然相同，但上游喂给解析器的节奏不同 |
| 事件消费方式 | 后续直接按 typed 事件流处理 | 先解析成 `Event`，再重编码成 SSE 行，再做 bridge 统计 | 否 | 语义已接近，但不是官方那种“typed event 直接驱动后续逻辑” |
| 字节级透传 | 不需要回放 SSE 给下游 | 会把解析后的 `Event` 重新编码成 SSE frame 输出 | 否 | 下游大多没问题，但不可能与原始上游字节完全一致 |
| `event:` / `data:` 重建 | 不需要 | `event_to_sse_frame()` 手动重建 | 否 | 空行、默认 `message` 事件、省略字段的表现取决于本地编码逻辑 |
| keepalive 策略 | 官方内部事件消费，不需要桥接层注入 | 本地 bridge 会按策略插入 keepalive frame | 否 | 用户看到的流不可能和官方逐字节一致，但这是为了稳定性做的本地增强 |
| 终态判断 | 由 typed event / transport 生命周期驱动 | `eventsource-stream` + 本地 inspector + bridge collector 共同判断 | 否 | 当前已比之前更接近官方，但仍叠加了本地错误分类 |
| 错误模型 | `TransportError` / `ApiError` | 上游错误 + bridge 错误 + UI 友好化错误 | 否 | 用户最终文案和日志字段不可能完全同官方 |
| 背压模型 | async stream 原生背压 | blocking reader + thread + channel | 否 | 高压场景下的排队、唤醒、占用模型不同 |
| Pending / poll 语义 | runtime 驱动 | `noop_waker` + `poll_next()` + `thread::yield_now()` | 否 | 虽然能工作，但不是官方 transport 的原生执行方式 |
| channel 桥接 | 无需额外 mpsc | `sync_channel` 承接 frame pump | 否 | 多了一层队列和容量约束 |
| 观测统计注入点 | typed event 消费阶段 | 读到 frame 后立刻更新 `PassthroughSseCollector` | 否 | 我们更偏 bridge 侧统计，不是官方 client 侧语义 |

## 3. 源码锚点

### 官方 Codex

- `D:\MyComputer\own\GPTTeam\codex\codex\codex-rs\codex-client\src\transport.rs`
  - `stream()` 里直接 `resp.bytes_stream()`
- `D:\MyComputer\own\GPTTeam\codex\codex\codex-rs\codex-api\src\sse\responses.rs`
  - `stream.eventsource()`

### 当前网关

- `D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\stream_readers\openai_responses.rs`
  - `OpenAIResponsesSsePump::new()`
  - `unfold(Some(byte_stream), ...)`
  - `byte_stream.eventsource()`
  - `event_to_sse_frame()`
- `D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\upstream\response.rs`
  - `GatewayStreamResponse::from_blocking_response()`
  - `GatewayByteStream::from_blocking_response()`
- `D:\MyComputer\own\GPTTeam\CodexManager\CodexManager\crates\service\src\gateway\observability\http_bridge\aggregate\sse_frame.rs`
  - `inspect_openai_responses_sse_frame()`

## 4. 当前最重要的判断

这块现在最容易说错的地方，是把“用了同一个 `eventsource-stream`”误写成“整个流式 transport 已经完全一致”。

更准确的判断应该是：

- `组件层`：已对齐
- `transport 语义层`：未完全对齐
- `业务观测层`：已经比 generic collector 更接近官方，但仍是我们自己的 bridge 语义

## 5. 还剩哪些真实差异最值得关注

### P1：typed event 与 bridge collector 仍不是同一层语义

虽然现在 `/v1/responses` 已经有专用 inspector，但它仍然是：

- 先把 SSE frame 解析出来
- 再在 bridge 里提取 `usage` / `terminal` / `error`

而不是官方那种：

- 先得到 typed `ResponseEvent`
- 后续逻辑完全基于 typed event 做状态推进

这意味着我们已经“更像官方”，但还不是“同一套事件驱动模型”。

### P2：`GatewayByteStream` pump 仍会带来执行模型差异

当前我们还有这些官方没有的本地结构：

- transport 层把 `blocking::Response` 包成 `GatewayByteStream`
- 独立线程
- `sync_channel`
- `recv_timeout`
- keepalive 注入
- 重新编码 SSE

这些都不一定是坏事，但它们说明现在的目标更接近：

- “兼容官方协议并提高稳定性”

而不是：

- “完全复刻官方 client transport”

## 6. 是否还值得继续改

我的判断是：

- 如果目标是“减少报错率、减少误判、便于排障”，当前这块已经够用了，收益最高的工作已经完成
- 如果目标是“尽可能逐行为和官方 transport 完全一致”，那下一步就不是继续修 parser，而是要重构到 async transport 主干

## 7. 建议顺序

| 顺序 | 建议 | 原因 |
| --- | --- | --- |
| 1 | 保持当前 `eventsource-stream + 专用 inspector` 方案 | 这是当前性价比最高的稳定点 |
| 2 | 继续统一 `response.incomplete` / `idle timeout` / body error 的错误归因文案 | 对用户和排障收益更直接 |
| 3 | 只有在你明确要追求“官方逐行为一致”时，再评估把 upstream streaming 改成 async transport | 这一步改动大，而且是横切重构 |

## 8. 一句话结论

现在不是“完全一样”，而是“最关键的解析器已经一样，transport 还不是同一套执行模型”。这一点写文档和排查问题时都应该明确区分。
