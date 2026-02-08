//! 计算状态根时的错误类型定义。
//!
//! 本模块定义了 Trie 操作过程中可能发生的各种错误:
//! - [`StateRootError`]: 状态根计算错误（数据库错误、存储根错误、前缀集加载错误）
//! - [`StorageRootError`]: 存储根计算错误
//! - [`StateProofError`]: 状态证明生成错误
//! - [`SparseStateTrieError`] / [`SparseTrieError`]: 稀疏 Trie 操作错误
//! - [`TrieWitnessError`]: Trie 见证数据生成错误

use alloc::{boxed::Box, string::ToString};
use alloy_primitives::{Bytes, B256};
use nybbles::Nibbles;
use reth_storage_errors::{db::DatabaseError, provider::ProviderError};
use thiserror::Error;

/// 状态根计算错误。
///
/// 状态根（state root）是以太坊区块头中的关键字段，
/// 它是整个世界状态的 Merkle Patricia Trie 根哈希。
/// 计算过程中可能遇到数据库错误、存储根错误或前缀集加载错误。
#[derive(Error, Clone, Debug)]
pub enum StateRootError {
    /// 内部数据库错误（读取 trie 节点或账户数据失败）。
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// 存储根计算错误（某个账户的存储 trie 计算失败）。
    #[error(transparent)]
    StorageRootError(#[from] StorageRootError),
    /// 加载前缀集（prefix sets）时的 Provider 错误。
    /// 前缀集用于标记哪些 trie 路径发生了变更，需要重新计算。
    #[error(transparent)]
    PrefixSetLoadError(#[from] ProviderError),
}

impl From<StateRootError> for ProviderError {
    fn from(value: StateRootError) -> Self {
        match value {
            StateRootError::Database(err) |
            StateRootError::StorageRootError(StorageRootError::Database(err)) => {
                Self::Database(err)
            }
            StateRootError::PrefixSetLoadError(err) => err,
        }
    }
}

/// Storage root error.
#[derive(Error, Clone, Debug)]
pub enum StorageRootError {
    /// Internal database error.
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

impl From<StorageRootError> for DatabaseError {
    fn from(err: StorageRootError) -> Self {
        match err {
            StorageRootError::Database(err) => err,
        }
    }
}

/// State proof errors.
#[derive(Error, Clone, Debug)]
pub enum StateProofError {
    /// Internal database error.
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// RLP decoding error.
    #[error(transparent)]
    Rlp(#[from] alloy_rlp::Error),
}

impl From<StateProofError> for ProviderError {
    fn from(value: StateProofError) -> Self {
        match value {
            StateProofError::Database(error) => Self::Database(error),
            StateProofError::Rlp(error) => Self::Rlp(error),
        }
    }
}

/// Result type with [`SparseStateTrieError`] as error.
pub type SparseStateTrieResult<Ok> = Result<Ok, SparseStateTrieError>;

/// Error encountered in `SparseStateTrie`.
#[derive(Error, Debug)]
#[error(transparent)]
pub struct SparseStateTrieError(#[from] Box<SparseStateTrieErrorKind>);

impl<T: Into<SparseStateTrieErrorKind>> From<T> for SparseStateTrieError {
    #[cold]
    fn from(value: T) -> Self {
        Self(Box::new(value.into()))
    }
}

impl From<SparseTrieError> for SparseStateTrieErrorKind {
    #[cold]
    fn from(value: SparseTrieError) -> Self {
        Self::Sparse(*value.0)
    }
}

impl SparseStateTrieError {
    /// Returns the error kind.
    pub const fn kind(&self) -> &SparseStateTrieErrorKind {
        &self.0
    }

    /// Consumes the error and returns the error kind.
    pub fn into_kind(self) -> SparseStateTrieErrorKind {
        *self.0
    }
}

/// Error encountered in `SparseStateTrie`.
#[derive(Error, Debug)]
pub enum SparseStateTrieErrorKind {
    /// Encountered invalid root node.
    #[error("invalid root node at {path:?}: {node:?}")]
    InvalidRootNode {
        /// Path to first proof node.
        path: Nibbles,
        /// Encoded first proof node.
        node: Bytes,
    },
    /// Storage sparse trie error.
    #[error("error in storage trie for address {0:?}: {1:?}")]
    SparseStorageTrie(B256, SparseTrieErrorKind),
    /// Sparse trie error.
    #[error(transparent)]
    Sparse(#[from] SparseTrieErrorKind),
    /// RLP error.
    #[error(transparent)]
    Rlp(#[from] alloy_rlp::Error),
}

/// Result type with [`SparseTrieError`] as error.
pub type SparseTrieResult<Ok> = Result<Ok, SparseTrieError>;

/// Error encountered in `SparseTrie`.
#[derive(Error, Debug)]
#[error(transparent)]
pub struct SparseTrieError(#[from] Box<SparseTrieErrorKind>);

impl<T: Into<SparseTrieErrorKind>> From<T> for SparseTrieError {
    #[cold]
    fn from(value: T) -> Self {
        Self(Box::new(value.into()))
    }
}

impl SparseTrieError {
    /// Returns the error kind.
    pub const fn kind(&self) -> &SparseTrieErrorKind {
        &self.0
    }

    /// Consumes the error and returns the error kind.
    pub fn into_kind(self) -> SparseTrieErrorKind {
        *self.0
    }
}

/// [`SparseTrieError`] 的具体错误种类。
///
/// 稀疏 Trie 的节点分为两种状态:
/// - **Blind（盲态）**: 仅存储哈希值，节点内容未加载
/// - **Revealed（揭示态）**: 节点内容已加载到内存
///
/// 当尝试在盲态节点上执行更新/删除操作时，会产生以下错误。
#[derive(Error, Debug)]
pub enum SparseTrieErrorKind {
    /// 稀疏 trie 仍处于盲态。在尝试更新时抛出。
    #[error("sparse trie is blind")]
    Blind,
    /// 更新时遇到了盲态节点（需要先通过 reveal_nodes 加载）。
    #[error("attempted to update blind node at {path:?}: {hash}")]
    BlindedNode {
        /// Blind node path.
        path: Nibbles,
        /// Node hash
        hash: B256,
    },
    /// Encountered unexpected node at path when revealing.
    #[error("encountered an invalid node at path {path:?} when revealing: {node:?}")]
    Reveal {
        /// Path to the node.
        path: Nibbles,
        /// Node that was at the path when revealing.
        node: Box<dyn core::fmt::Debug + Send + Sync>,
    },
    /// RLP error.
    #[error(transparent)]
    Rlp(#[from] alloy_rlp::Error),
    /// Node not found in provider during revealing.
    #[error("node {path:?} not found in provider during revealing")]
    NodeNotFoundInProvider {
        /// Path to the missing node.
        path: Nibbles,
    },
    /// Other.
    #[error(transparent)]
    Other(#[from] Box<dyn core::error::Error + Send + Sync>),
}

/// Trie witness errors.
#[derive(Error, Debug)]
pub enum TrieWitnessError {
    /// Error gather proofs.
    #[error(transparent)]
    Proof(#[from] StateProofError),
    /// RLP decoding error.
    #[error(transparent)]
    Rlp(#[from] alloy_rlp::Error),
    /// Sparse state trie error.
    #[error(transparent)]
    Sparse(#[from] SparseStateTrieError),
    /// Missing account.
    #[error("missing account {_0}")]
    MissingAccount(B256),
}

impl From<SparseStateTrieErrorKind> for TrieWitnessError {
    fn from(error: SparseStateTrieErrorKind) -> Self {
        Self::Sparse(error.into())
    }
}

impl From<TrieWitnessError> for ProviderError {
    fn from(error: TrieWitnessError) -> Self {
        Self::TrieWitnessError(error.to_string())
    }
}
