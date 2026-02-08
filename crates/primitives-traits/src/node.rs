// ============================================================================
// 为什么需要 NodePrimitives？
// ============================================================================
//
// 问题背景：
//   reth 想同时支持 Ethereum 和 Optimism（以及未来更多链）。
//   这两条链的"区块"、"交易"、"收据"长得不一样：
//     - Ethereum 的区块体有 uncle headers
//     - Optimism 的区块体没有 uncle headers，但有 deposit 交易
//
// 如果代码里写死具体类型：
//   fn process_block(block: EthereumBlock) { ... }
//   那 Optimism 就没法复用这个函数。
//
// 解决方案：
//   定义一个 trait（蓝图），说"我不管你具体是什么类型，
//   只要你能告诉我你的 Block、Transaction、Receipt 分别是什么就行"。
//   这就是 NodePrimitives 的作用。
//
// 使用时：
//   fn process_block<N: NodePrimitives>(block: N::Block) { ... }
//   这个函数对 Ethereum 和 Optimism 都能用。
// ============================================================================

use crate::{
    FullBlock, FullBlockBody, FullBlockHeader, FullReceipt, FullSignedTx, MaybeSerdeBincodeCompat,
};
use core::fmt;

/// 配置节点所有的原始数据类型。
///
/// 这个 trait 定义了整个节点中用于表示区块链数据的核心类型。
/// 它是不同节点实现（Ethereum、Optimism 等）之间类型一致性的基础。
//
// ---- 语法讲解 ----
//
// pub trait NodePrimitives: A + B + C { ... }
//                           ^^^^^^^^^ 这叫"超级 trait 约束"（supertrait bounds）
// 意思是：任何实现 NodePrimitives 的类型，必须同时实现 A、B、C。
//
// 下面逐个解释每个约束：
//   Send     → 可以安全地跨线程传递（因为节点是多线程的）
//   Sync     → 可以安全地在多线程间共享引用
//   Unpin    → 不需要固定在内存中（与 async/Pin 相关，这里表示类型比较"普通"）
//   Clone    → 可以克隆（.clone()）
//   Default  → 可以创建默认值（Default::default()）
//   fmt::Debug → 可以用 {:?} 打印调试信息
//   PartialEq + Eq → 可以用 == 比较
//   'static  → 类型里不包含临时引用（生命周期约束，保证可以长期持有）
//
pub trait NodePrimitives:
    Send + Sync + Unpin + Clone + Default + fmt::Debug + PartialEq + Eq + 'static
{
    // ---- 语法讲解：关联类型（Associated Type）----
    //
    // type Block: FullBlock<...>;
    // ^^^^                       这是"关联类型"声明，不是定义具体类型
    //       ^^^^^^^^^^^^^        冒号后面是约束，表示这个类型必须满足的条件
    //
    // 类比：
    //   trait 说："我有一个叫 Block 的'插槽'，你来填上具体类型"
    //   impl 说："好的，我填 EthereumBlock"
    //
    // 为什么用关联类型而不是泛型参数？
    //   泛型参数：trait NodePrimitives<B, H, T, R> → 一个类型可以有多种实现，调用时要写全
    //   关联类型：trait NodePrimitives { type Block; } → 一个类型只有一种实现，更简洁
    //   对于"Ethereum 节点的区块类型是什么"这种问题，答案是唯一的，所以用关联类型。

    /// 区块类型。
    //
    // FullBlock<Header = Self::BlockHeader, Body = Self::BlockBody>
    //           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    //           这叫"关联类型等式约束"，意思是：
    //           Block 这个类型实现了 FullBlock trait，
    //           并且它的 Header 关联类型 == 本 trait 的 BlockHeader，
    //           并且它的 Body 关联类型 == 本 trait 的 BlockBody。
    //
    //           这保证了类型一致性！
    //           比如你不能把 Optimism 的 Header 塞进 Ethereum 的 Block 里。
    //
    // + MaybeSerdeBincodeCompat
    //   额外要求：如果启用了 serde-bincode-compat feature，还要支持 bincode 序列化。
    type Block: FullBlock<Header = Self::BlockHeader, Body = Self::BlockBody>
        + MaybeSerdeBincodeCompat;

    /// 区块头类型。
    type BlockHeader: FullBlockHeader;

    /// 区块体类型。
    //
    // FullBlockBody<Transaction = Self::SignedTx, OmmerHeader = Self::BlockHeader>
    //               ^^^^^^^^^^^^^^^^^^^^^^^^      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    //               区块体里的交易类型               叔块头类型
    //               必须 == 本 trait 的 SignedTx     必须 == 本 trait 的 BlockHeader
    //
    // 这些等式约束形成了一张"类型关系网"：
    //
    //   NodePrimitives
    //   ├── Block.Header    == BlockHeader     ✓ 一致
    //   ├── Block.Body      == BlockBody       ✓ 一致
    //   ├── BlockBody.Tx    == SignedTx        ✓ 一致
    //   └── BlockBody.Ommer == BlockHeader     ✓ 一致
    //
    // 编译器会在编译时检查这些关系，类型不匹配直接报错。
    type BlockBody: FullBlockBody<Transaction = Self::SignedTx, OmmerHeader = Self::BlockHeader>;

    /// 签名交易类型。
    ///
    /// 代表交易在区块链中的存在形式 ——
    /// 包含签名的共识格式，可以被包含在区块中。
    type SignedTx: FullSignedTx;

    /// 交易收据类型。
    type Receipt: FullReceipt;
}

// ============================================================================
// 类型别名（Type Alias）
// ============================================================================
//
// 下面这些是为了让代码更简洁而定义的"快捷方式"。
//
// ---- 语法讲解 ----
//
// pub type HeaderTy<N> = <N as NodePrimitives>::BlockHeader;
//          ^^^^^^^^      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
//          新名字         完整写法
//
// <N as NodePrimitives>::BlockHeader 这个语法叫"完全限定路径"（Fully Qualified Path）
// 意思是："把 N 当作 NodePrimitives 看待，取它的 BlockHeader 关联类型"
//
// 为什么需要 <N as NodePrimitives>？
//   因为 N 可能同时实现了多个 trait，每个 trait 都可能有同名的关联类型。
//   用 <N as Trait> 明确指定"我要的是哪个 trait 的"。
//
// 使用前后对比：
//   不用别名：fn foo<N: NodePrimitives>(header: <N as NodePrimitives>::BlockHeader)
//   用别名后：fn foo<N: NodePrimitives>(header: HeaderTy<N>)
//   效果完全一样，只是更短。
// ============================================================================

/// 快捷访问 NodePrimitives 的区块头类型。
pub type HeaderTy<N> = <N as NodePrimitives>::BlockHeader;

/// 快捷访问 NodePrimitives 的区块体类型。
pub type BodyTy<N> = <N as NodePrimitives>::BlockBody;

/// 快捷访问 NodePrimitives 的区块类型。
pub type BlockTy<N> = <N as NodePrimitives>::Block;

/// 快捷访问 NodePrimitives 的收据类型。
pub type ReceiptTy<N> = <N as NodePrimitives>::Receipt;

/// 快捷访问 NodePrimitives 的签名交易类型。
pub type TxTy<N> = <N as NodePrimitives>::SignedTx;
