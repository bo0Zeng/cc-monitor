//! 「这一段拼到根上之后还在不在根底下」的路径解析：词法（纯路径算术，不碰盘）＋ 从根句柄逐段往下开（[`walk_in_root`]）。
//! 文件管理那一块的写面（`control/files_write.rs` 等）与原生那一块的传输台（`control/transfer.rs`，下载的本机落点）
//! 每一处改动的路径都先过它；两块之间零互相依赖（`files/module_boundary_guard.rs`），共用的只许住 `common/`。
//! 它只答落点在不在根底下，**不问落点是谁的数据**（要求：「文件管理器全部都可以改. 不需要任何围栏」）。
//! 判据住 `tests/backend/control/files_write_tests.rs`（经写面的再导出跑）。

use crate::platform::fs::{Resolved, Ro};
use copy_core::{copy_text, io_reason};
use std::path::{Path, PathBuf};

/// 路径解析①（词法）：**纯路径算术，不碰盘**。过了就返回「打算写到哪」。
///
/// `rel` 是**相对**目标根的那一段。拒绝的形状逐条：
///
/// - 空串 / 全是空白 —— 没有目标就不该有写。
/// - 绝对路径、Windows 盘符 —— 那不是「根底下的一段」。
/// - 上跳段与当前目录段 —— 上跳是逃出目标根的第一条路；当前目录段本身无害，
///   但留着它就等于承认「这里做路径规范化」，而规范化与安全判定混在一起正是
///   本仓反复踩的那种坑 ⇒ **一律拒，让调用方送干净的段进来。**
///
/// 段判定按字节（与远端写围栏 · 解压条目同一道 `ssh_dial::plain_path`；本机 Windows 上 `\` 也是分隔符）⇒
/// 非 UTF-8 的名字照样逐段判。
pub fn lexical_in_root(root: &Path, rel: impl AsRef<Path>) -> Result<PathBuf, String> {
    use ssh_dial::plain_path::{plain_segments, Dots, Fault};
    let rel = rel.as_ref();
    if rel.to_string_lossy().trim().is_empty() {
        return Err(copy_text("beFilesWrite.path.empty", &[]));
    }
    let path = rel.display().to_string();
    match plain_segments(
        rel.as_os_str().as_encoded_bytes(),
        cfg!(windows),
        Dots::Reject,
    ) {
        Ok(_) => Ok(root.join(rel)),
        Err(Fault::Parent) => Err(copy_text(
            "beFilesWrite.path.parentStep",
            &[("path", &path)],
        )),
        Err(Fault::Dot) => Err(copy_text("beFilesWrite.path.dotStep", &[("path", &path)])),
        Err(Fault::Absolute) => Err(copy_text("beFilesWrite.path.absolute", &[("path", &path)])),
    }
}

/// 最后那一段解不解：作用在链接本身上的动词（新建 · 建目录 · 改名 · 删）不解；跟链接的（改权限 · 换内容 · 复制的源）解到底。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Last {
    Link,
    Follow,
}

/// 路径解析：词法过了 → 把要动的那一格解成真路径（`Link` 只解父目录，`Follow` 连最后一段也解）→ 判它那一层仍在根下 →
/// 从根句柄逐段不跟链接地开到那一层（[`Resolved::walk`]）。回的那一格写只经那个目录句柄：判过之后半路哪一层被换成链接，
/// 逐段开那一步就失败（判完开完之后再换，动的仍是开着的那一层），落不到根外。
/// 父目录必须已在盘上（建目录是 `files_write::make_dir` 的事）；那一格就是根自己（经链接解到根）⇒ 当越根拒。
pub fn walk_in_root(last: Last, root: &Path, rel: impl AsRef<Path>) -> Result<Resolved, String> {
    let target = lexical_in_root(root, rel)?;
    let unresolved = |p: &Path, e: std::io::Error| {
        copy_text(
            "beFilesWrite.path.unresolved",
            &[
                ("path", &win_path_core::shown(&p)),
                ("why", &io_reason(e.kind())),
            ],
        )
    };
    let real_root = Ro::canonicalize(root).map_err(|e| unresolved(root, e))?;
    let real = match last {
        Last::Link => {
            let parent = target.parent().ok_or_else(|| {
                copy_text(
                    "beFilesWrite.path.noParent",
                    &[("path", &win_path_core::shown(&target))],
                )
            })?;
            let name = target.file_name().ok_or_else(|| {
                copy_text(
                    "beFilesWrite.path.noName",
                    &[("path", &win_path_core::shown(&target))],
                )
            })?;
            Ro::canonicalize(parent)
                .map_err(|e| unresolved(parent, e))?
                .join(name)
        }
        Last::Follow => Ro::canonicalize(&target).map_err(|e| unresolved(&target, e))?,
    };
    let (Some(dir), Some(name)) = (real.parent(), real.file_name()) else {
        return Err(copy_text(
            "beFilesWrite.path.noName",
            &[("path", &win_path_core::shown(&real))],
        ));
    };
    let Ok(inner) = dir.strip_prefix(&real_root) else {
        return Err(copy_text(
            "beFilesWrite.path.escaped",
            &[
                ("path", &win_path_core::shown(&real)),
                ("root", &win_path_core::shown(&real_root)),
            ],
        ));
    };
    Resolved::walk(&real_root, inner, name).map_err(|e| unresolved(dir, e))
}
