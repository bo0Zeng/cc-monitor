//! 主线外清单的对照：本包判据与上层判据（observe 的 watcher 冷读那一路）都拿它比。
use super::{Chain, ChainFact};

/// 一批事实 ⇒ 主线外清单（判据的对照：与 [`Chain`] 逐条记、逐条算的结果相同；冷读那一路改成扫描图里的 [`Chain`] 接着记）。
pub fn off_of(facts: impl IntoIterator<Item = ChainFact>) -> Vec<String> {
    let mut c = Chain::new();
    for f in facts {
        c.load(f);
    }
    c.settle();
    c.off
}
