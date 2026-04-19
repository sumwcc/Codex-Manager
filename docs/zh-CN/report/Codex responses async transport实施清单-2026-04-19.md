# Codex responses async transport实施清单 - 2026-04-19

## 1. 目标

把当前 `/v1/responses` 主链路从“`blocking::Response` + bridge pump”进一步往官方 Codex 的 async transport 语义靠拢，但只改必要路径，不做整仓无边界重构。

目标不是一次性把全仓所有 `reqwest::blocking` 都换掉，而是先把最关键的这条链路拆出来：

- 上游请求进入网关
- 命中 `/v1/responses` SSE
- 上游返回 stream
- bridge 做观测统计
- 下游收到 SSE

## 2. 当前切口

当前 `/v1/responses` 已经做到：

- 请求 shape 高度对齐
- SSE 解析组件对齐到 `eventsource-stream`
- responses 事件级统计比 generic collector 更接近官方

当前仍未对齐的是：

- 上游 transport 还是 `reqwest::blocking::Client`
- SSE 输入不是 `resp.bytes_stream()`
- reader 仍依赖 `thread + sync_channel + recv_timeout`
- event 被重新编码回 SSE 后再输出

## 3. 不做的事

这轮如果真的开改，建议明确不做下面这些扩散项：

- 不顺手把 `aggregate_api.rs`、plugin runtime、usage HTTP 等其它 blocking client 一起迁掉
- 不顺手统一所有 provider 的 stream reader
- 不先做全局 async 化
- 不把 tiny_http 整体切掉

先只聚焦：

- `/v1/responses`
- `Codex backend / ChatGPT backend` 的 SSE passthrough

## 4. Phase 拆分

### Phase 0：先把边界抽象出来

状态：已完成

#### 目标

把“上游响应对象”从裸 `reqwest::blocking::Response` 中解耦出来，为后续双轨并存做准备。

#### 主要文件

- [transport.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/upstream/attempt_flow/transport.rs)
- [primary_flow.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/upstream/attempt_flow/primary_flow.rs)
- [candidate_flow.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/upstream/attempt_flow/candidate_flow.rs)
- [openai_base.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/upstream/attempt_flow/openai_base.rs)
- [fallback_branch.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/upstream/attempt_flow/fallback_branch.rs)
- [postprocess.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/upstream/attempt_flow/postprocess.rs)

#### 建议改法

- 定义一个新的上游响应抽象，例如：
  - `GatewayUpstreamResponse::Blocking(reqwest::blocking::Response)`
  - 后续再预留 `GatewayUpstreamResponse::Stream(...)`
- 先不动行为，只把函数签名从直接传 `blocking::Response` 改为传抽象类型
- 让当前所有路径仍落到 `Blocking` 分支

已完成情况：

- 已新增 `GatewayUpstreamResponse`
- 已从 `attempt_flow/transport.rs` 一路接到 `response_finalize.rs`
- 当前仍在 `finalize_upstream_response()` 里单点解包回 `blocking::Response`
- `http_bridge` 和各类 `stream_readers` 尚未进入 Phase 1/2 改造

#### 验收标准

- 行为完全不变
- 所有已有 gateway tests 继续通过

#### 风险

- 这是签名级横切改动，波及文件多，但逻辑风险低

---

### Phase 1：给 `/v1/responses` 单独引入 async-like byte stream 响应抽象

状态：已完成

#### 目标

不是先把整个 gateway upstream 改成 async，而是只给 `/v1/responses` 的 stream 响应引入“接近官方 `bytes_stream()`”的数据入口。

#### 主要文件

- [transport.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/upstream/attempt_flow/transport.rs)
- [delivery.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/observability/http_bridge/delivery.rs)
- [mod.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/observability/http_bridge/mod.rs)

#### 建议改法

- 在 transport 层新增一个仅供 `/v1/responses` 使用的 stream body 抽象，例如：
  - `GatewayByteStream`
  - `GatewayStreamResponse { status, headers, body_stream }`
- 当前即使底层还是 blocking，也把“响应字节流”抽象提前到 transport 层，而不是到了 reader 才把 `Response` 包成 stream
- 先让 `/v1/responses` 分支能选择：
  - 旧路径：`blocking::Response`
  - 新路径：`GatewayStreamResponse`

已完成情况：

- `GatewayByteStream` / `GatewayStreamResponse` 已落在 `gateway/upstream/response.rs`
- `send_upstream_request*()` 在原生 `/v1/responses` 流式路径上已经返回 stream variant
- 非 `/v1/responses` 路径仍保持原来的 blocking response

#### 验收标准

- `/v1/responses` 仍能正常透传 SSE
- 非 `/v1/responses` 路径完全不受影响

#### 风险

- 这里开始会影响 delivery 分发逻辑

---

### Phase 2：把 `OpenAIResponsesPassthroughSseReader` 从“blocking pump”改成“stream pump”

状态：已完成

#### 目标

把当前 `blocking::Response.read() -> unfold(Bytes)` 的桥接往真正的上游 stream body 再挪一步。

#### 主要文件

- [openai_responses.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/observability/http_bridge/stream_readers/openai_responses.rs)
- [common.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/observability/http_bridge/stream_readers/common.rs)
- [stream_readers.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/observability/http_bridge/stream_readers.rs)

#### 建议改法

- 让 `OpenAIResponsesPassthroughSseReader::new()` 接受“字节流抽象”而不是 `reqwest::blocking::Response`
- 保留 `eventsource-stream` 不动
- 先把 `event_to_sse_frame()`、typed inspector、collector 逻辑原样保留
- 只替换“字节从哪里来”

已完成情况：

- `OpenAIResponsesPassthroughSseReader` 已新增 `from_stream_response()`
- `/v1/responses` 的 stream variant 现在走新的 `respond_with_stream_upstream()` 入口
- 旧的 blocking `delivery` 主函数与其它 provider reader 都保持不变

#### 验收标准

- 这些测试继续通过：
  - `openai_responses_passthrough_reader_emits_keepalive_for_responses_stream`
  - `openai_responses_passthrough_reader_parses_split_events_with_eventsource_stream`
  - `openai_responses_passthrough_reader_collects_output_item_field_text`
  - `openai_responses_passthrough_reader_marks_incomplete_terminal_error_from_status_details`

#### 风险

- 如果这里动过大，最容易引入的是 keepalive 时序变化和 EOF/错误分类变化

---

### Phase 3：把 delivery 分支从“Response 对象驱动”改成“响应能力驱动”

状态：已完成

#### 目标

让 delivery 不再预设“上游响应一定是 blocking response”，而是按能力分发：

- 原始 body 直读
- SSE byte stream
- JSON body

#### 主要文件

- [delivery.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/observability/http_bridge/delivery.rs)
- [response_finalize.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/upstream/proxy_pipeline/response_finalize.rs)

#### 建议改法

- 把 `/v1/responses` SSE 分支先改成消费 stream body 抽象
- 其它 adapter 仍保持 blocking response
- 只在 `ResponseAdapter::Passthrough` 且 `request_path.starts_with(\"/v1/responses\")` 时启用新逻辑

已完成情况：

- `http_bridge/mod.rs` 已按 `GatewayUpstreamResponse` 分流
- 新增了 `respond_with_stream_upstream()` 作为 `/v1/responses` 的 stream 分支
- 旧的 blocking `respond_with_upstream()` 逻辑仍原样保留，未波及其它 adapter

#### 验收标准

- `/v1/responses` 路径日志、usage、last_sse_event_type 不回退
- chat/completions compat 路径不受影响

#### 风险

- 这里最怕把 adapter 条件改乱，导致非 responses 路径误走新 reader

---

### Phase 4：评估是否继续把 upstream 请求执行本身 async 化

状态：已完成评估，当前不继续推进

#### 目标

这一步不是默认继续做，而是 Phase 0-3 完成后再决定。

#### 主要文件

- [transport.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/upstream/attempt_flow/transport.rs)
- [runtime_config.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/core/runtime_config.rs)
- 以及所有当前以 `reqwest::blocking::Client` 贯穿的 candidate / fallback / postprocess 调用链

#### 判断标准

只有满足下面至少两条，才值得继续：

- 仍观察到 blocking pump 特有的中断或时序问题
- `/v1/responses` 需要进一步对齐官方 `ReqwestTransport` 行为
- 现有 thread/channel bridge 已经成为性能瓶颈

否则建议停在 Phase 3。

当前结论：

- 现阶段已经完成“上游响应抽象层 + `/v1/responses` stream body 抽象 + reader 迁移 + delivery 分流”
- 剩余再往前走就是更大范围的 async request 执行重构，不再属于高性价比收尾项
- 因此当前阶段在 Phase 3 停住最合适

## 5. 建议先动的精确文件顺序

如果你真要开始改，我建议按下面顺序开工：

1. [transport.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/upstream/attempt_flow/transport.rs)
2. [primary_flow.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/upstream/attempt_flow/primary_flow.rs)
3. [candidate_flow.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/upstream/attempt_flow/candidate_flow.rs)
4. [openai_base.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/upstream/attempt_flow/openai_base.rs)
5. [fallback_branch.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/upstream/attempt_flow/fallback_branch.rs)
6. [postprocess.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/upstream/attempt_flow/postprocess.rs)
7. [openai_responses.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/observability/http_bridge/stream_readers/openai_responses.rs)
8. [common.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/observability/http_bridge/stream_readers/common.rs)
9. [delivery.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/observability/http_bridge/delivery.rs)
10. [http_bridge_tests.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/src/gateway/observability/tests/http_bridge_tests.rs)
11. [openai.rs](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/crates/service/tests/gateway_logs/openai.rs)

## 6. 每阶段最少验证集

### 请求形态不回退

- `responses_default_path_preserves_native_codex_body_shape`

### `/v1/responses` reader 不回退

- `openai_responses_passthrough_reader_emits_keepalive_for_responses_stream`
- `openai_responses_passthrough_reader_parses_split_events_with_eventsource_stream`
- `openai_responses_passthrough_reader_collects_output_item_field_text`
- `openai_responses_passthrough_reader_marks_incomplete_terminal_error_from_status_details`

### gateway logs 不回退

- `gateway_openai_responses_keeps_conversation_anchor_over_conflicting_prompt_cache_key`
- `gateway_openai_responses_stay_on_chatgpt_codex_base`
- `gateway_openai_stream_logs_cached_and_reasoning_tokens`

### 前端构建不回退

- 在 [apps/package.json](D:/MyComputer/own/GPTTeam/CodexManager/CodexManager/apps/package.json) 所在目录执行 `pnpm run build:desktop`

## 7. 风险排序

| 风险级别 | 风险 | 说明 |
| --- | --- | --- |
| 高 | 横切签名重构 | 当前 upstream 调用链大量直接传 `reqwest::blocking::Response` |
| 高 | EOF / timeout / disconnect 语义变化 | 这是最容易引发“报错文案变了但根因没变”的地方 |
| 中 | keepalive 时序变化 | 会影响前端感知和测试稳定性 |
| 中 | 事件回放字节差异 | 即使语义正确，也可能造成 snapshot 类测试变化 |
| 低 | typed inspector 逻辑回退 | 当前这块已经比较稳，尽量不要在 transport 重构时一起改 |

## 8. 回滚建议

如果某一阶段引入了不稳定性，不要硬扛到“全 async 化”。

推荐回滚粒度：

- 先回滚 Phase 2 的 reader 输入源变更
- 保留 Phase 0 的抽象层
- 保留当前已完成的 typed inspector 和请求 shape 对齐

也就是说，最稳妥的止损点是：

- 允许抽象层存在
- 但 reader 仍然退回当前 `blocking::Response.read() -> unfold(Bytes) -> eventsource-stream`

## 9. 一句话建议

如果只是为了“少报错、少误判、排障更清楚”，当前停在 Phase 3 已经是最合适的收口点。  
如果你的目标是“逐行为贴近官方 Codex client”，当前也不需要回头重做 Phase 0-3，而是只在确认收益足够后再继续评估 Phase 4 的更大范围 async request 执行重构。
