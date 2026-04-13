# Gateway 优化 TODO Round 8

更新时间：2026-04-13

本轮目标：拆出前端 `transport` 中的错误解析与 RPC envelope 解包逻辑，降低 `transport.ts` 的职责密度。

- [x] 新建 `transport-errors` 模块
- [x] 让 `transport.ts` 复用共享错误解析与 envelope helper
- [x] 为新模块补最小 Node 单测
- [x] 运行关键前端验证并记录结果

本轮验证：

- `pnpm test:runtime`
- `pnpm build:desktop`
