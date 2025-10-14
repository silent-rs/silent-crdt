# silent-crdt

## 项目概述

**silent-crdt** 是 Silent Odyssey 第五阶段项目，旨在基于 Silent 框架实现 CRDT（Conflict-free Replicated Data Types）协议的验证与实验。项目聚焦分布式协同一致性、离线编辑与状态自动合并能力，推动 Silent 体系在多节点、弱连接环境下的数据同步与冲突解决探索。

## 协议范围与目标

- 支持核心 CRDT 数据结构（如 GCounter、PNCounter、LWW-Register、OR-Set、Map 等）
- 验证多节点同步与最终一致性
- 支持离线编辑与自动合并场景
- 提供 JSON 格式的同步接口，便于系统集成

## 架构设计

建议目录结构如下：
```
src/
├── main.rs          # 启动入口
├── crdt.rs          # 各类 CRDT 实现
├── sync.rs          # 状态同步与合并逻辑
├── storage.rs       # 本地持久化与版本记录
├── api.rs           # 提供 gRPC / HTTP 接口
└── tests.rs         # 一致性验证与单元测试
```

## 使用示例

### HTTP API 模式

启动服务：
```bash
cargo run
```

访问 Web 监控面板：
```bash
open http://127.0.0.1:8080
```

同步请求示例：
```bash
curl -X POST http://127.0.0.1:8080/sync -d '{"changes":[{"op":"add","key":"note","value":"hello"}]}'
```

### gRPC 模式

启动 gRPC 服务（同时启动 HTTP 和 gRPC）：
```bash
cargo run -- --grpc-enabled --grpc-port 50051
```

运行 gRPC 客户端示例：
```bash
cargo run --example grpc_client
```

gRPC 服务提供以下 RPC 方法：
- `Sync` - 同步数据变更
- `Merge` - 合并状态
- `GetState` - 获取当前状态
- `GetStateHash` - 获取状态哈希
- `GetOpLog` - 获取操作日志
- `GetHistory` - 获取操作历史
- `GetConflicts` - 获取冲突信息
- `HealthCheck` - 健康检查

## Web 监控面板

项目提供了两个可视化 Web 面板，用于监控和分析 CRDT 系统：

### 1. 实时状态监控面板

访问地址：`http://127.0.0.1:8080/`

功能特性：
- **实时状态监控** - 显示节点状态、CRDT 条目数、操作日志数、状态哈希
- **数据可视化** - 展示所有 CRDT 数据结构（GCounter、PNCounter、LWWRegister、ORSet）
- **向量时钟** - 查看各节点的向量时钟状态
- **操作日志** - 实时显示系统操作日志
- **自动刷新** - 支持自动刷新（5秒间隔）

### 2. 操作历史与冲突分析面板

访问地址：`http://127.0.0.1:8080/static/history.html`

功能特性：
- **操作历史时间线** - 按时间顺序展示所有 CRDT 操作
- **并发冲突检测** - 自动检测 LWWRegister 的并发写入冲突
- **因果上下文展示** - 显示每个操作的向量时钟（因果依赖关系）
- **冲突解决策略** - 可视化展示 CRDT 如何自动解决冲突
- **操作过滤** - 按 CRDT 类型过滤操作历史
- **统计分析** - 显示总操作数、冲突数、参与节点数、时间跨度

这两个面板为理解和调试分布式 CRDT 系统提供了强大的可视化工具。

## 权限控制与安全

### 启用权限控制

默认情况下，权限控制是**禁用**的。要启用权限控制，请在启动时添加 `--auth-enabled` 参数：

```bash
cargo run -- --auth-enabled --jwt-secret "your-secret-key"
```

### 角色说明

系统支持三种角色：

- **admin** - 管理员，拥有所有权限
- **writer** - 写入者，可以修改 CRDT 数据和查看状态
- **reader** - 读取者，只能查看状态和历史

### 生成 JWT Token

```bash
curl -X POST http://127.0.0.1:8080/auth/token \
  -H "Content-Type: application/json" \
  -d '{
    "node_id": "node1",
    "role": "writer",
    "expires_in_secs": 3600
  }'
```

响应示例：
```json
{
  "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "expires_in": 3600
}
```

### 使用 Token 访问 API

在请求头中添加 `Authorization: Bearer <token>`：

```bash
# 查看状态（需要 reader 权限）
curl -X GET http://127.0.0.1:8080/state \
  -H "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."

# 同步数据（需要 writer 权限）
curl -X POST http://127.0.0.1:8080/sync \
  -H "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..." \
  -H "Content-Type: application/json" \
  -d '{"changes":[{"op":"add","key":"note","value":"hello"}]}'
```

### 获取节点公钥

每个节点都有一个 Ed25519 密钥对，用于操作签名：

```bash
curl -X GET http://127.0.0.1:8080/auth/public-key
```

响应示例：
```json
{
  "node_id": "01HZXABC123...",
  "public_key": "base64_encoded_public_key..."
}
```

### API 权限要求

| API 端点 | 需要权限 | 说明 |
|---------|---------|------|
| `POST /auth/token` | 无 | 生成 JWT token |
| `GET /auth/public-key` | 无 | 获取节点公钥 |
| `POST /sync` | writer | 同步数据变更 |
| `POST /sync-peer` | writer | 触发节点间同步 |
| `POST /merge` | writer | 合并状态 |
| `GET /state` | reader | 查看当前状态 |
| `GET /state-hash` | reader | 查看状态哈希 |
| `GET /oplog` | reader | 查看操作日志 |
| `GET /history` | reader | 查看操作历史 |
| `GET /conflicts` | reader | 查看冲突信息 |
| `GET /health` | 无 | 健康检查 |

## 测试与验证

- 支持多实例模拟分布式同步
- 验证最终一致性（所有节点状态收敛）
- 性能指标：合并延迟、冲突率、状态收敛时间

### 验证原理与流程

- 核心属性说明：
  - 收敛性（Convergence）：所有副本在网络最终稳定后达到相同状态。验证方式：对各节点最终状态生成确定性哈希（如基于序列化后的状态 `state_hash`）或结构化快照，对比哈希/快照是否一致。
  - 幂等性（Idempotence）：同一批操作/状态重复合并不改变结果。验证方式：对同一快照执行多次 `merge`，比较前后 `state_hash` 是否相同，同时检查操作日志重复应用后的状态等价。
  - 交换性（Commutativity）：并发操作的合并顺序不影响结果。验证方式：对相同操作集应用不同乱序排列并合并，比较最终 `state_hash` 是否一致。

- Silent-CRDT 验证机制（建议实现/对接）：
  - 操作日志（OpLog）：为每条变更记录操作 ID（推荐 `scru128`）、因果元数据与操作内容，用于重放与外部验证。
  - 版本向量 / 向量时钟：在 `merge` 时用于判定并发与因果关系，辅助检测缺失或重复消息。
  - 状态哈希：对 CRDT 规范化状态（排序后的键集、因果元信息）进行哈希，作为最终一致性的判据；同时可输出到验证报告。

- 自动化验证流程（标准步骤）：
  1. 多节点初始化：创建 N 个逻辑副本，分别分配节点 ID。
  2. 各节点独立执行操作：随机或预设操作序列，写入各自 OpLog。
  3. 延迟/乱序同步：模拟网络延迟、丢包、乱序，随机选择对等节点执行 `merge`。
  4. 最终状态合并对比：在网络“稳定”后收集各节点 `state_hash` 并断言一致；重复合并以验证幂等。
  5. 输出验证报告：打印收敛性/幂等性/交换性结果与指标统计（通过/失败）。

示例命令（按需在 `tests/` 中提供对应用例）：
```bash
cargo test --test convergence -- --nocapture
cargo test --test idempotence -- --nocapture
cargo test --test commutativity -- --nocapture
```

在调试时开启详细日志或追踪特性以观察合并事件：
```bash
RUST_LOG=debug cargo test --test convergence -- --nocapture
cargo test --features trace --test convergence -- --nocapture
```

### 本地测试步骤

1. 启动多个 Silent-CRDT 实例（例如分别运行在 8080、8081 端口）：
   ```bash
   cargo run -- --port 8080
   cargo run -- --port 8081
   ```
2. 向第一个实例提交变更：
   ```bash
   curl -X POST http://127.0.0.1:8080/sync -d '{"changes":[{"op":"add","key":"user","value":"Alice"}]}'
   ```
3. 触发两个节点之间的同步：
   ```bash
   curl -X POST http://127.0.0.1:8080/sync-peer -d '{"peer":"127.0.0.1:8081"}'
   ```
4. 检查第二个节点的状态，确认数据已收敛（可对比状态哈希或状态快照）。

### 单元测试与集成测试

- 使用 `cargo test` 运行单元测试与一致性场景用例。
- 在 `tests/` 目录下编写多节点模拟用例，通过异步任务与概率性网络故障（延迟/丢包/乱序）模拟并发更新与同步。
- 使用 `RUST_LOG=debug` 或 `--features trace` 观察 `merge`、因果判定与冲突解决日志。

### 验证工具集成

- 外部工具建议：
  - `automerge-rs` 测试模块：借鉴其并发与等价性测试思路，作为对照实验或回归基线。
  - `jepsen-knossos` / `antidote-verifier`：用于外部一致性验证与历史检查（需要将操作日志导出为其可消费格式）。
  - 自定义 Rust 随机测试脚本：基于统一 `Op` 生成器批量产生随机操作与乱序合并，自动发现不收敛场景。
- 导出操作日志到 JSON（用于外部验证或重放）：
  ```json
  {
    "node": "n1",
    "ops": [
      {
        "id": "01HZX...SCRU128",
        "causal": { "vv": { "n1": 3, "n2": 1 } },
        "ts": 1712312345678,
        "op": { "type": "or-set.add", "key": "k", "val": "v" }
      }
    ]
  }
  ```
  建议提供导出命令或 API（例如 `GET /oplog?n=n1`），并将样例日志保存到 `docs/validation/` 便于外部工具消费。

### 性能与一致性验证建议

- **延迟测试**：记录合并操作的耗时（ms）。
- **冲突测试**：在多节点同时修改同一 key，验证最终合并结果一致。
- **压力测试**：逐步增加节点数量与更新频率，观察收敛时间变化。

### 性能验证指标

| 指标 | 含义 |
|------|------|
| 合并延迟 (ms) | 完成一次 merge 操作的平均耗时 |
| 冲突率 (%) | 并发更新产生冲突的比例 |
| 收敛时间 (ms) | 所有节点状态一致所需时间 |
| 状态体积 (KB) | 每节点状态数据大小 |

基准测试建议：
- 使用 `criterion` 编写基准，统计不同 CRDT 类型在不同数据规模下的合并/应用操作耗时。
- 使用 `cargo bench` 在 CI 或本地持续跟踪回归：
  ```bash
  cargo bench
  ```
- 记录并输出到 `docs/bench/`，便于对比不同实现/参数的差异。

### 可视化与结果分析

- 建议生成以下报告或图表：
  - 多节点状态收敛曲线（时间-哈希一致性比例）。
  - 操作吞吐量与合并延迟散点图（发现尾延迟）。
  - 不同节点合并顺序的结果对比表（验证交换性）。
- Mermaid 时序图示例：
  ```mermaid
  sequenceDiagram
      participant A
      participant B
      participant C
      A->>B: 同步状态
      B->>C: 合并更新
      C->>A: 回传确认（状态收敛）
  ```

输出验证报告示例（文本）：
```text
Convergence: PASS (N=5, hashes equal)
Idempotence: PASS (repeat merges unchanged)
Commutativity: PASS (3 permutations equal)
Merge p50/p95: 2.1ms / 6.7ms
Conflicts: 3.2% (resolved by CRDT rules)
```

## 当前功能与路线图

- ✅ 基础 CRDT 类型（Counter / Set / Map）
- ✅ 网络同步接口（HTTP RESTful API）
- ✅ 多节点状态回放与冲突可视化
  - ✅ 操作历史时间线展示
  - ✅ 并发冲突自动检测
  - ✅ 因果上下文可视化
  - ✅ LWW 冲突解决策略展示
- ✅ 权限控制与版本签名
  - ✅ 基于 JWT 的身份认证
  - ✅ 基于角色的访问控制（RBAC）
  - ✅ Ed25519 数字签名支持
  - ✅ 权限验证中间件
- ✅ gRPC 接口支持
  - ✅ Protocol Buffers 定义
  - ✅ gRPC 服务端实现
  - ✅ 完整的 RPC 方法支持
  - ✅ gRPC 客户端示例

## 关联项目

- [silent-quic](https://github.com/silent-rs/silent-quic) — 高性能传输层
- [silent-nas](https://github.com/silent-rs/silent-nas) — 文件同步与分布式存储验证
