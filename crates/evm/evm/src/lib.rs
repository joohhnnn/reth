//! EVM 配置相关的 trait 定义。
//!
//! 本模块定义了 reth 中 EVM（以太坊虚拟机）执行的核心抽象层。
//! 主要包含:
//! - [`ConfigureEvm`] trait: EVM 配置的核心入口，负责创建 EVM 环境、区块执行器和区块组装器
//! - [`TransactionEnv`] trait: 交易环境的抽象，允许设置 gas 限制、nonce、访问列表等
//! - [`NextBlockEnvAttributes`]: 下一个区块的环境属性（时间戳、手续费接收者、随机数等）
//!
//! ## 架构概览
//!
//! EVM 抽象分为三层:
//! 1. **Evm（由 EvmFactory 创建）**: 核心 EVM 实现，负责执行单个交易
//! 2. **BlockExecutor（由 BlockExecutorFactory 创建）**: 在 Evm 之上执行整个区块
//! 3. **BlockAssembler**: 从执行结果组装出完整的区块
//!
//! ## 两种使用模式
//!
//! 1. **执行已有区块**（同步时）: `executor.execute(&block)`
//! 2. **构建新区块**（出块时）: `builder_for_next_block()` → 执行交易 → `finish()`
//!
//! # Revm features
//!
//! This crate does __not__ enforce specific revm features such as `blst` or `c-kzg`, which are
//! critical for revm's evm internals, it is the responsibility of the implementer to ensure the
//! proper features are selected.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/reth/main/assets/reth-docs.png",
    html_favicon_url = "https://avatars0.githubusercontent.com/u/97369466?s=256",
    issue_tracker_base_url = "https://github.com/paradigmxyz/reth/issues/"
)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use crate::execute::{BasicBlockBuilder, Executor};
use alloc::vec::Vec;
use alloy_eips::{
    eip2718::{EIP2930_TX_TYPE_ID, LEGACY_TX_TYPE_ID},
    eip2930::AccessList,
    eip4895::Withdrawals,
};
use alloy_evm::{
    block::{BlockExecutorFactory, BlockExecutorFor},
    precompiles::PrecompilesMap,
};
use alloy_primitives::{Address, Bytes, B256};
use core::{error::Error, fmt::Debug};
use execute::{BasicBlockExecutor, BlockAssembler, BlockBuilder};
use reth_execution_errors::BlockExecutionError;
use reth_primitives_traits::{
    BlockTy, HeaderTy, NodePrimitives, ReceiptTy, SealedBlock, SealedHeader, TxTy,
};
use revm::{context::TxEnv, database::State, primitives::hardfork::SpecId};

pub mod either;
/// EVM environment configuration.
pub mod execute;

mod aliases;
pub use aliases::*;

#[cfg(feature = "std")]
mod engine;
#[cfg(feature = "std")]
pub use engine::{ConfigureEngineEvm, ExecutableTxIterator, ExecutableTxTuple};

#[cfg(feature = "metrics")]
pub mod metrics;
pub mod noop;
#[cfg(any(test, feature = "test-utils"))]
/// test helpers for mocking executor
pub mod test_utils;

pub use alloy_evm::{
    block::{state_changes, system_calls, OnStateHook},
    *,
};

/// Reth 的完整 EVM 配置 trait。
///
/// 这是 EVM 执行的核心 trait，封装了交易执行和区块执行/构建所需的全部配置，
/// 提供了统一的 EVM 操作接口。
///
/// ## 关联类型说明
/// - `Primitives`: 链特定的原语类型（如 EthPrimitives 用于以太坊主网）
/// - `Error`: EVM 操作返回的错误类型
/// - `NextBlockEnvCtx`: 配置下一个区块环境所需的上下文（CL 提供的属性）
/// - `BlockExecutorFactory`: 区块执行器工厂，内部包含 EvmFactory
/// - `BlockAssembler`: 区块组装器，将执行结果组装成完整区块
///
/// ## 核心方法
/// - `evm_env()`: 从区块头创建 EVM 环境
/// - `next_evm_env()`: 为下一个区块创建 EVM 环境（需要 CL 属性）
/// - `executor()`: 创建区块执行器（用于执行已有区块）
/// - `builder_for_next_block()`: 创建区块构建器（用于出块）
///
/// This trait encapsulates complete configuration required for transaction execution and block
/// execution/building, providing a unified interface for EVM operations.
///
/// # Architecture Overview
///
/// The EVM abstraction consists of the following layers:
///
/// 1. **[`Evm`] (produced by [`EvmFactory`])**: The core EVM implementation responsible for
///    executing individual transactions and producing outputs including state changes, logs, gas
///    usage, etc.
///
/// 2. **[`BlockExecutor`] (produced by [`BlockExecutorFactory`])**: A higher-level component that
///    operates on top of [`Evm`] to execute entire blocks. This involves:
///    - Executing all transactions in sequence
///    - Building receipts from transaction outputs
///    - Applying block rewards to the beneficiary
///    - Executing system calls (e.g., EIP-4788 beacon root updates)
///    - Managing state changes and bundle accumulation
///
/// 3. **[`BlockAssembler`]**: Responsible for assembling valid blocks from executed transactions.
///    It takes the output from [`BlockExecutor`] along with execution context and produces a
///    complete block ready for inclusion in the chain.
///
/// # Usage Patterns
///
/// The abstraction supports two primary use cases:
///
/// ## 1. Executing Externally Provided Blocks (e.g., during sync)
///
/// ```rust,ignore
/// use reth_evm::ConfigureEvm;
///
/// // Execute a received block
/// let mut executor = evm_config.executor(state_db);
/// let output = executor.execute(&block)?;
///
/// // Access the execution results
/// println!("Gas used: {}", output.result.gas_used);
/// println!("Receipts: {:?}", output.result.receipts);
/// ```
///
/// ## 2. Building New Blocks (e.g., payload building)
///
/// Payload building is slightly different as it doesn't have the block's header yet, but rather
/// attributes for the block's environment, such as timestamp, fee recipient, and randomness value.
/// The block's header will be the outcome of the block building process.
///
/// ```rust,ignore
/// use reth_evm::{ConfigureEvm, NextBlockEnvAttributes};
///
/// // Create attributes for the next block
/// let attributes = NextBlockEnvAttributes {
///     timestamp: current_time + 12,
///     suggested_fee_recipient: beneficiary_address,
///     prev_randao: randomness_value,
///     gas_limit: 30_000_000,
///     withdrawals: Some(withdrawals),
///     parent_beacon_block_root: Some(beacon_root),
/// };
///
/// // Build a new block on top of parent
/// let mut builder = evm_config.builder_for_next_block(
///     &mut state_db,
///     &parent_header,
///     attributes
/// )?;
///
/// // Apply pre-execution changes (e.g., beacon root update)
/// builder.apply_pre_execution_changes()?;
///
/// // Execute transactions
/// for tx in pending_transactions {
///     match builder.execute_transaction(tx) {
///         Ok(gas_used) => {
///             println!("Transaction executed, gas used: {}", gas_used);
///         }
///         Err(e) => {
///             println!("Transaction failed: {:?}", e);
///         }
///     }
/// }
///
/// // Finish block building and get the outcome (block)
/// let outcome = builder.finish(state_provider)?;
/// let block = outcome.block;
/// ```
///
/// # Key Components
///
/// ## [`NextBlockEnvCtx`]
///
/// Contains attributes needed to configure the next block that cannot be derived from the
/// parent block alone. This includes data typically provided by the consensus layer:
/// - `timestamp`: Block timestamp
/// - `suggested_fee_recipient`: Beneficiary address
/// - `prev_randao`: Randomness value
/// - `gas_limit`: Block gas limit
/// - `withdrawals`: Consensus layer withdrawals
/// - `parent_beacon_block_root`: EIP-4788 beacon root
///
/// ## [`BlockAssembler`]
///
/// Takes the execution output and produces a complete block. It receives:
/// - Transaction execution results (receipts, gas used)
/// - Final state root after all executions
/// - Bundle state with all changes
/// - Execution context and environment
///
/// The assembler is responsible for:
/// - Setting the correct block header fields
/// - Including executed transactions
/// - Setting gas used and receipts root
/// - Applying any chain-specific rules
///
/// [`ExecutionCtx`]: BlockExecutorFactory::ExecutionCtx
/// [`NextBlockEnvCtx`]: ConfigureEvm::NextBlockEnvCtx
/// [`BlockExecutor`]: alloy_evm::block::BlockExecutor
#[auto_impl::auto_impl(&, Arc)]
pub trait ConfigureEvm: Clone + Debug + Send + Sync + Unpin {
    /// EVM 使用的原语类型（区块、交易、收据等链特定类型）。
    /// 例如以太坊主网使用 EthPrimitives，Optimism 使用 OpPrimitives。
    type Primitives: NodePrimitives;

    /// [`Self::next_evm_env`] 返回的错误类型。
    type Error: Error + Send + Sync + 'static;

    /// 配置下一个区块环境所需的上下文。
    /// 包含无法从父区块派生的值（由共识层提供），如时间戳、随机数等。
    type NextBlockEnvCtx: Debug + Clone;

    /// 已配置的区块执行器工厂，内部包含 [`EvmFactory`]。
    /// 负责创建 BlockExecutor，后者负责执行整个区块的交易。
    type BlockExecutorFactory: for<'a> BlockExecutorFactory<
        Transaction = TxTy<Self::Primitives>,
        Receipt = ReceiptTy<Self::Primitives>,
        ExecutionCtx<'a>: Debug + Send,
        EvmFactory: EvmFactory<
            Tx: TransactionEnv
                    + FromRecoveredTx<TxTy<Self::Primitives>>
                    + FromTxWithEncoded<TxTy<Self::Primitives>>,
            Precompiles = PrecompilesMap,
            Spec: Into<SpecId>,
        >,
    >;

    /// 区块组装器类型。负责将执行结果（收据、gas 使用量等）组装成完整的区块。
    type BlockAssembler: BlockAssembler<
        Self::BlockExecutorFactory,
        Block = BlockTy<Self::Primitives>,
    >;

    /// Returns reference to the configured [`BlockExecutorFactory`].
    fn block_executor_factory(&self) -> &Self::BlockExecutorFactory;

    /// Returns reference to the configured [`BlockAssembler`].
    fn block_assembler(&self) -> &Self::BlockAssembler;

    /// Creates a new [`EvmEnv`] for the given header.
    fn evm_env(&self, header: &HeaderTy<Self::Primitives>) -> Result<EvmEnvFor<Self>, Self::Error>;

    /// Returns the configured [`EvmEnv`] for `parent + 1` block.
    ///
    /// This is intended for usage in block building after the merge and requires additional
    /// attributes that can't be derived from the parent block: attributes that are determined by
    /// the CL, such as the timestamp, suggested fee recipient, and randomness value.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let evm_env = evm_config.next_evm_env(&parent_header, &attributes)?;
    /// // evm_env now contains:
    /// // - Correct spec ID based on timestamp and block number
    /// // - Block environment with next block's parameters
    /// // - Configuration like chain ID and blob parameters
    /// ```
    fn next_evm_env(
        &self,
        parent: &HeaderTy<Self::Primitives>,
        attributes: &Self::NextBlockEnvCtx,
    ) -> Result<EvmEnvFor<Self>, Self::Error>;

    /// Returns the configured [`BlockExecutorFactory::ExecutionCtx`] for a given block.
    fn context_for_block<'a>(
        &self,
        block: &'a SealedBlock<BlockTy<Self::Primitives>>,
    ) -> Result<ExecutionCtxFor<'a, Self>, Self::Error>;

    /// Returns the configured [`BlockExecutorFactory::ExecutionCtx`] for `parent + 1`
    /// block.
    fn context_for_next_block(
        &self,
        parent: &SealedHeader<HeaderTy<Self::Primitives>>,
        attributes: Self::NextBlockEnvCtx,
    ) -> Result<ExecutionCtxFor<'_, Self>, Self::Error>;

    /// Returns a [`TxEnv`] from a transaction.
    fn tx_env(&self, transaction: impl IntoTxEnv<TxEnvFor<Self>>) -> TxEnvFor<Self> {
        transaction.into_tx_env()
    }

    /// Provides a reference to [`EvmFactory`] implementation.
    fn evm_factory(&self) -> &EvmFactoryFor<Self> {
        self.block_executor_factory().evm_factory()
    }

    /// Returns a new EVM with the given database configured with the given environment settings,
    /// including the spec id and transaction environment.
    ///
    /// This will preserve any handler modifications
    fn evm_with_env<DB: Database>(&self, db: DB, evm_env: EvmEnvFor<Self>) -> EvmFor<Self, DB> {
        self.evm_factory().create_evm(db, evm_env)
    }

    /// Returns a new EVM with the given database configured with `cfg` and `block_env`
    /// configuration derived from the given header. Relies on
    /// [`ConfigureEvm::evm_env`].
    ///
    /// # Caution
    ///
    /// This does not initialize the tx environment.
    fn evm_for_block<DB: Database>(
        &self,
        db: DB,
        header: &HeaderTy<Self::Primitives>,
    ) -> Result<EvmFor<Self, DB>, Self::Error> {
        let evm_env = self.evm_env(header)?;
        Ok(self.evm_with_env(db, evm_env))
    }

    /// Returns a new EVM with the given database configured with the given environment settings,
    /// including the spec id.
    ///
    /// This will use the given external inspector as the EVM external context.
    ///
    /// This will preserve any handler modifications
    fn evm_with_env_and_inspector<DB, I>(
        &self,
        db: DB,
        evm_env: EvmEnvFor<Self>,
        inspector: I,
    ) -> EvmFor<Self, DB, I>
    where
        DB: Database,
        I: InspectorFor<Self, DB>,
    {
        self.evm_factory().create_evm_with_inspector(db, evm_env, inspector)
    }

    /// Creates a strategy with given EVM and execution context.
    fn create_executor<'a, DB, I>(
        &'a self,
        evm: EvmFor<Self, &'a mut State<DB>, I>,
        ctx: <Self::BlockExecutorFactory as BlockExecutorFactory>::ExecutionCtx<'a>,
    ) -> impl BlockExecutorFor<'a, Self::BlockExecutorFactory, DB, I>
    where
        DB: Database,
        I: InspectorFor<Self, &'a mut State<DB>> + 'a,
    {
        self.block_executor_factory().create_executor(evm, ctx)
    }

    /// Creates a strategy for execution of a given block.
    fn executor_for_block<'a, DB: Database>(
        &'a self,
        db: &'a mut State<DB>,
        block: &'a SealedBlock<<Self::Primitives as NodePrimitives>::Block>,
    ) -> Result<impl BlockExecutorFor<'a, Self::BlockExecutorFactory, DB>, Self::Error> {
        let evm = self.evm_for_block(db, block.header())?;
        let ctx = self.context_for_block(block)?;
        Ok(self.create_executor(evm, ctx))
    }

    /// Creates a [`BlockBuilder`]. Should be used when building a new block.
    ///
    /// Block builder wraps an inner [`alloy_evm::block::BlockExecutor`] and has a similar
    /// interface. Builder collects all of the executed transactions, and once
    /// [`BlockBuilder::finish`] is called, it invokes the configured [`BlockAssembler`] to
    /// create a block.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Create a builder with specific EVM configuration
    /// let evm = evm_config.evm_with_env(&mut state_db, evm_env);
    /// let ctx = evm_config.context_for_next_block(&parent, attributes);
    /// let builder = evm_config.create_block_builder(evm, &parent, ctx);
    /// ```
    fn create_block_builder<'a, DB, I>(
        &'a self,
        evm: EvmFor<Self, &'a mut State<DB>, I>,
        parent: &'a SealedHeader<HeaderTy<Self::Primitives>>,
        ctx: <Self::BlockExecutorFactory as BlockExecutorFactory>::ExecutionCtx<'a>,
    ) -> impl BlockBuilder<
        Primitives = Self::Primitives,
        Executor: BlockExecutorFor<'a, Self::BlockExecutorFactory, DB, I>,
    >
    where
        DB: Database,
        I: InspectorFor<Self, &'a mut State<DB>> + 'a,
    {
        BasicBlockBuilder {
            executor: self.create_executor(evm, ctx.clone()),
            ctx,
            assembler: self.block_assembler(),
            parent,
            transactions: Vec::new(),
        }
    }

    /// Creates a [`BlockBuilder`] for building of a new block. This is a helper to invoke
    /// [`ConfigureEvm::create_block_builder`].
    ///
    /// This is the primary method for building new blocks. It combines:
    /// 1. Creating the EVM environment for the next block
    /// 2. Setting up the execution context from attributes
    /// 3. Initializing the block builder with proper configuration
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Build a block with specific attributes
    /// let mut builder = evm_config.builder_for_next_block(
    ///     &mut state_db,
    ///     &parent_header,
    ///     attributes
    /// )?;
    ///
    /// // Execute system calls (e.g., beacon root update)
    /// builder.apply_pre_execution_changes()?;
    ///
    /// // Execute transactions
    /// for tx in transactions {
    ///     builder.execute_transaction(tx)?;
    /// }
    ///
    /// // Complete block building
    /// let outcome = builder.finish(state_provider)?;
    /// ```
    fn builder_for_next_block<'a, DB: Database + 'a>(
        &'a self,
        db: &'a mut State<DB>,
        parent: &'a SealedHeader<<Self::Primitives as NodePrimitives>::BlockHeader>,
        attributes: Self::NextBlockEnvCtx,
    ) -> Result<
        impl BlockBuilder<
            Primitives = Self::Primitives,
            Executor: BlockExecutorFor<'a, Self::BlockExecutorFactory, DB>,
        >,
        Self::Error,
    > {
        let evm_env = self.next_evm_env(parent, &attributes)?;
        let evm = self.evm_with_env(db, evm_env);
        let ctx = self.context_for_next_block(parent, attributes)?;
        Ok(self.create_block_builder(evm, parent, ctx))
    }

    /// Returns a new [`Executor`] for executing blocks.
    ///
    /// The executor processes complete blocks including:
    /// - All transactions in order
    /// - Block rewards and fees
    /// - Block level system calls
    /// - State transitions
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Create an executor
    /// let mut executor = evm_config.executor(state_db);
    ///
    /// // Execute a single block
    /// let output = executor.execute(&block)?;
    ///
    /// // Execute multiple blocks
    /// let batch_output = executor.execute_batch(&blocks)?;
    /// ```
    #[auto_impl(keep_default_for(&, Arc))]
    fn executor<DB: Database>(
        &self,
        db: DB,
    ) -> impl Executor<DB, Primitives = Self::Primitives, Error = BlockExecutionError> {
        BasicBlockExecutor::new(self, db)
    }

    /// Returns a new [`BasicBlockExecutor`].
    #[auto_impl(keep_default_for(&, Arc))]
    fn batch_executor<DB: Database>(
        &self,
        db: DB,
    ) -> impl Executor<DB, Primitives = Self::Primitives, Error = BlockExecutionError> {
        BasicBlockExecutor::new(self, db)
    }
}

/// Represents additional attributes required to configure the next block.
///
/// This struct contains all the information needed to build a new block that cannot be
/// derived from the parent block header alone. These attributes are typically provided
/// by the consensus layer (CL) through the Engine API during payload building.
///
/// # Relationship with [`ConfigureEvm`] and [`BlockAssembler`]
///
/// The flow for building a new block involves:
///
/// 1. **Receive attributes** from the consensus layer containing:
///    - Timestamp for the new block
///    - Fee recipient (coinbase/beneficiary)
///    - Randomness value (prevRandao)
///    - Withdrawals to process
///    - Parent beacon block root for EIP-4788
///
/// 2. **Configure EVM environment** using these attributes: ```rust,ignore let evm_env =
///    evm_config.next_evm_env(&parent, &attributes)?; ```
///
/// 3. **Build the block** with transactions: ```rust,ignore let mut builder =
///    evm_config.builder_for_next_block( &mut state, &parent, attributes )?; ```
///
/// 4. **Assemble the final block** using [`BlockAssembler`] which takes:
///    - Execution results from all transactions
///    - The attributes used during execution
///    - Final state root after all changes
///
/// This design cleanly separates:
/// - **Configuration** (what parameters to use) - handled by `NextBlockEnvAttributes`
/// - **Execution** (running transactions) - handled by `BlockExecutor`
/// - **Assembly** (creating the final block) - handled by `BlockAssembler`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextBlockEnvAttributes {
    /// 下一个区块的时间戳（由共识层提供）。
    pub timestamp: u64,
    /// 建议的手续费接收者地址（coinbase/beneficiary），即矿工/验证者地址。
    pub suggested_fee_recipient: Address,
    /// 随机数值（PoS 后由 Beacon 链的 RANDAO 提供，替代了 PoW 的 difficulty）。
    pub prev_randao: B256,
    /// 区块 gas 上限。
    pub gas_limit: u64,
    /// 父 Beacon 区块根（EIP-4788），用于在 EVM 中访问 Beacon 链状态。
    pub parent_beacon_block_root: Option<B256>,
    /// 共识层提款列表（EIP-4895，上海升级引入）。
    pub withdrawals: Option<Withdrawals>,
    /// 可选的额外数据字段。
    pub extra_data: Bytes,
}

/// 交易环境的抽象 trait。
///
/// 提供对交易执行环境的统一操作接口，包括设置 gas 限制、nonce 和访问列表。
/// 主要实现者是 revm 的 `TxEnv` 和 Optimism 的 `OpTransaction`。
pub trait TransactionEnv:
    revm::context_interface::Transaction + Debug + Clone + Send + Sync + 'static
{
    /// Set the gas limit.
    fn set_gas_limit(&mut self, gas_limit: u64);

    /// Set the gas limit.
    fn with_gas_limit(mut self, gas_limit: u64) -> Self {
        self.set_gas_limit(gas_limit);
        self
    }

    /// Returns the configured nonce.
    fn nonce(&self) -> u64;

    /// Sets the nonce.
    fn set_nonce(&mut self, nonce: u64);

    /// Sets the nonce.
    fn with_nonce(mut self, nonce: u64) -> Self {
        self.set_nonce(nonce);
        self
    }

    /// Set access list.
    fn set_access_list(&mut self, access_list: AccessList);

    /// Set access list.
    fn with_access_list(mut self, access_list: AccessList) -> Self {
        self.set_access_list(access_list);
        self
    }
}

impl TransactionEnv for TxEnv {
    fn set_gas_limit(&mut self, gas_limit: u64) {
        self.gas_limit = gas_limit;
    }

    fn nonce(&self) -> u64 {
        self.nonce
    }

    fn set_nonce(&mut self, nonce: u64) {
        self.nonce = nonce;
    }

    fn set_access_list(&mut self, access_list: AccessList) {
        self.access_list = access_list;

        if self.tx_type == LEGACY_TX_TYPE_ID {
            // if this was previously marked as legacy tx, this must be upgraded to eip2930 with an
            // accesslist
            self.tx_type = EIP2930_TX_TYPE_ID;
        }
    }
}

#[cfg(feature = "op")]
impl<T: TransactionEnv> TransactionEnv for op_revm::OpTransaction<T> {
    fn set_gas_limit(&mut self, gas_limit: u64) {
        self.base.set_gas_limit(gas_limit);
    }

    fn nonce(&self) -> u64 {
        TransactionEnv::nonce(&self.base)
    }

    fn set_nonce(&mut self, nonce: u64) {
        self.base.set_nonce(nonce);
    }

    fn set_access_list(&mut self, access_list: AccessList) {
        self.base.set_access_list(access_list);
    }
}
