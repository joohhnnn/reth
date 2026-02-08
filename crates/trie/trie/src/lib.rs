//! Merkle Patricia Trie（MPT）的实现 —— 一种密码学认证的基数树，用于存储键值对。
//!
//! ## 什么是 MPT？
//! 以太坊使用 MPT 来存储世界状态（所有账户的余额、nonce、代码、存储）。
//! 每个区块头包含一个 `state_root`（状态根），它是整个 MPT 的根哈希。
//! 通过比较根哈希，可以高效验证状态的完整性。
//!
//! ## 本模块的核心组件
//! - [`StateRoot`]: 计算状态根（遍历所有账户）
//! - [`StorageRoot`]: 计算单个账户的存储根
//! - [`TrieWalker`]: 字典序遍历 trie 节点
//! - [`TrieNodeIter`]: 组合 walker 和 hashed cursor 的迭代器
//! - [`trie_cursor`]: 用于导航数据库中已存储的 trie 节点
//! - [`hashed_cursor`]: 用于导航哈希后的账户/存储数据
//! - [`proof`]: Merkle 证明生成
//!
//! ## 增量计算
//! reth 使用前缀集（PrefixSet）追踪哪些路径发生了变更，
//! 只重新计算变更部分，避免遍历整棵树。
//!
//! ## 断点续传
//! 对于大型同步，状态根计算可以在达到阈值后暂停（返回 Progress），
//! 保存中间状态后续恢复，避免长时间阻塞。
//!
//! <https://ethereum.org/en/developers/docs/data-structures-and-encoding/patricia-merkle-trie/>
//!
//! ## Feature Flags
//!
//! - `rayon`: uses rayon for parallel [`HashedPostState`] creation.
//! - `test-utils`: Export utilities for testing

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/reth/main/assets/reth-docs.png",
    html_favicon_url = "https://avatars0.githubusercontent.com/u/97369466?s=256",
    issue_tracker_base_url = "https://github.com/paradigmxyz/reth/issues/"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

/// 前向只读内存游标实现（用于遍历已排序的内存数据）。
pub mod forward_cursor;

/// Trie 游标实现 —— 用于导航数据库中存储的账户 trie 和存储 trie 节点。
/// 提供 seek、next 等操作来遍历已持久化的 trie 分支节点。
pub mod trie_cursor;

/// 哈希状态游标实现 —— 用于导航哈希后的账户和存储数据。
/// 账户地址和存储键经过 keccak256 哈希后按字典序排列。
pub mod hashed_cursor;

/// Trie 遍历器 —— 按字典序遍历 trie 节点。
/// 结合 TrieCursor 和 PrefixSet 来决定哪些子树可以跳过。
pub mod walker;

/// 节点迭代器 —— 组合 TrieWalker（中间节点）和 HashedCursor（叶子节点），
/// 为 HashBuilder 提供按序排列的 trie 元素。
pub mod node_iter;

/// Merkle 证明生成（用于轻客户端验证）。
pub mod proof;

/// Merkle 证明生成 v2（仅叶子节点实现，更高效）。
pub mod proof_v2;

/// Trie 见证数据（witness）生成。
pub mod witness;

/// Trie 变更集计算（追踪新增和删除的节点）。
pub mod changesets;

/// Merkle Patricia Trie 的核心实现 —— StateRoot 和 StorageRoot。
mod trie;
pub use trie::{StateRoot, StorageRoot, TrieType};

/// 状态根检查点进度工具 —— 支持断点续传的大型状态根计算。
mod progress;
pub use progress::{
    IntermediateStateRootState, IntermediateStorageRootState, StateRootProgress,
    StorageRootProgress,
};

/// Trie 计算统计信息（分支数、叶子数、耗时等）。
pub mod stats;

// re-export for convenience
pub use reth_trie_common::*;

/// Trie calculation metrics.
#[cfg(feature = "metrics")]
pub mod metrics;

/// Collection of trie-related test utilities.
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

/// Collection of mock types for testing.
#[cfg(any(test, feature = "test-utils"))]
pub mod mock;

/// Verification of existing stored trie nodes against state data.
pub mod verify;
