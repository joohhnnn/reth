//! 盲态 trie 节点的检索 trait 和默认实现。
//!
//! 在稀疏 trie 中，未揭示的节点处于"盲态"（仅知道哈希值）。
//! 当需要操作这些节点时，需要通过 [`TrieNodeProvider`] 从数据库中获取其内容。
//!
//! ## 核心 trait
//! - [`TrieNodeProviderFactory`]: 创建节点提供者的工厂（分为账户和存储两种）
//! - [`TrieNodeProvider`]: 根据路径获取 trie 节点的提供者
//!
//! ## 默认实现
//! - [`DefaultTrieNodeProvider`]: 始终返回 `None`（用于测试）
//! - [`NoRevealProvider`]: 拒绝揭示任何节点（用于乐观更新，失败时触发 BlindedNode 错误）

use alloy_primitives::{Bytes, B256};
use reth_execution_errors::SparseTrieError;
use reth_trie_common::{Nibbles, TrieMask};

/// Trie 节点提供者工厂 trait。
///
/// 用于创建两种类型的节点提供者:
/// - 账户节点提供者: 获取全局状态 trie 中的盲态节点
/// - 存储节点提供者: 获取特定账户的存储 trie 中的盲态节点
#[auto_impl::auto_impl(&)]
pub trait TrieNodeProviderFactory {
    /// 能够获取盲态账户 trie 节点的提供者类型。
    type AccountNodeProvider: TrieNodeProvider;
    /// 能够获取盲态存储 trie 节点的提供者类型。
    type StorageNodeProvider: TrieNodeProvider;

    /// 返回账户节点提供者。
    fn account_node_provider(&self) -> Self::AccountNodeProvider;

    /// 返回指定账户的存储节点提供者。
    fn storage_node_provider(&self, account: B256) -> Self::StorageNodeProvider;
}

/// 已揭示的盲态 trie 节点 —— 包含原始节点数据和分支节点掩码。
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct RevealedNode {
    /// 原始 trie 节点（RLP 编码的字节）。
    pub node: Bytes,
    /// 分支节点的 tree_mask（指示哪些子节点在数据库中有对应的 trie 条目）。
    pub tree_mask: Option<TrieMask>,
    /// 分支节点的 hash_mask（指示哪些子节点的哈希存储在数据库中）。
    pub hash_mask: Option<TrieMask>,
}

/// Trie 节点提供者 trait —— 根据路径从数据库中获取 trie 节点。
///
/// 当稀疏 trie 在操作过程中遇到盲态节点时，通过此 trait 获取节点内容，
/// 将其从盲态转为揭示态，以便继续操作。
#[auto_impl::auto_impl(&)]
pub trait TrieNodeProvider {
    /// 根据路径获取 trie 节点。返回 None 表示节点不存在或无法获取。
    fn trie_node(&self, path: &Nibbles) -> Result<Option<RevealedNode>, SparseTrieError>;
}

/// 默认 trie 节点提供者工厂 —— 创建始终返回 None 的提供者（用于测试）。
#[derive(PartialEq, Eq, Clone, Default, Debug)]
pub struct DefaultTrieNodeProviderFactory;

impl TrieNodeProviderFactory for DefaultTrieNodeProviderFactory {
    type AccountNodeProvider = DefaultTrieNodeProvider;
    type StorageNodeProvider = DefaultTrieNodeProvider;

    fn account_node_provider(&self) -> Self::AccountNodeProvider {
        DefaultTrieNodeProvider
    }

    fn storage_node_provider(&self, _account: B256) -> Self::StorageNodeProvider {
        DefaultTrieNodeProvider
    }
}

/// 默认 trie 节点提供者 —— 始终返回 `Ok(None)`，表示无法提供任何节点。
#[derive(PartialEq, Eq, Clone, Default, Debug)]
pub struct DefaultTrieNodeProvider;

impl TrieNodeProvider for DefaultTrieNodeProvider {
    fn trie_node(&self, _path: &Nibbles) -> Result<Option<RevealedNode>, SparseTrieError> {
        Ok(None)
    }
}

/// 拒绝揭示节点的提供者 —— 用于乐观更新场景。
///
/// 在 `update_leaves` 中使用，尝试在不进行数据库查询的情况下执行 trie 操作。
/// 当遇到盲态节点需要揭示时，此提供者返回 `None`，
/// 导致操作以 `BlindedNode` 错误失败，随后触发证明获取流程。
#[derive(PartialEq, Eq, Clone, Copy, Default, Debug)]
pub struct NoRevealProvider;

impl TrieNodeProvider for NoRevealProvider {
    fn trie_node(&self, _path: &Nibbles) -> Result<Option<RevealedNode>, SparseTrieError> {
        Ok(None)
    }
}

/// 将 nibble 路径右填充 0 到 32 字节并转为 [`B256`]。
/// 用于将不完整的 trie 路径转换为完整的 32 字节键。
#[inline]
pub fn pad_path_to_key(path: &Nibbles) -> B256 {
    let mut padded = path.pack();
    padded.resize(32, 0);
    B256::from_slice(&padded)
}
