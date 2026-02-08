//! 稀疏 Merkle Patricia Trie (Sparse MPT) 实现。
//!
//! 稀疏 trie 是一种内存优化的 MPT 实现，核心思想是：
//! - **按需加载**: 未访问的节点以哈希形式存储（盲态/Blind），需要时才加载内容（揭示态/Revealed）
//! - **增量更新**: 只在内存中保留需要操作的部分，避免加载整棵 trie
//! - **高效复用**: 支持在不同 payload 执行间复用已分配的内存
//!
//! ## 核心类型
//! - [`RevealableSparseTrie`]: 盲态/揭示态的包装枚举
//! - [`SerialSparseTrie`]: 串行稀疏 trie 实现（单线程操作）
//! - [`SparseStateTrie`]: 完整的以太坊状态 trie 管理器（账户 trie + 存储 trie）
//! - [`SparseTrie`](traits::SparseTrie): 稀疏 trie 核心操作 trait
//!
//! ## 使用流程
//! ```text
//! 1. 初始状态 → Blind（盲态，仅知道根哈希）
//! 2. reveal_multiproof() → 通过 Merkle 证明揭示需要的节点
//! 3. update_leaf() / remove_leaf() → 修改叶子节点
//! 4. root() → 计算新的根哈希
//! 5. take_updates() → 获取需要写回数据库的变更
//! ```

#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

/// 稀疏状态 trie —— 管理账户 trie 和所有存储 trie 的顶层结构。
mod state;
pub use state::*;

/// 稀疏 trie 核心实现 —— RevealableSparseTrie、SerialSparseTrie 和 SparseNode。
mod trie;
pub use trie::*;

/// trait 定义 —— SparseTrie、SparseTrieExt、LeafUpdate 等。
mod traits;
pub use traits::*;

/// 节点提供者 —— 用于从数据库获取盲态节点内容。
pub mod provider;

#[cfg(feature = "metrics")]
mod metrics;

/// 重新导出稀疏 trie 错误类型。
pub mod errors {
    pub use reth_execution_errors::{
        SparseStateTrieError, SparseStateTrieErrorKind, SparseStateTrieResult, SparseTrieError,
        SparseTrieErrorKind, SparseTrieResult,
    };
}
