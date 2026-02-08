use crate::{execute::ExecutableTxFor, ConfigureEvm, EvmEnvFor, ExecutionCtxFor, TxEnvFor};
use alloy_evm::{block::ExecutableTxParts, RecoveredTx};
use rayon::prelude::*;
use reth_primitives_traits::TxTy;

/// [`ConfigureEvm`] 的扩展 trait，提供执行 payload（有效载荷）的方法。
///
/// 专门用于 Engine API 场景（处理来自共识层的 newPayload 请求）。
/// 提供从 payload 数据创建 EVM 环境和交易迭代器的方法。
pub trait ConfigureEngineEvm<ExecutionData>: ConfigureEvm {
    /// Returns an [`crate::EvmEnv`] for the given payload.
    fn evm_env_for_payload(&self, payload: &ExecutionData) -> Result<EvmEnvFor<Self>, Self::Error>;

    /// Returns an [`ExecutionCtxFor`] for the given payload.
    fn context_for_payload<'a>(
        &self,
        payload: &'a ExecutionData,
    ) -> Result<ExecutionCtxFor<'a, Self>, Self::Error>;

    /// Returns an [`ExecutableTxIterator`] for the given payload.
    fn tx_iterator_for_payload(
        &self,
        payload: &ExecutionData,
    ) -> Result<impl ExecutableTxIterator<Self>, Self::Error>;
}

/// 辅助 trait：表示"原始"交易迭代器和转换闭包的配对。
///
/// 用于 Engine 中并行化解码或恢复签名等重计算工作。
/// 例如，原始交易可能是未恢复签名的交易字节，转换闭包负责恢复发送者地址。
/// 这种设计允许使用 rayon 并行处理交易恢复。
pub trait ExecutableTxTuple: Into<(Self::IntoIter, Self::Convert)> + Send + 'static {
    /// Raw transaction that can be converted to an [`ExecutableTxTuple::Tx`]
    ///
    /// This can be any type that can be converted to an [`ExecutableTxTuple::Tx`]. For example,
    /// an unrecovered transaction or just the transaction bytes.
    type RawTx: Send + Sync + 'static;
    /// The executable transaction type iterator yields.
    type Tx: Clone + Send + Sync + 'static;
    /// Errors that may occur while recovering or decoding transactions.
    type Error: core::error::Error + Send + Sync + 'static;

    /// Iterator over [`ExecutableTxTuple::Tx`].
    type IntoIter: IntoParallelIterator<Item = Self::RawTx, Iter: IndexedParallelIterator>
        + Send
        + 'static;
    /// Closure that can be used to convert a [`ExecutableTxTuple::RawTx`] to a
    /// [`ExecutableTxTuple::Tx`]. This might involve heavy work like decoding or recovery
    /// and will be parallelized in the engine.
    type Convert: Fn(Self::RawTx) -> Result<Self::Tx, Self::Error> + Send + Sync + 'static;
}

impl<RawTx, Tx, Err, I, F> ExecutableTxTuple for (I, F)
where
    RawTx: Send + Sync + 'static,
    Tx: Clone + Send + Sync + 'static,
    Err: core::error::Error + Send + Sync + 'static,
    I: IntoParallelIterator<Item = RawTx, Iter: IndexedParallelIterator> + Send + 'static,
    F: Fn(RawTx) -> Result<Tx, Err> + Send + Sync + 'static,
{
    type RawTx = RawTx;
    type Tx = Tx;
    type Error = Err;

    type IntoIter = I;
    type Convert = F;
}

/// Iterator over executable transactions.
pub trait ExecutableTxIterator<Evm: ConfigureEvm>:
    ExecutableTxTuple<Tx: ExecutableTxFor<Evm, Recovered = Self::Recovered>>
{
    /// HACK: for some reason, this duplicated AT is the only way to enforce the inner Recovered:
    /// Send + Sync bound. Effectively alias for `Self::Tx::Recovered`.
    type Recovered: RecoveredTx<TxTy<Evm::Primitives>> + Send + Sync;
}

impl<T, Evm: ConfigureEvm> ExecutableTxIterator<Evm> for T
where
    T: ExecutableTxTuple<Tx: ExecutableTxFor<Evm, Recovered: Send + Sync>>,
{
    type Recovered = <T::Tx as ExecutableTxParts<TxEnvFor<Evm>, TxTy<Evm::Primitives>>>::Recovered;
}
