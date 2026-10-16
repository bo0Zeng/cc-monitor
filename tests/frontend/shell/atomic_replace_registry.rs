//! **原子替换调用点登记：每一处写明写的是哪类文件、所以用哪一套语义**。
//!
//! `src/doc/INVARIANTS.md §4`：用户文件不能拿暂存件 `rename` / `MoveFileExW` 覆盖（会盖掉 dst 的 ACL）；
//! monitor 自己的文件（`config.json` · 释放出来的程序）没有要保的 ACL，标准库的 `rename` 就够。
//! 两类文件的正确行为本来就不同，套一层按参数分派的统一写入器只会把「选哪套」藏起来；
//! 登记表让这个决定留在明处：新增一处没登记的调用点就红，并把选择规则原样打出来。

#[cfg(test)]
#[path = "atomic_replace_registry_tests.rs"]
mod tests;
