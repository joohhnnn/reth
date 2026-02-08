//! 并行稀疏 Merkle Patricia Trie 实现。
//!
//! 将稀疏 trie 分为上层和下层两部分:
//! - **上层子 trie**: 包含浅层节点（路径深度 < 2 nibble）
//! - **下层子 trie**: 256 个独立的子 trie（按前 2 个 nibble 索引），可并行计算哈希
//!
//! 这种分层设计使得哈希更新可以在 256 个下层子 trie 上并行执行，
//! 显著加速状态根计算，尤其在一个区块修改大量不同前缀的键时效果明显。
//!
//! ## 核心类型
//! - [`ParallelSparseTrie`]: 并行稀疏 trie 主结构
//! - [`ParallelismThresholds`]: 控制何时启用并行处理的阈值配置

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

extern crate alloc;

/// 并行稀疏 trie 主实现（ParallelSparseTrie 及其操作方法）。
mod trie;
pub use trie::*;

/// 下层子 trie 实现（LowerSparseSubtrie）。
mod lower;
use lower::*;

#[cfg(feature = "metrics")]
mod metrics;
