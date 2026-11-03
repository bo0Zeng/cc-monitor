//! 历史页平铺清单几份判据共用的：这台记录树的串行锁（本包与上层判据经 `test-support` 共用）。

/// 这台的记录树（测试档沙箱家里那一棵）：往里写夹具记录的判据与要它一问一问之间不变的判据都先拿这把锁，错开跑。
/// 不拿 ⇒ 并行时一边写进夹具、另一边第二问就「变了」。
pub static MACHINE_TREE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
