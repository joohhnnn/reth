//! 使用 [`ConfigureEvm`] 及本 crate 中 trait 时的辅助类型别名。
//!
//! 这些类型别名简化了深层嵌套的关联类型访问。例如，要获取某个 `ConfigureEvm`
//! 实现的 EVM 工厂类型，不需要写冗长的 `<<Evm as ConfigureEvm>::BlockExecutorFactory
//! as BlockExecutorFactory>::EvmFactory`，而只需写 `EvmFactoryFor<Evm>`。

use crate::ConfigureEvm;
use alloy_evm::{block::BlockExecutorFactory, Database, EvmEnv, EvmFactory};
use revm::{inspector::NoOpInspector, Inspector};

/// 获取给定 [`ConfigureEvm`] 的 EVM 工厂类型。
pub type EvmFactoryFor<Evm> =
    <<Evm as ConfigureEvm>::BlockExecutorFactory as BlockExecutorFactory>::EvmFactory;

/// 获取给定 [`ConfigureEvm`] 的硬分叉规范（Spec）类型（如 SpecId::SHANGHAI）。
pub type SpecFor<Evm> = <EvmFactoryFor<Evm> as EvmFactory>::Spec;

/// 获取给定 [`ConfigureEvm`] 的区块环境类型（包含 coinbase、gas limit 等）。
pub type BlockEnvFor<Evm> = <EvmFactoryFor<Evm> as EvmFactory>::BlockEnv;

/// 获取给定 [`ConfigureEvm`] 的 EVM 实例类型。
pub type EvmFor<Evm, DB, I = NoOpInspector> = <EvmFactoryFor<Evm> as EvmFactory>::Evm<DB, I>;

/// 获取给定 [`ConfigureEvm`] 的 EVM 错误类型。
pub type EvmErrorFor<Evm, DB> = <EvmFactoryFor<Evm> as EvmFactory>::Error<DB>;

/// 获取给定 [`ConfigureEvm`] 的 EVM 上下文类型。
pub type EvmContextFor<Evm, DB> = <EvmFactoryFor<Evm> as EvmFactory>::Context<DB>;

/// 获取给定 [`ConfigureEvm`] 的 EVM 停止原因类型（如 OutOfGas、Revert 等）。
pub type HaltReasonFor<Evm> = <EvmFactoryFor<Evm> as EvmFactory>::HaltReason;

/// 获取给定 [`ConfigureEvm`] 的交易环境类型。
pub type TxEnvFor<Evm> = <EvmFactoryFor<Evm> as EvmFactory>::Tx;

/// 获取给定 [`ConfigureEvm`] 的区块执行上下文类型。
pub type ExecutionCtxFor<'a, Evm> =
    <<Evm as ConfigureEvm>::BlockExecutorFactory as BlockExecutorFactory>::ExecutionCtx<'a>;

/// 给定 [`ConfigureEvm`] 的 EVM 环境类型别名（包含 Spec + BlockEnv）。
pub type EvmEnvFor<Evm> = EvmEnv<SpecFor<Evm>, BlockEnvFor<Evm>>;

/// Inspector trait 的辅助约束 —— 将 Inspector 绑定到特定的 ConfigureEvm 和数据库类型。
/// Inspector 是 revm 的调试/跟踪接口，用于在 EVM 执行过程中插入自定义逻辑。
pub trait InspectorFor<Evm: ConfigureEvm, DB: Database>: Inspector<EvmContextFor<Evm, DB>> {}
impl<T, Evm, DB> InspectorFor<Evm, DB> for T
where
    Evm: ConfigureEvm,
    DB: Database,
    T: Inspector<EvmContextFor<Evm, DB>>,
{
}
