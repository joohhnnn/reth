//! 辅助类型：表示两种可能的执行器类型之一。
//!
//! 使用 `Either<A, B>` 允许在运行时选择两种不同的执行器实现，
//! 同时保持静态分发。常见于需要根据配置选择不同执行策略的场景。

use crate::{execute::Executor, Database, OnStateHook};

// re-export Either
pub use futures_util::future::Either;
use reth_execution_types::{BlockExecutionOutput, BlockExecutionResult};
use reth_primitives_traits::{NodePrimitives, RecoveredBlock};

impl<A, B, DB> Executor<DB> for Either<A, B>
where
    A: Executor<DB>,
    B: Executor<DB, Primitives = A::Primitives, Error = A::Error>,
    DB: Database,
{
    type Primitives = A::Primitives;
    type Error = A::Error;

    fn execute_one(
        &mut self,
        block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
    ) -> Result<BlockExecutionResult<<Self::Primitives as NodePrimitives>::Receipt>, Self::Error>
    {
        match self {
            Self::Left(a) => a.execute_one(block),
            Self::Right(b) => b.execute_one(block),
        }
    }

    fn execute_one_with_state_hook<F>(
        &mut self,
        block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
        state_hook: F,
    ) -> Result<BlockExecutionResult<<Self::Primitives as NodePrimitives>::Receipt>, Self::Error>
    where
        F: OnStateHook + 'static,
    {
        match self {
            Self::Left(a) => a.execute_one_with_state_hook(block, state_hook),
            Self::Right(b) => b.execute_one_with_state_hook(block, state_hook),
        }
    }

    fn execute(
        self,
        block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
    ) -> Result<BlockExecutionOutput<<Self::Primitives as NodePrimitives>::Receipt>, Self::Error>
    {
        match self {
            Self::Left(a) => a.execute(block),
            Self::Right(b) => b.execute(block),
        }
    }

    fn execute_with_state_closure<F>(
        self,
        block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
        state: F,
    ) -> Result<BlockExecutionOutput<<Self::Primitives as NodePrimitives>::Receipt>, Self::Error>
    where
        F: FnMut(&revm::database::State<DB>),
    {
        match self {
            Self::Left(a) => a.execute_with_state_closure(block, state),
            Self::Right(b) => b.execute_with_state_closure(block, state),
        }
    }

    fn into_state(self) -> revm::database::State<DB> {
        match self {
            Self::Left(a) => a.into_state(),
            Self::Right(b) => b.into_state(),
        }
    }

    fn size_hint(&self) -> usize {
        match self {
            Self::Left(a) => a.size_hint(),
            Self::Right(b) => b.size_hint(),
        }
    }
}
