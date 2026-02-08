//! 区块执行相关的 trait 和类型定义。
//!
//! 本模块包含:
//! - [`Executor`] trait: 区块执行的核心抽象，支持单块和批量执行
//! - [`BlockAssembler`] trait: 从执行结果组装完整区块
//! - [`BlockBuilder`] trait: 编排交易执行和区块构建的完整流程
//! - [`BasicBlockExecutor`]: 基于 ConfigureEvm 的通用区块执行器实现
//! - [`BasicBlockBuilder`]: 区块构建器的具体实现
//! - [`BlockAssemblerInput`]: 区块组装所需的输入数据
//! - [`BlockBuilderOutcome`]: 区块构建的输出结果

use crate::{ConfigureEvm, Database, OnStateHook, TxEnvFor};
use alloc::{boxed::Box, sync::Arc, vec::Vec};
use alloy_consensus::{BlockHeader, Header};
use alloy_eips::eip2718::WithEncoded;
pub use alloy_evm::block::{BlockExecutor, BlockExecutorFactory};
use alloy_evm::{
    block::{CommitChanges, ExecutableTxParts},
    Evm, EvmEnv, EvmFactory, RecoveredTx, ToTxEnv,
};
use alloy_primitives::{Address, B256};
pub use reth_execution_errors::{
    BlockExecutionError, BlockValidationError, InternalBlockExecutionError,
};
use reth_execution_types::BlockExecutionResult;
pub use reth_execution_types::{BlockExecutionOutput, ExecutionOutcome};
use reth_primitives_traits::{
    Block, HeaderTy, NodePrimitives, ReceiptTy, Recovered, RecoveredBlock, SealedHeader, TxTy,
};
use reth_storage_api::StateProvider;
pub use reth_storage_errors::provider::ProviderError;
use reth_trie_common::{updates::TrieUpdates, HashedPostState};
use revm::{
    context::result::ExecutionResult,
    database::{states::bundle_state::BundleRetention, BundleState, State},
};

/// 区块执行器 trait —— 知道如何执行一个区块。
///
/// 内部使用 [`crate::Evm`] 执行交易，并使用 [`State`] 作为数据库。
/// 这是执行层最核心的 trait 之一。
///
/// ## 主要方法
/// - `execute_one()`: 执行单个区块，仅返回执行结果（不含状态变更）
/// - `execute()`: 执行区块并返回完整输出（包含状态变更 BundleState）
/// - `execute_batch()`: 批量执行多个区块，返回聚合的 ExecutionOutcome
/// - `into_state()`: 消费执行器，返回包含所有状态变更的 State
///
/// ## 数据流
/// ```text
/// Block → execute_one() → BlockExecutionResult（收据、gas 等）
///       → into_state()  → State（BundleState 状态变更）
///       → execute()     → BlockExecutionOutput（结果 + 状态变更）
/// ```
pub trait Executor<DB: Database>: Sized {
    /// 执行器使用的原语类型（包含 Block、Transaction、Receipt 等类型定义）。
    type Primitives: NodePrimitives;
    /// 执行器返回的错误类型。
    type Error;

    /// 执行单个区块，返回 [`BlockExecutionResult`]（收据、gas 使用量、请求），不包含状态变更。
    /// 状态变更会累积在内部的 State 中，可通过 `into_state()` 获取。
    fn execute_one(
        &mut self,
        block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
    ) -> Result<BlockExecutionResult<<Self::Primitives as NodePrimitives>::Receipt>, Self::Error>;

    /// Executes the EVM with the given input and accepts a state hook closure that is invoked with
    /// the EVM state after execution.
    fn execute_one_with_state_hook<F>(
        &mut self,
        block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
        state_hook: F,
    ) -> Result<BlockExecutionResult<<Self::Primitives as NodePrimitives>::Receipt>, Self::Error>
    where
        F: OnStateHook + 'static;

    /// Consumes the type and executes the block.
    ///
    /// # Note
    /// Execution happens without any validation of the output.
    ///
    /// # Returns
    /// The output of the block execution.
    fn execute(
        mut self,
        block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
    ) -> Result<BlockExecutionOutput<<Self::Primitives as NodePrimitives>::Receipt>, Self::Error>
    {
        let result = self.execute_one(block)?;
        let mut state = self.into_state();
        Ok(BlockExecutionOutput { state: state.take_bundle(), result })
    }

    /// Executes multiple inputs in the batch, and returns an aggregated [`ExecutionOutcome`].
    fn execute_batch<'a, I>(
        mut self,
        blocks: I,
    ) -> Result<ExecutionOutcome<<Self::Primitives as NodePrimitives>::Receipt>, Self::Error>
    where
        I: IntoIterator<Item = &'a RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>>,
    {
        let blocks_iter = blocks.into_iter();
        let capacity = blocks_iter.size_hint().0;
        let mut results = Vec::with_capacity(capacity);
        let mut first_block = None;
        for block in blocks_iter {
            if first_block.is_none() {
                first_block = Some(block.header().number());
            }
            results.push(self.execute_one(block)?);
        }

        Ok(ExecutionOutcome::from_blocks(
            first_block.unwrap_or_default(),
            self.into_state().take_bundle(),
            results,
        ))
    }

    /// Executes the EVM with the given input and accepts a state closure that is invoked with
    /// the EVM state after execution.
    fn execute_with_state_closure<F>(
        mut self,
        block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
        mut f: F,
    ) -> Result<BlockExecutionOutput<<Self::Primitives as NodePrimitives>::Receipt>, Self::Error>
    where
        F: FnMut(&State<DB>),
    {
        let result = self.execute_one(block)?;
        let mut state = self.into_state();
        f(&state);
        Ok(BlockExecutionOutput { state: state.take_bundle(), result })
    }

    /// Executes the EVM with the given input and accepts a state closure that is always invoked
    /// with the EVM state after execution, even after failure.
    fn execute_with_state_closure_always<F>(
        mut self,
        block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
        mut f: F,
    ) -> Result<BlockExecutionOutput<<Self::Primitives as NodePrimitives>::Receipt>, Self::Error>
    where
        F: FnMut(&State<DB>),
    {
        let result = self.execute_one(block);
        let mut state = self.into_state();
        f(&state);

        Ok(BlockExecutionOutput { state: state.take_bundle(), result: result? })
    }

    /// Executes the EVM with the given input and accepts a state hook closure that is invoked with
    /// the EVM state after execution.
    fn execute_with_state_hook<F>(
        mut self,
        block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
        state_hook: F,
    ) -> Result<BlockExecutionOutput<<Self::Primitives as NodePrimitives>::Receipt>, Self::Error>
    where
        F: OnStateHook + 'static,
    {
        let result = self.execute_one_with_state_hook(block, state_hook)?;
        let mut state = self.into_state();
        Ok(BlockExecutionOutput { state: state.take_bundle(), result })
    }

    /// Consumes the executor and returns the [`State`] containing all state changes.
    fn into_state(self) -> State<DB>;

    /// The size hint of the batch's tracked state size.
    ///
    /// This is used to optimize DB commits depending on the size of the state.
    fn size_hint(&self) -> usize;
}

/// Input for block building. Consumed by [`BlockAssembler`].
///
/// This struct contains all the data needed by the [`BlockAssembler`] to create
/// a complete block after transaction execution.
///
/// # Fields Overview
///
/// - `evm_env`: The EVM configuration used during execution (spec ID, block env, etc.)
/// - `execution_ctx`: Additional context like withdrawals and ommers
/// - `parent`: The parent block header this block builds on
/// - `transactions`: All transactions that were successfully executed
/// - `output`: Execution results including receipts and gas used
/// - `bundle_state`: Accumulated state changes from all transactions
/// - `state_provider`: Access to the current state for additional lookups
/// - `state_root`: The calculated state root after all changes
///
/// # Usage
///
/// This is typically created internally by [`BlockBuilder::finish`] after all
/// transactions have been executed:
///
/// ```rust,ignore
/// let input = BlockAssemblerInput {
///     evm_env: builder.evm_env(),
///     execution_ctx: builder.context(),
///     parent: &parent_header,
///     transactions: executed_transactions,
///     output: &execution_result,
///     bundle_state: &state_changes,
///     state_provider: &state,
///     state_root: calculated_root,
/// };
///
/// let block = assembler.assemble_block(input)?;
/// ```
#[derive(derive_more::Debug)]
#[non_exhaustive]
pub struct BlockAssemblerInput<'a, 'b, F: BlockExecutorFactory, H = Header> {
    /// Configuration of EVM used when executing the block.
    ///
    /// Contains context relevant to EVM such as [`revm::context::BlockEnv`].
    pub evm_env:
        EvmEnv<<F::EvmFactory as EvmFactory>::Spec, <F::EvmFactory as EvmFactory>::BlockEnv>,
    /// [`BlockExecutorFactory::ExecutionCtx`] used to execute the block.
    pub execution_ctx: F::ExecutionCtx<'a>,
    /// Parent block header.
    pub parent: &'a SealedHeader<H>,
    /// Transactions that were executed in this block.
    pub transactions: Vec<F::Transaction>,
    /// Output of block execution.
    pub output: &'b BlockExecutionResult<F::Receipt>,
    /// [`BundleState`] after the block execution.
    pub bundle_state: &'a BundleState,
    /// Provider with access to state.
    #[debug(skip)]
    pub state_provider: &'b dyn StateProvider,
    /// State root for this block.
    pub state_root: B256,
}

impl<'a, 'b, F: BlockExecutorFactory, H> BlockAssemblerInput<'a, 'b, F, H> {
    /// Creates a new [`BlockAssemblerInput`].
    #[expect(clippy::too_many_arguments)]
    pub fn new(
        evm_env: EvmEnv<
            <F::EvmFactory as EvmFactory>::Spec,
            <F::EvmFactory as EvmFactory>::BlockEnv,
        >,
        execution_ctx: F::ExecutionCtx<'a>,
        parent: &'a SealedHeader<H>,
        transactions: Vec<F::Transaction>,
        output: &'b BlockExecutionResult<F::Receipt>,
        bundle_state: &'a BundleState,
        state_provider: &'b dyn StateProvider,
        state_root: B256,
    ) -> Self {
        Self {
            evm_env,
            execution_ctx,
            parent,
            transactions,
            output,
            bundle_state,
            state_provider,
            state_root,
        }
    }
}

/// 区块组装器 trait —— 知道如何从执行结果组装出一个完整的区块。
///
/// [`BlockAssembler`] 是区块生产的最后一步。在 [`BlockExecutor`] 执行完所有交易之后，
/// 组装器接收全部执行输出并创建格式正确的区块。
///
/// ## 职责
/// - 设置正确的区块头字段（gas used, receipts root, logs bloom 等）
/// - 按正确顺序包含已执行的交易
/// - 设置执行后的状态根（state root）
/// - 应用链特定规则（以太坊 vs Optimism 等）
///
/// # Responsibilities
///
/// The assembler is responsible for:
/// - Setting the correct block header fields (gas used, receipts root, logs bloom, etc.)
/// - Including the executed transactions in the correct order
/// - Setting the state root from the post-execution state
/// - Applying any chain-specific rules or adjustments
///
/// # Example Flow
///
/// ```rust,ignore
/// // 1. Execute transactions and get results
/// let execution_result = block_executor.finish()?;
///
/// // 2. Calculate state root from changes
/// let state_root = state_provider.state_root(&bundle_state)?;
///
/// // 3. Assemble the final block
/// let block = assembler.assemble_block(BlockAssemblerInput {
///     evm_env,           // Environment used during execution
///     execution_ctx,     // Context like withdrawals, ommers
///     parent,            // Parent block header
///     transactions,      // Executed transactions
///     output,            // Execution results (receipts, gas)
///     bundle_state,      // All state changes
///     state_provider,    // For additional lookups if needed
///     state_root,        // Computed state root
/// })?;
/// ```
///
/// # Relationship with Block Building
///
/// The assembler works together with:
/// - `NextBlockEnvAttributes`: Provides the configuration for the new block
/// - [`BlockExecutor`]: Executes transactions and produces results
/// - [`BlockBuilder`]: Orchestrates the entire process and calls the assembler
#[auto_impl::auto_impl(&, Arc)]
pub trait BlockAssembler<F: BlockExecutorFactory> {
    /// The block type produced by the assembler.
    type Block: Block;

    /// Builds a block. see [`BlockAssemblerInput`] documentation for more details.
    fn assemble_block(
        &self,
        input: BlockAssemblerInput<'_, '_, F, <Self::Block as Block>::Header>,
    ) -> Result<Self::Block, BlockExecutionError>;
}

/// 区块构建的输出结果。
///
/// 包含构建一个新区块后的所有产出:
/// - 执行结果（收据、gas 等）
/// - 哈希状态（用于 trie 更新）
/// - Trie 更新（增量更新数据库中的 Merkle Patricia Trie）
/// - 最终的完整区块
#[derive(Debug, Clone)]
pub struct BlockBuilderOutcome<N: NodePrimitives> {
    /// 区块执行结果（收据列表、总 gas 使用量、EIP-7685 请求）。
    pub execution_result: BlockExecutionResult<N::Receipt>,
    /// 执行后的哈希状态（账户和存储的 keccak256 哈希映射）。
    /// 用于增量计算状态根。
    pub hashed_state: HashedPostState,
    /// 状态根计算过程中收集的 Trie 更新（需要写入数据库的 trie 节点变更）。
    pub trie_updates: TrieUpdates,
    /// 构建出的完整区块（包含区块头、交易列表、发送者列表等）。
    pub block: RecoveredBlock<N::Block>,
}

/// 区块构建器 trait —— 知道如何执行交易并构建区块。
///
/// 包装了一个内部的 [`BlockExecutor`]，提供执行交易和构建区块的方法。
/// 这是构建新区块（payload building）时使用的核心接口。
///
/// ## 使用流程
/// ```text
/// 1. apply_pre_execution_changes()  → 应用执行前变更（如 EIP-4788 Beacon 根更新）
/// 2. execute_transaction(tx)        → 逐个执行交易（可多次调用）
/// 3. finish(state_provider)         → 完成构建，计算状态根，组装区块
/// ```
pub trait BlockBuilder {
    /// 内部 [`BlockExecutor`] 使用的原语类型。
    type Primitives: NodePrimitives;
    /// 内部的区块执行器类型。
    type Executor: BlockExecutor<
        Transaction = TxTy<Self::Primitives>,
        Receipt = ReceiptTy<Self::Primitives>,
    >;

    /// Invokes [`BlockExecutor::apply_pre_execution_changes`].
    fn apply_pre_execution_changes(&mut self) -> Result<(), BlockExecutionError>;

    /// Invokes [`BlockExecutor::execute_transaction_with_commit_condition`] and saves the
    /// transaction in internal state only if the transaction was committed.
    fn execute_transaction_with_commit_condition(
        &mut self,
        tx: impl ExecutorTx<Self::Executor>,
        f: impl FnOnce(
            &ExecutionResult<<<Self::Executor as BlockExecutor>::Evm as Evm>::HaltReason>,
        ) -> CommitChanges,
    ) -> Result<Option<u64>, BlockExecutionError>;

    /// Invokes [`BlockExecutor::execute_transaction_with_result_closure`] and saves the
    /// transaction in internal state.
    fn execute_transaction_with_result_closure(
        &mut self,
        tx: impl ExecutorTx<Self::Executor>,
        f: impl FnOnce(&ExecutionResult<<<Self::Executor as BlockExecutor>::Evm as Evm>::HaltReason>),
    ) -> Result<u64, BlockExecutionError> {
        self.execute_transaction_with_commit_condition(tx, |res| {
            f(res);
            CommitChanges::Yes
        })
        .map(Option::unwrap_or_default)
    }

    /// Invokes [`BlockExecutor::execute_transaction`] and saves the transaction in
    /// internal state.
    fn execute_transaction(
        &mut self,
        tx: impl ExecutorTx<Self::Executor>,
    ) -> Result<u64, BlockExecutionError> {
        self.execute_transaction_with_result_closure(tx, |_| ())
    }

    /// Completes the block building process and returns the [`BlockBuilderOutcome`].
    fn finish(
        self,
        state_provider: impl StateProvider,
    ) -> Result<BlockBuilderOutcome<Self::Primitives>, BlockExecutionError>;

    /// Provides mutable access to the inner [`BlockExecutor`].
    fn executor_mut(&mut self) -> &mut Self::Executor;

    /// Provides access to the inner [`BlockExecutor`].
    fn executor(&self) -> &Self::Executor;

    /// Helper to access inner [`BlockExecutor::Evm`] mutably.
    fn evm_mut(&mut self) -> &mut <Self::Executor as BlockExecutor>::Evm {
        self.executor_mut().evm_mut()
    }

    /// Helper to access inner [`BlockExecutor::Evm`].
    fn evm(&self) -> &<Self::Executor as BlockExecutor>::Evm {
        self.executor().evm()
    }

    /// Consumes the type and returns the underlying [`BlockExecutor`].
    fn into_executor(self) -> Self::Executor;
}

/// A type that constructs a block from transactions and execution results.
#[derive(Debug)]
pub struct BasicBlockBuilder<'a, F, Executor, Builder, N: NodePrimitives>
where
    F: BlockExecutorFactory,
{
    /// The block executor used to execute transactions.
    pub executor: Executor,
    /// The transactions executed in this block.
    pub transactions: Vec<Recovered<TxTy<N>>>,
    /// The parent block execution context.
    pub ctx: F::ExecutionCtx<'a>,
    /// The sealed parent block header.
    pub parent: &'a SealedHeader<HeaderTy<N>>,
    /// The assembler used to build the block.
    pub assembler: Builder,
}

/// Conversions for executable transactions.
pub trait ExecutorTx<Executor: BlockExecutor> {
    /// Converts the transaction into a tuple of [`TxEnvFor`] and [`Recovered`].
    fn into_parts(self) -> (<Executor::Evm as Evm>::Tx, Recovered<Executor::Transaction>);
}

impl<Executor: BlockExecutor> ExecutorTx<Executor>
    for WithEncoded<Recovered<Executor::Transaction>>
{
    fn into_parts(self) -> (<Executor::Evm as Evm>::Tx, Recovered<Executor::Transaction>) {
        (self.to_tx_env(), self.1)
    }
}

impl<Executor: BlockExecutor> ExecutorTx<Executor> for Recovered<Executor::Transaction> {
    fn into_parts(self) -> (<Executor::Evm as Evm>::Tx, Self) {
        (self.to_tx_env(), self)
    }
}

impl<Executor> ExecutorTx<Executor>
    for WithTxEnv<<Executor::Evm as Evm>::Tx, Recovered<Executor::Transaction>>
where
    Executor: BlockExecutor<Transaction: Clone>,
{
    fn into_parts(self) -> (<Executor::Evm as Evm>::Tx, Recovered<Executor::Transaction>) {
        (self.tx_env, Arc::unwrap_or_clone(self.tx))
    }
}

impl<'a, F, DB, Executor, Builder, N> BlockBuilder
    for BasicBlockBuilder<'a, F, Executor, Builder, N>
where
    F: BlockExecutorFactory<Transaction = N::SignedTx, Receipt = N::Receipt>,
    Executor: BlockExecutor<
        Evm: Evm<
            Spec = <F::EvmFactory as EvmFactory>::Spec,
            HaltReason = <F::EvmFactory as EvmFactory>::HaltReason,
            BlockEnv = <F::EvmFactory as EvmFactory>::BlockEnv,
            DB = &'a mut State<DB>,
        >,
        Transaction = N::SignedTx,
        Receipt = N::Receipt,
    >,
    DB: Database + 'a,
    Builder: BlockAssembler<F, Block = N::Block>,
    N: NodePrimitives,
{
    type Primitives = N;
    type Executor = Executor;

    fn apply_pre_execution_changes(&mut self) -> Result<(), BlockExecutionError> {
        self.executor.apply_pre_execution_changes()
    }

    fn execute_transaction_with_commit_condition(
        &mut self,
        tx: impl ExecutorTx<Self::Executor>,
        f: impl FnOnce(
            &ExecutionResult<<<Self::Executor as BlockExecutor>::Evm as Evm>::HaltReason>,
        ) -> CommitChanges,
    ) -> Result<Option<u64>, BlockExecutionError> {
        let (tx_env, tx) = tx.into_parts();
        if let Some(gas_used) =
            self.executor.execute_transaction_with_commit_condition((tx_env, &tx), f)?
        {
            self.transactions.push(tx);
            Ok(Some(gas_used))
        } else {
            Ok(None)
        }
    }

    fn finish(
        self,
        state: impl StateProvider,
    ) -> Result<BlockBuilderOutcome<N>, BlockExecutionError> {
        let (evm, result) = self.executor.finish()?;
        let (db, evm_env) = evm.finish();

        // merge all transitions into bundle state
        db.merge_transitions(BundleRetention::Reverts);

        // calculate the state root
        let hashed_state = state.hashed_post_state(&db.bundle_state);
        let (state_root, trie_updates) = state
            .state_root_with_updates(hashed_state.clone())
            .map_err(BlockExecutionError::other)?;

        let (transactions, senders) =
            self.transactions.into_iter().map(|tx| tx.into_parts()).unzip();

        let block = self.assembler.assemble_block(BlockAssemblerInput {
            evm_env,
            execution_ctx: self.ctx,
            parent: self.parent,
            transactions,
            output: &result,
            bundle_state: &db.bundle_state,
            state_provider: &state,
            state_root,
        })?;

        let block = RecoveredBlock::new_unhashed(block, senders);

        Ok(BlockBuilderOutcome { execution_result: result, hashed_state, trie_updates, block })
    }

    fn executor_mut(&mut self) -> &mut Self::Executor {
        &mut self.executor
    }

    fn executor(&self) -> &Self::Executor {
        &self.executor
    }

    fn into_executor(self) -> Self::Executor {
        self.executor
    }
}

/// 通用区块执行器 —— 使用 [`ConfigureEvm`] 来执行区块。
///
/// 这是 `Executor` trait 的主要具体实现。内部持有:
/// - `strategy_factory`: EVM 配置（实现了 ConfigureEvm），用于为每个区块创建执行策略
/// - `db`: revm 的 State 数据库，带有 bundle 更新跟踪，用于累积多个区块的状态变更
///
/// ## 执行流程
/// 1. 对每个区块调用 `execute_one()`
/// 2. 通过 strategy_factory 创建该区块的执行器
/// 3. 执行区块中的所有交易
/// 4. 合并状态转换到 bundle state
/// 5. 最终通过 `into_state()` 获取完整的状态变更
#[expect(missing_debug_implementations)]
pub struct BasicBlockExecutor<F, DB> {
    /// 区块执行策略工厂（即 ConfigureEvm 实现），用于为每个区块创建 EVM 环境和执行器。
    pub(crate) strategy_factory: F,
    /// revm 的 State 数据库包装，跟踪所有状态变更（账户余额、nonce、存储槽等）。
    /// 启用了 bundle_update，可以累积多个区块的变更。
    pub(crate) db: State<DB>,
}

impl<F, DB: Database> BasicBlockExecutor<F, DB> {
    /// Creates a new `BasicBlockExecutor` with the given strategy.
    pub fn new(strategy_factory: F, db: DB) -> Self {
        let db =
            State::builder().with_database(db).with_bundle_update().without_state_clear().build();
        Self { strategy_factory, db }
    }
}

impl<F, DB> Executor<DB> for BasicBlockExecutor<F, DB>
where
    F: ConfigureEvm,
    DB: Database,
{
    type Primitives = F::Primitives;
    type Error = BlockExecutionError;

    fn execute_one(
        &mut self,
        block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
    ) -> Result<BlockExecutionResult<<Self::Primitives as NodePrimitives>::Receipt>, Self::Error>
    {
        let result = self
            .strategy_factory
            .executor_for_block(&mut self.db, block)
            .map_err(BlockExecutionError::other)?
            .execute_block(block.transactions_recovered())?;

        self.db.merge_transitions(BundleRetention::Reverts);

        Ok(result)
    }

    fn execute_one_with_state_hook<H>(
        &mut self,
        block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
        state_hook: H,
    ) -> Result<BlockExecutionResult<<Self::Primitives as NodePrimitives>::Receipt>, Self::Error>
    where
        H: OnStateHook + 'static,
    {
        let result = self
            .strategy_factory
            .executor_for_block(&mut self.db, block)
            .map_err(BlockExecutionError::other)?
            .with_state_hook(Some(Box::new(state_hook)))
            .execute_block(block.transactions_recovered())?;

        self.db.merge_transitions(BundleRetention::Reverts);

        Ok(result)
    }

    fn into_state(self) -> State<DB> {
        self.db
    }

    fn size_hint(&self) -> usize {
        self.db.bundle_state.size_hint()
    }
}

/// A helper trait marking a 'static type that can be converted into an [`ExecutableTxParts`] for
/// block executor.
pub trait ExecutableTxFor<Evm: ConfigureEvm>:
    ExecutableTxParts<TxEnvFor<Evm>, TxTy<Evm::Primitives>> + RecoveredTx<TxTy<Evm::Primitives>>
{
}

impl<T, Evm: ConfigureEvm> ExecutableTxFor<Evm> for T where
    T: ExecutableTxParts<TxEnvFor<Evm>, TxTy<Evm::Primitives>> + RecoveredTx<TxTy<Evm::Primitives>>
{
}

/// A container for a transaction and a transaction environment.
#[derive(Debug)]
pub struct WithTxEnv<TxEnv, T> {
    /// The transaction environment for EVM.
    pub tx_env: TxEnv,
    /// The recovered transaction.
    pub tx: Arc<T>,
}

impl<TxEnv: Clone, T> Clone for WithTxEnv<TxEnv, T> {
    fn clone(&self) -> Self {
        Self { tx_env: self.tx_env.clone(), tx: self.tx.clone() }
    }
}

impl<TxEnv, Tx, T: RecoveredTx<Tx>> RecoveredTx<Tx> for WithTxEnv<TxEnv, T> {
    fn tx(&self) -> &Tx {
        self.tx.tx()
    }

    fn signer(&self) -> &Address {
        self.tx.signer()
    }
}

impl<TxEnv, T: RecoveredTx<Tx>, Tx> ExecutableTxParts<TxEnv, Tx> for WithTxEnv<TxEnv, T> {
    type Recovered = Arc<T>;

    fn into_parts(self) -> (TxEnv, Self::Recovered) {
        (self.tx_env, self.tx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Address;
    use alloy_consensus::constants::KECCAK_EMPTY;
    use alloy_evm::block::state_changes::balance_increment_state;
    use alloy_primitives::{address, map::HashMap, U256};
    use core::marker::PhantomData;
    use reth_ethereum_primitives::EthPrimitives;
    use revm::{
        database::{CacheDB, EmptyDB},
        state::AccountInfo,
    };

    #[derive(Clone, Debug, Default)]
    struct TestExecutorProvider;

    impl TestExecutorProvider {
        fn executor<DB>(&self, _db: DB) -> TestExecutor<DB>
        where
            DB: Database,
        {
            TestExecutor(PhantomData)
        }
    }

    struct TestExecutor<DB>(PhantomData<DB>);

    impl<DB: Database> Executor<DB> for TestExecutor<DB> {
        type Primitives = EthPrimitives;
        type Error = BlockExecutionError;

        fn execute_one(
            &mut self,
            _block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
        ) -> Result<BlockExecutionResult<<Self::Primitives as NodePrimitives>::Receipt>, Self::Error>
        {
            Err(BlockExecutionError::msg("execution unavailable for tests"))
        }

        fn execute_one_with_state_hook<F>(
            &mut self,
            _block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
            _state_hook: F,
        ) -> Result<BlockExecutionResult<<Self::Primitives as NodePrimitives>::Receipt>, Self::Error>
        where
            F: OnStateHook + 'static,
        {
            Err(BlockExecutionError::msg("execution unavailable for tests"))
        }

        fn into_state(self) -> State<DB> {
            unreachable!()
        }

        fn size_hint(&self) -> usize {
            0
        }
    }

    #[test]
    fn test_provider() {
        let provider = TestExecutorProvider;
        let db = CacheDB::<EmptyDB>::default();
        let executor = provider.executor(db);
        let _ = executor.execute(&Default::default());
    }

    fn setup_state_with_account(
        addr: Address,
        balance: u128,
        nonce: u64,
    ) -> State<CacheDB<EmptyDB>> {
        let db = CacheDB::<EmptyDB>::default();
        let mut state = State::builder().with_database(db).with_bundle_update().build();

        let account_info = AccountInfo {
            balance: U256::from(balance),
            nonce,
            code_hash: KECCAK_EMPTY,
            code: None,
            account_id: None,
        };
        state.insert_account(addr, account_info);
        state
    }

    #[test]
    fn test_balance_increment_state_zero() {
        let addr = address!("0x1000000000000000000000000000000000000000");
        let mut state = setup_state_with_account(addr, 100, 1);

        let mut increments = HashMap::default();
        increments.insert(addr, 0);

        let result = balance_increment_state(&increments, &mut state).unwrap();
        assert!(result.is_empty(), "Zero increments should be ignored");
    }

    #[test]
    fn test_balance_increment_state_empty_increments_map() {
        let mut state = State::builder()
            .with_database(CacheDB::<EmptyDB>::default())
            .with_bundle_update()
            .build();

        let increments = HashMap::default();
        let result = balance_increment_state(&increments, &mut state).unwrap();
        assert!(result.is_empty(), "Empty increments map should return empty state");
    }

    #[test]
    fn test_balance_increment_state_multiple_valid_increments() {
        let addr1 = address!("0x1000000000000000000000000000000000000000");
        let addr2 = address!("0x2000000000000000000000000000000000000000");

        let mut state = setup_state_with_account(addr1, 100, 1);

        let account2 = AccountInfo {
            balance: U256::from(200),
            nonce: 1,
            code_hash: KECCAK_EMPTY,
            code: None,
            account_id: None,
        };
        state.insert_account(addr2, account2);

        let mut increments = HashMap::default();
        increments.insert(addr1, 50);
        increments.insert(addr2, 100);

        let result = balance_increment_state(&increments, &mut state).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result.get(&addr1).unwrap().info.balance, U256::from(100));
        assert_eq!(result.get(&addr2).unwrap().info.balance, U256::from(200));
    }

    #[test]
    fn test_balance_increment_state_mixed_zero_and_nonzero_increments() {
        let addr1 = address!("0x1000000000000000000000000000000000000000");
        let addr2 = address!("0x2000000000000000000000000000000000000000");

        let mut state = setup_state_with_account(addr1, 100, 1);

        let account2 = AccountInfo {
            balance: U256::from(200),
            nonce: 1,
            code_hash: KECCAK_EMPTY,
            code: None,
            account_id: None,
        };
        state.insert_account(addr2, account2);

        let mut increments = HashMap::default();
        increments.insert(addr1, 0);
        increments.insert(addr2, 100);

        let result = balance_increment_state(&increments, &mut state).unwrap();

        assert_eq!(result.len(), 1, "Only non-zero increments should be included");
        assert!(!result.contains_key(&addr1), "Zero increment account should not be included");
        assert_eq!(result.get(&addr2).unwrap().info.balance, U256::from(200));
    }
}
