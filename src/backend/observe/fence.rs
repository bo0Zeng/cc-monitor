//! 〔TL3 · 审计 F 🔴-6〕**读路径的越界围栏 —— observe 里唯一的家**（`设计/15 §4.2` · `§5.3 C5`）。
//!
//! 「路径必须在某个根之下」那道 `canonicalize` ＋ 前缀校验（挡 symlink 逃逸 / `../` 穿越）。
//! 它先前有**两个家**：`history_query.rs` 里的具名围栏 [`fence_under_projects`]（头注逐字「别再造一份」）与
//! `search_query::search` 里的内联一份（头注逐字「复刻 history_query」）—— 而这次护的是**安全判定**：
//! 强化其中一份（比如将来要挡一种新的逃逸形态）时另一份不会跟。
//! 今天：具名那一份连名字搬到这里（`history_query` 三条按路径读的路照旧调它），`search_query` 的内联那份删了、改经 [`Fence`]。
//!
//! # 为什么住 `observe/`，不进 `common/`
//!
//! `15 §4.2` 逐字：「它落不进 `common/`：`common` 门槛①要求『≥2 个上层用（按层数）』，两处同属 observe ⇒ 不达标。
//! ⇒ 正确处置是在 **observe 内部**给它一个家（`observe/fence.rs`，`§5.3 C5`）；**不是改 `common` 的门槛**（改了就变杂物间）」。
//!
//! # 它是什么、不是什么
//!
//! 它是**读路径的越界防护**（路径解析的正确性），不是 V119 拿掉的那种「不许改什么」的数据围栏（`15 §4.2` 末段）。
//! 文件管理那一面（`files/`）没有数据围栏，也不走这里。
//!
//! # 报错原话
//!
//! 与收口前 `history_query.rs` 里那一份**逐字相同**（Claude 的 `projects/` 根：`projects root unavailable: …` ·
//! `path unavailable: …` · `refusing to access outside projects dir: …`）—— 句里的「projects」取根目录自己的名字，
//! 别的根（例如各家合成历史面给的记录根）说的是它自己的名字。
//!
//! 判据：`tests/backend/observe/fence_tests.rs`（observe 全树现扫：`canonicalize` 只在这里 · 两个读者都经它）。

use std::path::{Path, PathBuf};

/// 一道以 `root` 为界的围栏：`root` 已经规范化（symlink 解开）。
pub(crate) struct Fence {
    root: PathBuf,
    /// 报错里怎么称呼这个根（取根目录自己的名字）。
    what: String,
}

impl Fence {
    /// 以 `root` 为界。根自己解不开（不存在 / 读不了）⇒ `Err`。
    pub(crate) fn at(root: &Path) -> Result<Self, String> {
        let what = root
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let root = root
            .canonicalize()
            .map_err(|e| format!("{what} root unavailable: {e}"))?;
        Ok(Self { root, what })
    }

    /// 以这个 agent 家目录下的 `projects/` 为界（Claude 的会话记录树）。
    pub(crate) fn projects(agent_home: &Path) -> Result<Self, String> {
        Self::at(&crate::agents::claudecode::paths::projects_root(agent_home))
    }

    /// 规范化之后的根（遍历从这里起，走到的每一条都在界内的前提才成立）。
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// 放行一个候选路径：相对路径按根拼、绝对路径直接用；规范化（symlink 解开）之后必须仍在根下。
    /// 放行 ⇒ 回规范化之后的那条路径。
    pub(crate) fn admit(&self, candidate: &Path) -> Result<PathBuf, String> {
        let joined = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            self.root.join(candidate)
        };
        let target = joined
            .canonicalize()
            .map_err(|e| format!("path unavailable: {e}"))?;
        if !target.starts_with(&self.root) {
            return Err(format!(
                "refusing to access outside {} dir: {}",
                self.what,
                target.display()
            ));
        }
        Ok(target)
    }
}

/// `<agent_home>/projects/` 这道围栏放行一个候选路径（`history_query` 三条按路径读的路共用；名字随它从那份文件搬来）。
pub(crate) fn fence_under_projects(agent_home: &Path, candidate: &Path) -> Result<PathBuf, String> {
    Fence::projects(agent_home)?.admit(candidate)
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/fence_tests.rs"]
mod tests;
