use alloy_primitives::{B256, U256};
use reth_primitives_traits::Account;
use reth_storage_errors::db::DatabaseError;

/// Implementation of hashed state cursor traits for the post state.
mod post_state;
pub use post_state::*;

/// Implementation of noop hashed state cursor.
pub mod noop;

/// Mock trie cursor implementations.
#[cfg(any(test, feature = "test-utils"))]
pub mod mock;

/// Metrics tracking hashed cursor implementations.
pub mod metrics;
#[cfg(feature = "metrics")]
pub use metrics::HashedCursorMetrics;
pub use metrics::{HashedCursorMetricsCache, InstrumentedHashedCursor};

/// 哈希状态游标工厂 trait。
///
/// 创建遍历哈希后状态数据的游标。在以太坊中，账户地址和存储键
/// 经过 keccak256 哈希后存储，以确保在 Merkle Patricia Trie 中均匀分布。
///
/// ## 两种游标
/// - 账户游标: 遍历所有 (keccak256(address) → Account) 映射
/// - 存储游标: 遍历特定账户的 (keccak256(storage_slot) → U256) 映射
///
/// ## 数据源
/// 游标可能从数据库读取，也可能从内存覆盖层（HashedPostState）读取，
/// 或者组合两者（HashedPostStateCursor）。
#[auto_impl::auto_impl(&)]
pub trait HashedCursorFactory {
    /// 哈希账户游标类型（返回 Account: nonce, balance, code_hash 等）。
    type AccountCursor<'a>: HashedCursor<Value = Account>
    where
        Self: 'a;
    /// 哈希存储游标类型（返回 U256 存储值）。
    type StorageCursor<'a>: HashedStorageCursor<Value = U256>
    where
        Self: 'a;

    /// 返回遍历所有哈希账户的游标。
    fn hashed_account_cursor(&self) -> Result<Self::AccountCursor<'_>, DatabaseError>;

    /// 返回遍历指定账户的哈希存储条目的游标。
    fn hashed_storage_cursor(
        &self,
        hashed_address: B256,
    ) -> Result<Self::StorageCursor<'_>, DatabaseError>;
}

/// 哈希条目游标 trait。
///
/// 提供按 keccak256 哈希键的字典序遍历状态数据的接口。
/// 这是状态根计算的叶子数据来源。
///
/// ## 与 TrieCursor 的区别
/// - TrieCursor: 遍历 trie 的中间节点（BranchNodeCompact）
/// - HashedCursor: 遍历实际的账户/存储数据（叶子数据）
#[auto_impl::auto_impl(&mut)]
pub trait HashedCursor {
    /// 游标返回的值类型（Account 或 U256）。
    type Value: std::fmt::Debug;

    /// 查找大于等于给定键的条目并定位游标。
    /// 返回键大于等于查找键的第一个条目。
    fn seek(&mut self, key: B256) -> Result<Option<(B256, Self::Value)>, DatabaseError>;

    /// 移动游标到下一个条目并返回。
    fn next(&mut self) -> Result<Option<(B256, Self::Value)>, DatabaseError>;

    /// 重置游标到初始状态。重置后必须先调用 seek。
    fn reset(&mut self);
}

/// 哈希存储游标 trait —— 继承 HashedCursor，增加存储特有的操作。
#[auto_impl::auto_impl(&mut)]
pub trait HashedStorageCursor: HashedCursor {
    /// 判断当前账户是否没有存储数据（用于空存储优化）。
    fn is_storage_empty(&mut self) -> Result<bool, DatabaseError>;

    /// 设置存储游标的目标账户哈希地址。设置后必须先调用 seek。
    fn set_hashed_address(&mut self, hashed_address: B256);
}
