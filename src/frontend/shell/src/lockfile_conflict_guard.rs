//! **两份 lockfile 的真冲突必须为空**〔audit-0805 F16 下半，报告 I-6〕。
//!
//! # 报告说「10 个不一致」，核实之后是「2 个真冲突 + 11 个 monitor 超集」
//!
//! 08-06 实测：共有包 **68** · 版本集合不同 **13** · **真冲突 2**
//!（`serde_json 1.0.149/1.0.150` · `memchr 2.8.0/2.8.1`）· monitor 超集 **11**。
//!
//! 报告列的 8 个 `windows_*` **不构成冲突** —— monitor 侧**同时持有**那一版，
//! backend 的解析是 monitor 的**子集**。⇒ **判据不能写成「版本集合相同」**：
//! 那会把 11 个超集也判红，是**一条错的判据**（而且它会逼人去做一件没必要的对齐）。
//!
//! # 那 2 条为什么要紧
//!
//! `ci.yml` 的 backend job 有 `defaults.run.working-directory: src/backend`
//! ⇒ 那条**跨 target Windows check** 走的是 **backend 的 lock**；
//! 而 monitor 真编在 `src/frontend/shell` 下 ⇒ 走**它自己的 lock**。
//!
//! 而 `branch-core` / `usage-core` 都写 `serde_json = "1"`，两者**既是后端的生产依赖、
//! 又是 monitor 的 workspace member** ⇒ **同一份源码分别编进两个版本**。
//!
//! ⚠ 别读成小事：那条跨 target check 是 `ci.yml` 自称的「平台线**唯一真判据**」，
//! 而它证明的依赖树**与 monitor 真编的不是同一棵**。
//!
//! # 顺序：先立判据（红），再对齐（绿）
//!
//! 功能件 §4 把「得先动 lockfile」写成了不做的理由。其实**先立判据才是对的顺序** ——
//! 判据当时就是红的，那个红本身就是 **E11「先红后信」** 要的证据。
//!
//! ⚠ **08-07 订正时态**：上一句原写「判据**此刻**就该是红的」。那是建判据当天的现场，
//! 而那 2 条随后就对齐了（今天两侧都是 `serde_json 1.0.150` / `memchr 2.8.1`）⇒
//! 本模块现在是**绿的**，它守的是「别再漂回去」。
//! 留着原句会让人以为仓里还欠着一次对齐 —— 这正是 E12 那一族（散文记的是修之前）。

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/lockfile_conflict_guard_tests.rs"]
mod tests;
