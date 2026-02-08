use crate::{
    hashed_cursor::HashedCursor, trie_cursor::TrieCursor, walker::TrieWalker, Nibbles, TrieType,
};
use alloy_primitives::B256;
use alloy_trie::proof::AddedRemovedKeys;
use reth_storage_errors::db::DatabaseError;
use tracing::{instrument, trace};

/// Trie 分支节点的表示。
///
/// 当 TrieNodeIter 返回一个中间节点（即有子节点的分支）时，使用此结构。
/// 包含节点的键路径、哈希值和子树是否存在于 trie 中的标志。
#[derive(Debug)]
pub struct TrieBranchNode {
    /// 节点关联的键（nibbles 路径）。
    pub key: Nibbles,
    /// 节点的哈希值（B256），即该子树的根哈希。
    pub value: B256,
    /// 子节点是否在 trie 中（用于 HashBuilder 决定是否标记 tree_mask）。
    pub children_are_in_trie: bool,
}

impl TrieBranchNode {
    /// 创建新的 `TrieBranchNode`。
    pub const fn new(key: Nibbles, value: B256, children_are_in_trie: bool) -> Self {
        Self { key, value, children_are_in_trie }
    }
}

/// 迭代过程中返回的 trie 元素变体。
///
/// 在状态根计算中，HashBuilder 需要按字典序接收两种元素:
/// - **Branch**: 中间分支节点（已有哈希值，可直接使用）
/// - **Leaf**: 叶子节点（实际的账户或存储数据，需要 RLP 编码后哈希）
#[derive(Debug)]
pub enum TrieElement<Value> {
    /// 分支节点（中间节点）。
    Branch(TrieBranchNode),
    /// 叶子节点（键是 keccak256 哈希，值是账户/存储数据）。
    Leaf(B256, Value),
}

/// Result of calling [`HashedCursor::seek`].
#[derive(Debug)]
struct SeekedHashedEntry<V> {
    /// The key that was seeked.
    seeked_key: B256,
    /// The result of the seek.

    /// If no entry was found for the provided key, this will be [`None`].
    result: Option<(B256, V)>,
}

/// Trie 节点迭代器 —— 为 HashBuilder 提供按字典序排列的 trie 元素。
///
/// 这是状态根计算的核心迭代器。它组合了:
/// - `TrieWalker`（遍历数据库中的中间分支节点）
/// - `HashedCursor`（遍历哈希后的叶子数据）
///
/// ## 迭代算法
/// 1. 如果当前中间节点未被更新（可跳过），返回它作为 Branch 元素
/// 2. 推进 walker 到下一个中间节点，获取下一个未处理的键
/// 3. 将哈希游标定位到该键
/// 4. 返回所有在当前中间节点键之前的哈希条目作为 Leaf 元素
/// 5. 重复以上过程
///
/// ## 断点续传
/// 通过 `previous_hashed_key` 支持从上次中断的位置恢复迭代。
#[derive(Debug)]
pub struct TrieNodeIter<C, H: HashedCursor, K> {
    /// 中间节点遍历器。
    pub walker: TrieWalker<C, K>,
    /// 哈希条目游标（用于获取叶子数据）。
    pub hashed_cursor: H,
    /// Trie 类型（State 状态 trie 或 Storage 存储 trie）。
    trie_type: TrieType,
    /// 上一个哈希键。如果迭代之前被中断，此值用于从最后返回的叶子节点恢复迭代。
    previous_hashed_key: Option<B256>,

    /// Current hashed  entry.
    current_hashed_entry: Option<(B256, H::Value)>,
    /// Flag indicating whether we should check the current walker key.
    should_check_walker_key: bool,

    /// The last seeked hashed entry.
    ///
    /// We use it to not seek the same hashed entry twice, and instead reuse it.
    last_seeked_hashed_entry: Option<SeekedHashedEntry<H::Value>>,

    #[cfg(feature = "metrics")]
    metrics: crate::metrics::TrieNodeIterMetrics,
    /// Stores the result of the last successful [`Self::next_hashed_entry`], used to avoid a
    /// redundant [`Self::seek_hashed_entry`] call if the walker points to the same key that
    /// was just returned by `next()`.
    last_next_result: Option<(B256, H::Value)>,
}

impl<C, H: HashedCursor, K> TrieNodeIter<C, H, K>
where
    H::Value: Copy,
    K: AsRef<AddedRemovedKeys>,
{
    /// Creates a new [`TrieNodeIter`] for the state trie.
    pub fn state_trie(walker: TrieWalker<C, K>, hashed_cursor: H) -> Self {
        Self::new(walker, hashed_cursor, TrieType::State)
    }

    /// Creates a new [`TrieNodeIter`] for the storage trie.
    pub fn storage_trie(walker: TrieWalker<C, K>, hashed_cursor: H) -> Self {
        Self::new(walker, hashed_cursor, TrieType::Storage)
    }

    /// Creates a new [`TrieNodeIter`].
    fn new(walker: TrieWalker<C, K>, hashed_cursor: H, trie_type: TrieType) -> Self {
        Self {
            walker,
            hashed_cursor,
            trie_type,
            previous_hashed_key: None,
            current_hashed_entry: None,
            should_check_walker_key: false,
            last_seeked_hashed_entry: None,
            #[cfg(feature = "metrics")]
            metrics: crate::metrics::TrieNodeIterMetrics::new(trie_type),
            last_next_result: None,
        }
    }

    /// Sets the last iterated hashed key and returns the modified [`TrieNodeIter`].
    /// This is used to resume iteration from the last checkpoint.
    pub const fn with_last_hashed_key(mut self, previous_hashed_key: B256) -> Self {
        self.previous_hashed_key = Some(previous_hashed_key);
        self
    }

    /// Seeks the hashed cursor to the given key.
    ///
    /// If the key is the same as the last seeked key, the result of the last seek is returned.
    ///
    /// If `metrics` feature is enabled, it also updates the metrics.
    fn seek_hashed_entry(&mut self, key: B256) -> Result<Option<(B256, H::Value)>, DatabaseError> {
        if let Some((last_key, last_value)) = self.last_next_result &&
            last_key == key
        {
            trace!(target: "trie::node_iter", seek_key = ?key, "reusing result from last next() call instead of seeking");
            self.last_next_result = None; // Consume the cached value

            let result = Some((last_key, last_value));
            self.last_seeked_hashed_entry = Some(SeekedHashedEntry { seeked_key: key, result });

            return Ok(result);
        }

        if let Some(entry) = self
            .last_seeked_hashed_entry
            .as_ref()
            .filter(|entry| entry.seeked_key == key)
            .map(|entry| entry.result)
        {
            #[cfg(feature = "metrics")]
            self.metrics.inc_leaf_nodes_same_seeked();
            return Ok(entry);
        }

        trace!(target: "trie::node_iter", ?key, "performing hashed cursor seek");
        let result = self.hashed_cursor.seek(key)?;
        self.last_seeked_hashed_entry = Some(SeekedHashedEntry { seeked_key: key, result });

        #[cfg(feature = "metrics")]
        {
            self.metrics.inc_leaf_nodes_seeked();
        }
        Ok(result)
    }

    /// Advances the hashed cursor to the next entry.
    ///
    /// If `metrics` feature is enabled, it also updates the metrics.
    fn next_hashed_entry(&mut self) -> Result<Option<(B256, H::Value)>, DatabaseError> {
        let next = self.hashed_cursor.next()?;

        self.last_next_result = next;

        #[cfg(feature = "metrics")]
        {
            self.metrics.inc_leaf_nodes_advanced();
        }
        Ok(next)
    }
}

impl<C, H, K> TrieNodeIter<C, H, K>
where
    C: TrieCursor,
    H: HashedCursor,
    H::Value: Copy,
    K: AsRef<AddedRemovedKeys>,
{
    /// Return the next trie node to be added to the hash builder.
    ///
    /// Returns the nodes using this algorithm:
    /// 1. Return the current intermediate branch node if it hasn't been updated.
    /// 2. Advance the trie walker to the next intermediate branch node and retrieve next
    ///    unprocessed key.
    /// 3. Reposition the hashed cursor on the next unprocessed key.
    /// 4. Return every hashed entry up to the key of the current intermediate branch node.
    /// 5. Repeat.
    ///
    /// NOTE: The iteration will start from the key of the previous hashed entry if it was supplied.
    #[instrument(
        level = "trace",
        target = "trie::node_iter",
        skip_all,
        fields(trie_type = ?self.trie_type),
        ret
    )]
    pub fn try_next(
        &mut self,
    ) -> Result<Option<TrieElement<<H as HashedCursor>::Value>>, DatabaseError> {
        loop {
            // If the walker has a key...
            if let Some(key) = self.walker.key() {
                // Ensure that the current walker key shouldn't be checked and there's no previous
                // hashed key
                if !self.should_check_walker_key && self.previous_hashed_key.is_none() {
                    // Make sure we check the next walker key, because we only know we can skip the
                    // current one.
                    self.should_check_walker_key = true;
                    // If it's possible to skip the current node in the walker, return a branch node
                    if self.walker.can_skip_current_node {
                        #[cfg(feature = "metrics")]
                        self.metrics.inc_branch_nodes_returned();
                        return Ok(Some(TrieElement::Branch(TrieBranchNode::new(
                            *key,
                            self.walker.hash().unwrap(),
                            self.walker.children_are_in_trie(),
                        ))))
                    }
                }
            }

            // If there's a hashed entry...
            if let Some((hashed_key, value)) = self.current_hashed_entry.take() {
                // Check if the walker's key is less than the key of the current hashed entry
                if self.walker.key().is_some_and(|key| key < &Nibbles::unpack(hashed_key)) {
                    self.should_check_walker_key = false;
                    continue
                }

                // Set the next hashed entry as a leaf node and return
                trace!(target: "trie::node_iter", ?hashed_key, "next hashed entry");
                self.current_hashed_entry = self.next_hashed_entry()?;

                #[cfg(feature = "metrics")]
                self.metrics.inc_leaf_nodes_returned();
                return Ok(Some(TrieElement::Leaf(hashed_key, value)))
            }

            // Handle seeking and advancing based on the previous hashed key
            match self.previous_hashed_key.take() {
                Some(hashed_key) => {
                    trace!(target: "trie::node_iter", ?hashed_key, "seeking to the previous hashed entry");
                    // Seek to the previous hashed key and get the next hashed entry
                    self.seek_hashed_entry(hashed_key)?;
                    self.current_hashed_entry = self.next_hashed_entry()?;
                }
                None => {
                    // Get the seek key and set the current hashed entry based on walker's next
                    // unprocessed key
                    let (seek_key, seek_prefix) = match self.walker.next_unprocessed_key() {
                        Some(key) => key,
                        None => break, // no more keys
                    };

                    trace!(
                        target: "trie::node_iter",
                        ?seek_key,
                        can_skip_current_node = self.walker.can_skip_current_node,
                        last = ?self.walker.stack.last(),
                        "seeking to the next unprocessed hashed entry"
                    );
                    let can_skip_node = self.walker.can_skip_current_node;
                    self.walker.advance()?;
                    trace!(
                        target: "trie::node_iter",
                        last = ?self.walker.stack.last(),
                        "advanced walker"
                    );

                    // We should get the iterator to return a branch node if we can skip the
                    // current node and the tree flag for the current node is set.
                    //
                    // `can_skip_node` is already set when the hash flag is set, so we don't need
                    // to check for the hash flag explicitly.
                    //
                    // It is possible that the branch node at the key `seek_key` is not stored in
                    // the database, so the walker will advance to the branch node after it. Because
                    // of this, we need to check that the current walker key has a prefix of the key
                    // that we seeked to.
                    if can_skip_node &&
                        self.walker.key().is_some_and(|key| key.starts_with(&seek_prefix)) &&
                        self.walker.children_are_in_trie()
                    {
                        trace!(
                            target: "trie::node_iter",
                            ?seek_key,
                            walker_hash = ?self.walker.maybe_hash(),
                            "skipping hashed seek"
                        );

                        self.should_check_walker_key = false;
                        continue
                    }

                    self.current_hashed_entry = self.seek_hashed_entry(seek_key)?;
                }
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::{TrieElement, TrieNodeIter};
    use crate::{
        hashed_cursor::{
            mock::MockHashedCursorFactory, noop::NoopHashedCursor, HashedCursorFactory,
            HashedPostStateCursor,
        },
        mock::{KeyVisit, KeyVisitType},
        trie_cursor::{
            mock::MockTrieCursorFactory, noop::NoopAccountTrieCursor, TrieCursorFactory,
        },
        walker::TrieWalker,
    };
    use alloy_primitives::{
        b256,
        map::{B256Map, HashMap},
    };
    use alloy_trie::{
        BranchNodeCompact, HashBuilder, Nibbles, TrieAccount, TrieMask, EMPTY_ROOT_HASH,
    };
    use itertools::Itertools;
    use reth_primitives_traits::Account;
    use reth_trie_common::{
        prefix_set::PrefixSetMut, updates::TrieUpdates, BranchNode, HashedPostState, LeafNode,
        RlpNode,
    };
    use std::collections::BTreeMap;

    /// Calculate the branch node stored in the database by feeding the provided state to the hash
    /// builder and taking the trie updates.
    fn get_hash_builder_branch_nodes(
        state: impl IntoIterator<Item = (Nibbles, Account)> + Clone,
    ) -> HashMap<Nibbles, BranchNodeCompact> {
        let mut hash_builder = HashBuilder::default().with_updates(true);

        let mut prefix_set = PrefixSetMut::default();
        prefix_set.extend_keys(state.clone().into_iter().map(|(nibbles, _)| nibbles));
        let walker = TrieWalker::<_>::state_trie(NoopAccountTrieCursor, prefix_set.freeze());

        let hashed_post_state = HashedPostState::default()
            .with_accounts(state.into_iter().map(|(nibbles, account)| {
                (nibbles.pack().into_inner().unwrap().into(), Some(account))
            }))
            .into_sorted();

        let mut node_iter = TrieNodeIter::state_trie(
            walker,
            HashedPostStateCursor::new_account(
                NoopHashedCursor::<Account>::default(),
                &hashed_post_state,
            ),
        );

        while let Some(node) = node_iter.try_next().unwrap() {
            match node {
                TrieElement::Branch(branch) => {
                    hash_builder.add_branch(branch.key, branch.value, branch.children_are_in_trie);
                }
                TrieElement::Leaf(key, account) => {
                    hash_builder.add_leaf(
                        Nibbles::unpack(key),
                        &alloy_rlp::encode(account.into_trie_account(EMPTY_ROOT_HASH)),
                    );
                }
            }
        }
        hash_builder.root();

        let mut trie_updates = TrieUpdates::default();
        trie_updates.finalize(hash_builder, Default::default(), Default::default());

        trie_updates.account_nodes
    }

    #[test]
    fn test_trie_node_iter() {
        fn empty_leaf_rlp_for_key(key: Nibbles) -> RlpNode {
            RlpNode::from_rlp(&alloy_rlp::encode(LeafNode::new(
                key,
                alloy_rlp::encode(TrieAccount::default()),
            )))
        }

        reth_tracing::init_test_tracing();

        // Extension (Key = 0x0000000000000000000000000000000000000000000000000000000000000)
        // └── Branch (`branch_node_0`)
        //     ├── 0 -> Branch (`branch_node_1`)
        //     │      ├── 0 -> Leaf (`account_1`, Key = 0x0)
        //     │      └── 1 -> Leaf (`account_2`, Key = 0x0)
        //     ├── 1 -> Branch (`branch_node_2`)
        //     │      ├── 0 -> Branch (`branch_node_3`)
        //     │      │      ├── 0 -> Leaf (`account_3`, marked as changed)
        //     │      │      └── 1 -> Leaf (`account_4`)
        //     │      └── 1 -> Leaf (`account_5`, Key = 0x0)

        let account_1 = b256!("0x0000000000000000000000000000000000000000000000000000000000000000");
        let account_2 = b256!("0x0000000000000000000000000000000000000000000000000000000000000010");
        let account_3 = b256!("0x0000000000000000000000000000000000000000000000000000000000000100");
        let account_4 = b256!("0x0000000000000000000000000000000000000000000000000000000000000101");
        let account_5 = b256!("0x0000000000000000000000000000000000000000000000000000000000000110");
        let empty_account = Account::default();

        let hash_builder_branch_nodes = get_hash_builder_branch_nodes(vec![
            (Nibbles::unpack(account_1), empty_account),
            (Nibbles::unpack(account_2), empty_account),
            (Nibbles::unpack(account_3), empty_account),
            (Nibbles::unpack(account_4), empty_account),
            (Nibbles::unpack(account_5), empty_account),
        ]);

        let branch_node_1_rlp = RlpNode::from_rlp(&alloy_rlp::encode(BranchNode::new(
            vec![
                empty_leaf_rlp_for_key(Nibbles::from_nibbles([0])),
                empty_leaf_rlp_for_key(Nibbles::from_nibbles([0])),
            ],
            TrieMask::new(0b11),
        )));

        let branch_node_3_rlp = RlpNode::from_rlp(&alloy_rlp::encode(BranchNode::new(
            vec![
                empty_leaf_rlp_for_key(Nibbles::default()),
                empty_leaf_rlp_for_key(Nibbles::default()),
            ],
            TrieMask::new(0b11),
        )));

        let branch_node_2 = (
            Nibbles::from_nibbles([vec![0; 61], vec![1]].concat()),
            BranchNodeCompact::new(
                TrieMask::new(0b11),
                TrieMask::new(0b00),
                TrieMask::new(0b01),
                vec![branch_node_3_rlp.as_hash().unwrap()],
                None,
            ),
        );
        let branch_node_2_rlp = RlpNode::from_rlp(&alloy_rlp::encode(BranchNode::new(
            vec![branch_node_3_rlp, empty_leaf_rlp_for_key(Nibbles::from_nibbles([0]))],
            TrieMask::new(0b11),
        )));
        let branch_node_0 = (
            Nibbles::from_nibbles([0; 61]),
            BranchNodeCompact::new(
                TrieMask::new(0b11),
                TrieMask::new(0b10),
                TrieMask::new(0b11),
                vec![branch_node_1_rlp.as_hash().unwrap(), branch_node_2_rlp.as_hash().unwrap()],
                None,
            ),
        );

        let mock_trie_nodes = vec![branch_node_0.clone(), branch_node_2.clone()];
        pretty_assertions::assert_eq!(
            hash_builder_branch_nodes.into_iter().sorted().collect::<Vec<_>>(),
            mock_trie_nodes,
        );

        let trie_cursor_factory =
            MockTrieCursorFactory::new(mock_trie_nodes.into_iter().collect(), B256Map::default());

        // Mark the account 3 as changed.
        let mut prefix_set = PrefixSetMut::default();
        prefix_set.insert(Nibbles::unpack(account_3));
        let prefix_set = prefix_set.freeze();

        let walker = TrieWalker::<_>::state_trie(
            trie_cursor_factory.account_trie_cursor().unwrap(),
            prefix_set,
        );

        let hashed_cursor_factory = MockHashedCursorFactory::new(
            BTreeMap::from([
                (account_1, empty_account),
                (account_2, empty_account),
                (account_3, empty_account),
                (account_4, empty_account),
                (account_5, empty_account),
            ]),
            B256Map::default(),
        );

        let mut iter = TrieNodeIter::state_trie(
            walker,
            hashed_cursor_factory.hashed_account_cursor().unwrap(),
        );

        // Walk the iterator until it's exhausted.
        while iter.try_next().unwrap().is_some() {}

        pretty_assertions::assert_eq!(
            *trie_cursor_factory.visited_account_keys(),
            vec![
                KeyVisit {
                    visit_type: KeyVisitType::SeekExact(Nibbles::default()),
                    visited_key: None
                },
                KeyVisit {
                    visit_type: KeyVisitType::SeekNonExact(Nibbles::from_nibbles([0x0])),
                    visited_key: Some(branch_node_0.0)
                },
                KeyVisit {
                    visit_type: KeyVisitType::SeekNonExact(branch_node_2.0),
                    visited_key: Some(branch_node_2.0)
                },
                KeyVisit {
                    visit_type: KeyVisitType::SeekNonExact(Nibbles::from_nibbles([0x1])),
                    visited_key: None
                }
            ]
        );
        pretty_assertions::assert_eq!(
            *hashed_cursor_factory.visited_account_keys(),
            vec![
                // Why do we always seek this key first?
                KeyVisit {
                    visit_type: KeyVisitType::SeekNonExact(account_1),
                    visited_key: Some(account_1)
                },
                // Seek to the modified account.
                KeyVisit {
                    visit_type: KeyVisitType::SeekNonExact(account_3),
                    visited_key: Some(account_3)
                },
                // Collect the siblings of the modified account
                KeyVisit { visit_type: KeyVisitType::Next, visited_key: Some(account_4) },
                KeyVisit { visit_type: KeyVisitType::Next, visited_key: Some(account_5) },
                KeyVisit { visit_type: KeyVisitType::Next, visited_key: None },
            ],
        );
    }
}
