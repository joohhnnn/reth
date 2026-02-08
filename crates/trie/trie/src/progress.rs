use crate::{
    hash_builder::HashBuilder,
    trie_cursor::CursorSubNode,
    updates::{StorageTrieUpdates, TrieUpdates},
};
use alloy_primitives::B256;
use reth_primitives_traits::Account;
use reth_stages_types::MerkleCheckpoint;

/// 状态根计算的进度枚举。
///
/// 状态根计算可能在处理大量数据时需要暂停（达到阈值），
/// 以避免长时间阻塞。此枚举表示两种状态:
///
/// - **Complete**: 计算完成，包含最终的根哈希
/// - **Progress**: 计算进行中，包含中间状态以便后续恢复
#[derive(Debug)]
pub enum StateRootProgress {
    /// 完成: (状态根哈希, 遍历的条目总数, trie 更新)
    Complete(B256, usize, TrieUpdates),
    /// 进行中: (中间状态, 遍历的条目数, 已产生的 trie 更新)
    /// 中间状态包含 HashBuilder 和 walker 的栈，可用于恢复计算。
    Progress(Box<IntermediateStateRootState>, usize, TrieUpdates),
}

/// 状态根计算的中间状态（用于断点续传）。
///
/// 当计算被暂停时，保存当前的账户 trie 遍历状态和可能正在进行的存储根计算状态。
#[derive(Debug)]
pub struct IntermediateStateRootState {
    /// 账户根计算的中间状态（HashBuilder + walker 栈 + 最后处理的键）。
    pub account_root_state: IntermediateRootState,
    /// 存储根计算的中间状态（如果有正在进行的存储根计算）。
    pub storage_root_state: Option<IntermediateStorageRootState>,
}

/// The intermediate state of a storage root computation along with the account.
#[derive(Debug)]
pub struct IntermediateStorageRootState {
    /// The intermediate storage trie state.
    pub state: IntermediateRootState,
    /// The account for which the storage root is being computed.
    pub account: Account,
}

impl From<MerkleCheckpoint> for IntermediateStateRootState {
    fn from(value: MerkleCheckpoint) -> Self {
        Self {
            account_root_state: IntermediateRootState {
                hash_builder: HashBuilder::from(value.state),
                walker_stack: value.walker_stack.into_iter().map(CursorSubNode::from).collect(),
                last_hashed_key: value.last_account_key,
            },
            storage_root_state: value.storage_root_checkpoint.map(|checkpoint| {
                IntermediateStorageRootState {
                    state: IntermediateRootState {
                        hash_builder: HashBuilder::from(checkpoint.state),
                        walker_stack: checkpoint
                            .walker_stack
                            .into_iter()
                            .map(CursorSubNode::from)
                            .collect(),
                        last_hashed_key: checkpoint.last_storage_key,
                    },
                    account: Account {
                        nonce: checkpoint.account_nonce,
                        balance: checkpoint.account_balance,
                        bytecode_hash: Some(checkpoint.account_bytecode_hash),
                    },
                }
            }),
        }
    }
}

/// 根计算的中间状态（适用于账户根和存储根）。
///
/// 包含恢复计算所需的全部信息:
/// - HashBuilder: 部分构建的 Merkle 哈希树
/// - walker 栈: TrieWalker 的遍历位置
/// - 最后处理的键: 标记从哪里恢复
#[derive(Debug)]
pub struct IntermediateRootState {
    /// 之前构建的哈希构建器（保存了已计算的部分哈希树）。
    pub hash_builder: HashBuilder,
    /// 之前记录的 walker 栈（保存了 trie 遍历位置）。
    pub walker_stack: Vec<CursorSubNode>,
    /// 最后处理的哈希键（恢复时从此键之后继续）。
    pub last_hashed_key: B256,
}

/// 存储根计算的进度枚举（与 StateRootProgress 类似，但针对单个账户的存储 trie）。
#[derive(Debug)]
pub enum StorageRootProgress {
    /// 完成: (存储根哈希, 遍历的存储槽数, 存储 trie 更新)
    Complete(B256, usize, StorageTrieUpdates),
    /// 进行中: (中间状态, 遍历的存储槽数, 已产生的存储 trie 更新)
    Progress(Box<IntermediateRootState>, usize, StorageTrieUpdates),
}
