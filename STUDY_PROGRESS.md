# Reth 仓库学习进度

> 总计 ~129 个 crate，按依赖关系从底向上分为 7 个学习层级。
> 每完成一个模块的学习/注释，打勾标记。

---

## 整体架构图

```
                          ┌─────────────┐
                          │   bin/reth   │  ← 主程序入口
                          └──────┬──────┘
                                 │
                    ┌────────────┴────────────┐
                    │   reth-node-builder     │  ← 节点编排层
                    └────────────┬────────────┘
                                 │
          ┌──────────┬───────────┼───────────┬──────────┐
          │          │           │           │          │
     ┌────┴───┐ ┌───┴────┐ ┌───┴───┐ ┌────┴───┐ ┌───┴─────┐
     │ engine │ │  rpc   │ │stages │ │network │ │ tx-pool │  ← 服务层
     └────┬───┘ └───┬────┘ └───┬───┘ └────┬───┘ └───┬─────┘
          │         │          │           │         │
     ┌────┴─────────┴──────────┴───────────┴─────────┴────┐
     │              reth-provider (storage-api)            │  ← 数据访问层
     └────────────────────────┬────────────────────────────┘
                              │
          ┌───────────┬───────┼───────┬───────────┐
          │           │       │       │           │
     ┌────┴───┐ ┌────┴──┐ ┌─┴──┐ ┌──┴─────┐ ┌──┴──────┐
     │  trie  │ │  evm  │ │ db │ │chainspec│ │consensus│  ← 核心层
     └────┬───┘ └────┬──┘ └─┬──┘ └──┬─────┘ └──┬──────┘
          │          │      │       │           │
     ┌────┴──────────┴──────┴───────┴───────────┴────┐
     │           primitives / primitives-traits       │  ← 基础类型层
     └────────────────────────────────────────────────┘
```

---

## 第 1 层：基础类型（地基）

> 所有模块都依赖这一层，是最底层的数据结构定义。

- [ ] **primitives-traits** (`crates/primitives-traits/`) - 核心 trait 定义（Block, Transaction, Receipt 等）
- [ ] **primitives** (`crates/primitives/`) - 通用原语类型实现
- [ ] **errors** (`crates/errors/`) - 通用错误类型
- [ ] **chainspec** (`crates/chainspec/`) - 链规范（创世配置、硬分叉规则）

---

## 第 2 层：核心引擎（心脏）

> 区块链的核心逻辑：执行交易、验证共识、计算状态根、存储数据。

### 2A - EVM 执行
- [ ] **evm/evm** (`crates/evm/evm/`) - ConfigureEvm trait、Executor、BlockBuilder
- [ ] **evm/execution-types** (`crates/evm/execution-types/`) - ExecutionOutcome、Chain
- [ ] **evm/execution-errors** (`crates/evm/execution-errors/`) - 执行错误类型
- [ ] **revm** (`crates/revm/`) - Reth 对 revm 的包装和扩展

### 2B - 共识验证
- [ ] **consensus/consensus** (`crates/consensus/consensus/`) - Consensus trait 定义
- [ ] **consensus/common** (`crates/consensus/common/`) - 通用共识校验实现

### 2C - Merkle Trie
- [ ] **trie/common** (`crates/trie/common/`) - Trie 通用类型
- [ ] **trie/trie** (`crates/trie/trie/`) - StateRoot / StorageRoot 核心实现
- [ ] **trie/sparse** (`crates/trie/sparse/`) - 稀疏 trie
- [ ] **trie/sparse-parallel** (`crates/trie/sparse-parallel/`) - 并行稀疏 trie
- [ ] **trie/parallel** (`crates/trie/parallel/`) - ParallelStateRoot 并行状态根
- [ ] **trie/db** (`crates/trie/db/`) - Trie 数据库游标操作

### 2D - 存储引擎
- [ ] **storage/db-api** (`crates/storage/db-api/`) - 数据库 trait 抽象
- [ ] **storage/db** (`crates/storage/db/`) - MDBX 数据库实现
- [ ] **storage/db-common** (`crates/storage/db-common/`) - 数据库通用工具
- [ ] **storage/db-models** (`crates/storage/db-models/`) - 数据库表模型定义
- [ ] **storage/codecs** (`crates/storage/codecs/`) - 编解码器
- [ ] **storage/nippy-jar** (`crates/storage/nippy-jar/`) - 压缩归档格式
- [ ] **storage/storage-api** (`crates/storage/storage-api/`) - **存储 Provider trait（核心抽象）**
- [ ] **storage/provider** (`crates/storage/provider/`) - **存储 Provider 实现（数据访问统一入口）**

---

## 第 3 层：网络协议（与外界通信）

> P2P 网络栈：节点发现、协议握手、区块/交易传播。

- [ ] **net/eth-wire-types** (`crates/net/eth-wire-types/`) - ETH 协议消息类型
- [ ] **net/eth-wire** (`crates/net/eth-wire/`) - ETH 协议编解码
- [ ] **net/ecies** (`crates/net/ecies/`) - ECIES 加密传输
- [ ] **net/discv4** (`crates/net/discv4/`) - 节点发现协议 v4
- [ ] **net/discv5** (`crates/net/discv5/`) - 节点发现协议 v5
- [ ] **net/dns** (`crates/net/dns/`) - DNS 节点发现
- [ ] **net/nat** (`crates/net/nat/`) - NAT 穿透
- [ ] **net/peers** (`crates/net/peers/`) - 节点管理
- [ ] **net/network-api** (`crates/net/network-api/`) - 网络 API trait
- [ ] **net/network** (`crates/net/network/`) - **网络核心实现**
- [ ] **net/downloaders** (`crates/net/downloaders/`) - 区块下载器
- [ ] **net/p2p** (`crates/net/p2p/`) - P2P 抽象
- [ ] **net/banlist** (`crates/net/banlist/`) - 节点黑名单

---

## 第 4 层：同步与服务（让节点运转）

> 分阶段同步、交易池、共识引擎 —— 节点的"业务逻辑"。

### 4A - 分阶段同步（Pipeline）
- [ ] **stages/api** (`crates/stages/api/`) - Stage trait 定义
- [ ] **stages/stages** (`crates/stages/stages/`) - 各 Stage 实现（Headers, Bodies, Execution…）

### 4B - 交易池
- [ ] **transaction-pool** (`crates/transaction-pool/`) - 内存交易池（排序、验证、Blob 支持）

### 4C - 共识引擎（Engine API）
- [ ] **engine/primitives** (`crates/engine/primitives/`) - Engine 原语
- [ ] **engine/tree** (`crates/engine/tree/`) - **Engine 树（分叉管理核心）**
- [ ] **engine/service** (`crates/engine/service/`) - Engine 服务
- [ ] **engine/local** (`crates/engine/local/`) - 本地引擎（开发/测试用）

### 4D - 区块构建（Payload）
- [ ] **payload/primitives** (`crates/payload/primitives/`) - Payload 原语
- [ ] **payload/builder** (`crates/payload/builder/`) - Payload 构建器
- [ ] **payload/basic** (`crates/payload/basic/`) - 基本构建器实现
- [ ] **payload/validator** (`crates/payload/validator/`) - Payload 验证

---

## 第 5 层：对外接口（RPC）

> JSON-RPC 服务器，对外提供 eth_*, debug_*, trace_* 等 API。

- [ ] **rpc/rpc-api** (`crates/rpc/rpc-api/`) - RPC API trait 定义
- [ ] **rpc/rpc-eth-api** (`crates/rpc/rpc-eth-api/`) - eth 命名空间
- [ ] **rpc/rpc-eth-types** (`crates/rpc/rpc-eth-types/`) - eth RPC 类型
- [ ] **rpc/rpc-engine-api** (`crates/rpc/rpc-engine-api/`) - Engine API
- [ ] **rpc/rpc-builder** (`crates/rpc/rpc-builder/`) - RPC 服务器构建器
- [ ] **rpc/rpc** (`crates/rpc/rpc/`) - RPC 实现主模块

---

## 第 6 层：节点编排（拼装完整节点）

> 把前面所有组件组合成一个完整的、可运行的节点。

- [ ] **node/types** (`crates/node/types/`) - 节点类型
- [ ] **node/api** (`crates/node/api/`) - 节点 API trait
- [ ] **node/core** (`crates/node/core/`) - 节点核心
- [ ] **node/builder** (`crates/node/builder/`) - **NodeBuilder（节点组装器）**
- [ ] **chain-state** (`crates/chain-state/`) - 链状态管理

---

## 第 7 层：具体链实现（以太坊 / Optimism）

> 基于前面的抽象框架，针对具体链的完整实现。

### 7A - 以太坊主网
- [ ] **ethereum/hardforks** (`crates/ethereum/hardforks/`) - 硬分叉定义
- [ ] **ethereum/primitives** (`crates/ethereum/primitives/`) - 以太坊原语
- [ ] **ethereum/consensus** (`crates/ethereum/consensus/`) - 以太坊共识
- [ ] **ethereum/evm** (`crates/ethereum/evm/`) - EthEvmConfig
- [ ] **ethereum/payload** (`crates/ethereum/payload/`) - 以太坊 Payload 构建
- [ ] **ethereum/engine-primitives** (`crates/ethereum/engine-primitives/`) - Engine 原语
- [ ] **ethereum/node** (`crates/ethereum/node/`) - **以太坊完整节点配置**
- [ ] **ethereum/cli** (`crates/ethereum/cli/`) - 以太坊 CLI
- [ ] **bin/reth** (`bin/reth/`) - **主程序入口**

### 7B - Optimism L2（选学）
- [ ] **optimism/** (`crates/optimism/`) - OP Stack 完整实现（15 个子 crate）

---

## 辅助模块（按需查阅）

- [ ] **exex** (`crates/exex/`) - 执行扩展框架（ExEx）
- [ ] **prune** (`crates/prune/`) - 数据修剪
- [ ] **static-file** (`crates/static-file/`) - 静态文件归档
- [ ] **etl** (`crates/etl/`) - ETL 工具
- [ ] **era** (`crates/era/`) - Era 归档格式
- [ ] **cli** (`crates/cli/`) - CLI 框架
- [ ] **config** (`crates/config/`) - 配置文件
- [ ] **metrics** (`crates/metrics/`) - Metrics 指标
- [ ] **tracing** (`crates/tracing/`) - 日志追踪
- [ ] **tasks** (`crates/tasks/`) - 异步任务管理
- [ ] **ress** (`crates/ress/`) - 状态同步协议
- [ ] **stateless** (`crates/stateless/`) - 无状态执行

---

## 推荐学习顺序

```
第 1 层 → 第 2 层(EVM→共识→Trie→存储) → 第 4 层(同步→引擎) → 第 3 层(网络) → 第 5 层(RPC) → 第 6 层(节点) → 第 7 层(以太坊)
```

**为什么这个顺序？**
1. **基础类型**先看，后面每个模块都用到
2. **EVM/共识/Trie** 是核心算法，理解了才知道节点在"干什么"
3. **存储**是数据怎么持久化的
4. **同步和引擎**是节点怎么跟上链的
5. **网络**是节点怎么互相通信的
6. **RPC**是外部怎么查询节点的
7. **节点编排**是所有组件怎么拼在一起的
8. **以太坊实现**是上述抽象的具体落地

---

*最后更新: 2026-02-08*

---
---

# AI 对话式教学 Session

## 教学方法 Prompt

> **请读完本文件后，用以下方式继续教我 reth 源码。**

### 教学原则

用 **"角色对话 + 提问"** 的方式教我。核心规则：

1. **每个模块是一个角色**，有自己的"性格"和说话方式（比如 RPC 前台只管转发、验签员很严格、交易池像候车大厅）
2. **每一站给出真实代码路径和行号**，格式为 `crates/xxx/src/xxx.rs:行号`，方便我点击跳转
3. **贴关键代码片段**，不需要全部，只贴最核心的几行
4. **每隔 3-5 步插入一个选择题**，让我预测下一步会发生什么，我答完后再揭晓并解释
5. **每一站开头先更新全局地图**，用 👉 标出"你在这里"，让我始终知道当前位置
6. **用中文讲解**，代码保持英文

### 教学格式示例

每一站应该长这样：

```
当前位置：
[RPC前台] ──→ 👉[验签室] ──→ [批处理站] ──→ [交易池] ──→ [调度中心] ──→ [EVM] ──→ [状态写入]

📍 crates/rpc/rpc-eth-api/src/helpers/transaction.rs:79-87

（贴核心代码）

> **[验签员]** "让我看看这串字节... 先用 EIP-2718 解码，然后恢复发送者地址。"

（解释要点）

### ❓ 问题
接下来会到哪个模块？
- A. xxx
- B. xxx
- C. xxx
```

---

## 当前主题：一笔交易的一生

### 全局地图

```
你 (用户)
 │  eth_sendRawTransaction(0xf86c...)
 ▼
┌──────────┐    ┌───────────┐    ┌───────────┐
│ RPC 前台  │───→│  交易解码  │───→│  批处理站  │
│ (接待员)  │    │ (验证签名) │    │ (收发室)   │
└──────────┘    └───────────┘    └───────────┘
                                       │
                                       ▼
┌──────────┐    ┌───────────┐    ┌───────────┐
│ 状态写入  │←───│  EVM 工厂  │←───│ 交易池     │
│ (档案馆)  │    │ (执行车间) │    │ (候车大厅) │
└──────────┘    └───────────┘    └───────────┘
                     ↑
                ┌───────────┐
                │ Payload   │
                │ Builder   │
                │(调度中心)  │
                └───────────┘
```

### ✅ 已完成

#### 第一站：RPC 前台（接待员）
- 📍 `crates/rpc/rpc-eth-api/src/core.rs:845-849`
- RPC Server 收到 `eth_sendRawTransaction`，记日志后直接转发，本身不做处理
- 关键代码：
```rust
async fn send_raw_transaction(&self, tx: Bytes) -> RpcResult<B256> {
    trace!(target: "rpc::eth", ?tx, "Serving eth_sendRawTransaction");
    Ok(EthTransactions::send_raw_transaction(self, tx).await?)
}
```

#### 第二站：交易解码（验签室）
- 📍 `crates/rpc/rpc-eth-api/src/helpers/transaction.rs:79-87`（入口）
- 📍 `crates/rpc/rpc-eth-types/src/utils.rs:35-45`（核心解码函数）
- 三步：检查非空 → EIP-2718 解码 → 从签名恢复发送者地址
- 关键代码：
```rust
pub fn recover_raw_transaction<T: SignedTransaction>(data: &[u8]) -> EthResult<Recovered<T>> {
    if data.is_empty() {
        return Err(EthApiError::EmptyRawTransactionData)
    }
    let transaction = T::decode_2718_exact(data)
        .map_err(|_| EthApiError::FailedToDecodeSignedTransaction)?;
    SignedTransaction::try_into_recovered(transaction)
        .or(Err(EthApiError::InvalidTransactionSignature))
}
```

#### 第三站：送往交易池前的分叉
- 📍 `crates/rpc/rpc/src/eth/helpers/transaction.rs:39-122`
- 可选转发给外部端点（MEV builder）
- 广播原始交易给 WebSocket 订阅者（newPendingTransactions）
- 调用 `add_pool_transaction()` 提交到本地池
- 所有 RPC 来的交易标记为 `TransactionOrigin::Local`

### ⏳ 当前停在：第四站 - 批处理站

**上一个问题待回答：** 交易为什么要经过批处理站（Batcher）而不是直接进池？
- A. 防止 DDoS 限速
- B. 攒多笔交易批量处理提高吞吐
- C. 异步转同步

等我回答后继续。

关键代码位置（供 AI 参考）：
- 📍 `crates/rpc/rpc/src/eth/core.rs:557-569` — `add_pool_transaction`，将交易包装为 `BatchTxRequest` 发送到 channel
- 📍 `crates/transaction-pool/src/batcher.rs:68-89` — 批处理器 `process_batch`，单笔直接处理，多笔批量处理

### 📋 待讲解站点（供 AI 参考的代码位置）

#### 第五站：交易池验证
- 📍 `crates/transaction-pool/src/lib.rs:506-514` — `Pool::add_transaction` 入口
- 📍 `crates/transaction-pool/src/validate/eth.rs:631-664` — nonce/balance 验证
- 📍 `crates/transaction-pool/src/validate/eth.rs:846-852` — `validate_transaction` 异步函数
- 验证分两层：Stateless（格式/gas/链ID）和 Stateful（nonce/余额/code hash）

#### 第六站：子池分配（候车大厅的四个区域）
- 📍 `crates/transaction-pool/src/pool/txpool.rs:729-839` — `TxPool::add_transaction` 核心插入
- 📍 `crates/transaction-pool/src/pool/txpool.rs:1915-2099` — `AllTransactions::insert_tx` 状态判断
- 📍 `crates/transaction-pool/src/pool/state.rs:8-48` — TxState 位标志定义和 SubPool 映射规则
- 四个子池：
  - **Pending**：就绪，满足所有条件（无 nonce gap、余额够、fee 够）
  - **BaseFee**：有效但 maxFeePerGas < 当前 base fee
  - **Blob**：EIP-4844 交易，blob fee 不足
  - **Queued**：有 nonce gap 或余额不足

#### 第七站：Payload Builder（调度中心）
- 📍 `crates/ethereum/payload/src/lib.rs:138-385` — `default_ethereum_payload` 主函数
- 📍 `crates/ethereum/payload/src/lib.rs:183-186` — 从池获取 `best_txs` 迭代器
- 📍 `crates/ethereum/payload/src/lib.rs:216-353` — 主循环逐笔处理
- 📍 `crates/transaction-pool/src/ordering.rs:69-86` — `CoinbaseTipOrdering`，按 effective tip 排序
- 📍 `crates/payload/basic/src/lib.rs:341-362` — `BasicPayloadJob::spawn_build_job`

#### 第八站：EVM 执行（执行车间）
- 📍 `crates/evm/evm/src/execute.rs:354-426` — `BlockBuilder` trait 定义
- 📍 `crates/evm/evm/src/execute.rs:395-399` — `execute_transaction()` 签名
- 📍 `crates/evm/evm/src/execute.rs:476-549` — `BasicBlockBuilder` 实现
- 📍 `crates/evm/evm/src/execute.rs:527` — `db.merge_transitions(BundleRetention::Reverts)`

#### 第九站：状态写入（档案馆）
- 📍 `crates/evm/evm/src/execute.rs:530-533` — 计算 state root + trie updates
- 📍 `crates/evm/evm/src/execute.rs:538-547` — `assembler.assemble_block()` 组装最终区块
- 📍 `crates/storage/storage-api/src/state_writer.rs:85-120` — `StateWriter` trait
- 📍 `crates/storage/provider/src/writer/mod.rs:130-140` — 实际写入 DB
- 写入三部分：plain state changes / hashed state（给 trie 用）/ state reverts（链重组恢复）
- `BlockBuilderOutcome` 包含：execution_result, hashed_state, trie_updates, block
