//! 读路径的越界围栏 —— observe 里唯一的家：「路径必须在某个根之下」那道 `canonicalize` ＋ 前缀校验（挡 symlink 逃逸 / `../` 穿越）。
//! 判定本体（解开根 · 解开目标 · 前缀比）只住这里的 [`Fence`]；`history_query::fence_under_projects` 只是「以 `projects/` 为根立一道 `Fence`」，
//! `search_query` 也经它。安全判定只能有一份：两份的话，强化其中一份时另一份不会跟。
//! 「根是哪一个」是调用方的事 —— 本文件不认识任何一家 agent 的目录布局。
//!
//! 住 `observe/` 不进 `common/`：两个读者同属 observe，不够 `common` 的「≥2 个上层用」门槛。
//! 它是读路径的越界防护，不是「不许改什么」的数据围栏；文件管理那一面（`files/`）不走这里。
//!
//! 报错原话（Claude 的 `projects/` 根：`projects root unavailable: …` · `path unavailable: …` · `refusing to access outside projects dir: …`）
//! 里的「projects」取根目录自己的名字，别的根说的是它自己的名字。
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

#[cfg(test)]
#[path = "../../../tests/backend/observe/fence_tests.rs"]
mod tests;
