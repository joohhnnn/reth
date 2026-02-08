//! Trie 通用类型库 —— 整个 trie 模块共享的基础类型定义。
//!
//! 此 crate 定义了 Merkle Patricia Trie 所需的所有基础数据结构，
//! 被 `reth-trie`、`reth-trie-sparse`、`reth-trie-parallel` 等 crate 共同依赖。
//!
//! ## 核心类型
//! - [`Nibbles`]: 半字节路径（MPT 的键按 nibble 索引）
//! - [`TrieAccount`]: trie 中的账户表示（nonce, balance, storage_root, code_hash）
//! - [`BranchNodeCompact`]: 紧凑分支节点（存储在数据库中的中间节点）
//! - [`HashBuilder`]: 增量 Merkle 哈希构建器
//! - [`HashedPostState`]: 哈希后的状态变更（区块执行结果）
//! - [`TrieUpdates`](updates::TrieUpdates): trie 更新缓冲区
//! - [`PrefixSet`](prefix_set::PrefixSet): 前缀集合（标记需要重新计算的 trie 路径）

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/reth/main/assets/reth-docs.png",
    html_favicon_url = "https://avatars0.githubusercontent.com/u/97369466?s=256",
    issue_tracker_base_url = "https://github.com/paradigmxyz/reth/issues/"
)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

/// 懒初始化包装器 —— 延迟排序的 trie 数据容器。
mod lazy;
pub use lazy::{LazyTrieData, SortedTrieData};

/// 哈希后的内存状态 —— HashedPostState、HashedStorage 等。
/// 存储 keccak256(address) → Account 和 keccak256(slot) → U256 的映射。
mod hashed_state;
pub use hashed_state::*;

/// Trie 计算的输入 —— TrieInput 包含前缀集合和游标所需数据。
mod input;
pub use input::{TrieInput, TrieInputSorted};

/// 哈希构建器 —— 增量构建 Merkle 根哈希。
/// 按字典序接收键值对，逐步构建 MPT 并计算根哈希。
pub mod hash_builder;

/// Trie 计算相关的常量（如 TRIE_ACCOUNT_RLP_MAX_SIZE）。
mod constants;
pub use constants::*;

/// TrieAccount —— trie 中账户的 RLP 编码格式。
mod account;
pub use account::TrieAccount;

/// 键哈希器 —— 将原始键转为 trie 路径的哈希函数。
mod key;
pub use key::{KeccakKeyHasher, KeyHasher};

/// Nibbles —— 半字节路径类型，MPT 的核心索引单位。
mod nibbles;
pub use nibbles::{Nibbles, StoredNibbles, StoredNibblesSubKey};

/// 存储 trie 条目（数据库存储格式）。
mod storage;
pub use storage::StorageTrieEntry;

/// 子节点引用（TrieWalker 遍历栈中的条目）。
mod subnode;
pub use subnode::StoredSubNode;

/// 分支节点掩码和证明节点类型。
mod trie;
pub use trie::{BranchNodeMasks, BranchNodeMasksMap, ProofTrieNode};

/// 前缀集合 —— 存储 trie 中间变更的容器。
/// 当路径被修改时标记对应前缀，使增量计算知道哪些子树需要重新遍历。
pub mod prefix_set;

/// Merkle 证明相关类型和工具函数。
mod proofs;
#[cfg(any(test, feature = "test-utils"))]
pub use proofs::triehash;
pub use proofs::*;

/// 根哈希计算工具。
pub mod root;

/// 增量有序 trie 根计算。
pub mod ordered_root;

/// Trie 更新缓冲区 —— TrieUpdates 和 StorageTrieUpdates。
pub mod updates;

/// 追踪已添加和已删除的 trie 键。
pub mod added_removed_keys;

/// 内部工具函数。
mod utils;

/// Bincode 兼容的 serde 实现。
///
/// `bincode` 允许更高效地序列化 trie 类型，因为它支持非字符串 map 键。
#[cfg(all(feature = "serde", feature = "serde-bincode-compat"))]
pub mod serde_bincode_compat {
    pub use super::{
        hashed_state::serde_bincode_compat as hashed_state,
        updates::serde_bincode_compat as updates,
    };
}

/// 从 alloy_trie 重新导出的核心类型。
pub use alloy_trie::{nodes::*, proof, BranchNodeCompact, HashBuilder, TrieMask, EMPTY_ROOT_HASH};
