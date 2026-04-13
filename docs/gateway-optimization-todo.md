# Gateway 优化 TODO

更新时间：2026-04-13

本轮目标：先做两项低风险、可验证、能直接改善维护性的优化，不做大范围重写。

- [x] 梳理 gateway failover 与 candidate skip 指标语义，避免把“跳过候选”混记成“真实 failover”
- [x] 抽取本地短路统一响应 helper，收敛 `local_models` / `local_count_tokens` 的重复 trace、log、response 逻辑
- [x] 运行关键测试并记录结果

已执行验证：

- `cargo test -p codexmanager-service metrics_tokens -- --nocapture`
- `cargo test -p codexmanager-service local_models -- --nocapture`
- `cargo test -p codexmanager-service local_count_tokens -- --nocapture`
