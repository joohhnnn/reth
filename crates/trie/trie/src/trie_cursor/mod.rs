use crate::{BranchNodeCompact, Nibbles};
use alloy_primitives::B256;
use reth_storage_errors::db::DatabaseError;

/// In-memory implementations of trie cursors.
mod in_memory;

/// Cursor for iterating over a subtrie.
pub mod subnode;

/// Noop trie cursor implementations.
pub mod noop;

/// Depth-first trie iterator.
pub mod depth_first;

/// Mock trie cursor implementations.
#[cfg(any(test, feature = "test-utils"))]
pub mod mock;

/// Metrics tracking trie cursor implementations.
pub mod metrics;
#[cfg(feature = "metrics")]
pub use metrics::TrieCursorMetrics;
pub use metrics::{InstrumentedTrieCursor, TrieCursorMetricsCache};

pub use self::{depth_first::DepthFirstTrieIterator, in_memory::*, subnode::CursorSubNode};

/// Trie 游标工厂 trait。
///
/// 创建用于读取数据库中已存储的 trie 分支节点（BranchNodeCompact）的游标。
/// 这些分支节点是 Merkle Patricia Trie 的中间节点，存储了子节点的哈希值。
///
/// ## 两种游标
/// - 账户 trie 游标: 遍历全局状态 trie 的中间节点
/// - 存储 trie 游标: 遍历特定账户的存储 trie 的中间节点
#[auto_impl::auto_impl(&)]
pub trait TrieCursorFactory {
    /// 账户 trie 游标类型。
    type AccountTrieCursor<'a>: TrieCursor
    where
        Self: 'a;

    /// 存储 trie 游标类型。
    type StorageTrieCursor<'a>: TrieStorageCursor
    where
        Self: 'a;

    /// 创建账户 trie 游标（用于遍历全局状态 trie 的中间节点）。
    fn account_trie_cursor(&self) -> Result<Self::AccountTrieCursor<'_>, DatabaseError>;

    /// 创建存储 trie 游标（用于遍历指定账户的存储 trie 中间节点）。
    fn storage_trie_cursor(
        &self,
        hashed_address: B256,
    ) -> Result<Self::StorageTrieCursor<'_>, DatabaseError>;
}

/// Trie 游标 trait —— 用于遍历数据库中存储的 trie 分支节点。
///
/// 游标必须按字典序迭代键。返回的 `BranchNodeCompact` 包含:
/// - `state_mask`: 哪些子节点存在
/// - `tree_mask`: 哪些子节点在数据库中有对应的 trie 节点
/// - `hash_mask`: 哪些子节点有存储的哈希值
/// - `hashes`: 子节点的哈希值列表
///
/// 这些信息让 TrieWalker 能够决定哪些子树需要遍历、哪些可以跳过。
#[auto_impl::auto_impl(&mut)]
pub trait TrieCursor {
    /// 精确查找: 移动游标到指定键，如果精确匹配则返回。
    fn seek_exact(
        &mut self,
        key: Nibbles,
    ) -> Result<Option<(Nibbles, BranchNodeCompact)>, DatabaseError>;

    /// 范围查找: 移动游标到大于等于指定键的位置。
    fn seek(&mut self, key: Nibbles)
        -> Result<Option<(Nibbles, BranchNodeCompact)>, DatabaseError>;

    /// 移动游标到下一个键。
    fn next(&mut self) -> Result<Option<(Nibbles, BranchNodeCompact)>, DatabaseError>;

    /// 获取当前条目的键。
    fn current(&mut self) -> Result<Option<Nibbles>, DatabaseError>;

    /// 重置游标到起始位置。重置后必须先调用 seek 或 seek_exact。
    fn reset(&mut self);
}

/// 存储 trie 游标 trait —— 继承 TrieCursor，增加设置账户地址的能力。
///
/// 由于存储 trie 是按账户隔离的，需要先设置目标账户地址，
/// 然后才能遍历该账户的存储 trie 节点。
#[auto_impl::auto_impl(&mut)]
pub trait TrieStorageCursor: TrieCursor {
    /// 设置存储 trie 游标的目标账户哈希地址。
    /// 设置后必须先调用 seek 或 seek_exact。
    fn set_hashed_address(&mut self, hashed_address: B256);
}

/// Iterator wrapper for `TrieCursor` types
#[derive(Debug)]
pub struct TrieCursorIter<'a, C> {
    cursor: &'a mut C,
    /// The initial value from seek, if any
    initial: Option<Result<(Nibbles, BranchNodeCompact), DatabaseError>>,
}

impl<'a, C> TrieCursorIter<'a, C> {
    /// Create a new iterator from a mutable reference to a cursor. The Iterator will start from the
    /// empty path.
    pub fn new(cursor: &'a mut C) -> Self
    where
        C: TrieCursor,
    {
        let initial = cursor.seek(Nibbles::default()).transpose();
        Self { cursor, initial }
    }
}

impl<'a, C> From<&'a mut C> for TrieCursorIter<'a, C>
where
    C: TrieCursor,
{
    fn from(cursor: &'a mut C) -> Self {
        Self::new(cursor)
    }
}

impl<'a, C> Iterator for TrieCursorIter<'a, C>
where
    C: TrieCursor,
{
    type Item = Result<(Nibbles, BranchNodeCompact), DatabaseError>;

    fn next(&mut self) -> Option<Self::Item> {
        // If we have an initial value from seek, return it first
        if let Some(initial) = self.initial.take() {
            return Some(initial);
        }

        self.cursor.next().transpose()
    }
}
