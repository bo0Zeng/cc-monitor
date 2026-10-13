//! 「这一段拼到根上之后还在不在根底下」的两道路径解析：词法（纯路径算术，不碰盘）＋ 父目录解链接后再判一次。
//! 文件管理那一块的写面（`control/files_write.rs` 等）与原生那一块的传输台（`control/transfer.rs`，下载的本机落点）
//! 每一处改动的路径都先过它；两块之间零互相依赖（`files/module_boundary_guard.rs`），共用的只许住 `common/`。
//! 它只答落点在不在根底下，**不问落点是谁的数据**（用户原话「文件管理器全部都可以改. 不需要任何围栏」）。
//! 判据住 `tests/backend/control/files_write_tests.rs`（经写面的再导出跑）。

use crate::platform::fs::{Resolved, Ro};
use copy_core::{copy_text, io_reason};
use std::path::{Component, Path, PathBuf};

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
/// 🔴 这里原来还有一关「写点自己是不是一份会话文件」（再早是「目标根在不在
/// `~/.claude*` 那几棵树里」）。用户原话「**文件管理器全部都可以改. 不需要任何围栏**」⇒ 那一关删了：
/// 本函数只答「这一段拼到根上之后还在不在根底下」，**不问落点是谁的数据**。
///
/// 入参从 `&str` 换成了 `impl AsRef<Path>`：段判定走的是 `Path::components`，
/// 它本来就**不看编码** ⇒ 非 UTF-8 的名字（乱码文件名）照样逐段判；递归删的子项名字
/// 也不一定是 UTF-8，逐条目过路径解析要的正是这一格。判定一个字没变。
pub fn lexical_in_root(root: &Path, rel: impl AsRef<Path>) -> Result<PathBuf, String> {
    let rel = rel.as_ref();
    if rel.to_string_lossy().trim().is_empty() {
        return Err(copy_text("beFilesWrite.path.empty", &[]));
    }
    let shown = rel.display();
    for c in rel.components() {
        match c {
            Component::Normal(_) => {}
            Component::ParentDir => {
                return Err(copy_text(
                    "beFilesWrite.path.parentStep",
                    &[("path", &shown.to_string())],
                ));
            }
            Component::CurDir => {
                return Err(copy_text(
                    "beFilesWrite.path.dotStep",
                    &[("path", &shown.to_string())],
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(copy_text(
                    "beFilesWrite.path.absolute",
                    &[("path", &shown.to_string())],
                ));
            }
        }
    }
    let target = root.join(rel);
    // 上面已经把非 Normal 的段全拒了 ⇒ 这一条恒真。**留着它是自证**：
    // 哪天有人往上面那个 match 里加一档放行，这里会当场把后果说出来。
    if !target.starts_with(root) {
        return Err(copy_text(
            "beFilesWrite.path.outsideRoot",
            &[
                ("path", &target.display().to_string()),
                ("root", &root.display().to_string()),
            ],
        ));
    }
    Ok(target)
}

/// 路径解析②（现打）：把**父目录**解析成真路径（解 symlink）之后再判一次落点。
///
/// 解析① 是纯字符串算术，它**看不见盘上的 symlink**：目标根里放一条
/// `docs -> /别处`，`root/docs/x.md` 在词法上完全干净，落盘却落到了根外面。
///
/// 处置：解父目录（不解最后那一段 —— 作用在链接本身上的动词不跟它），
/// 然后在真路径上判一次「还在不在目标根底下」。
/// `root` 自己也解一次：两边都解完再比，才比得对。
///
/// 🔴 这里原来在真路径上还判一次「是不是那几份会话文件」，删了（理由同 [`lexical_in_root`]）。
/// 留下的这一判**不是**「不许改什么」，是「这一次到底改的是哪一份」的正确性：`root ＋ rel` 说的是根底下那一格，
/// 一条藏在半路的链接不许把它换成根外面的另一格。
///
/// 🔴 本函数此前还收一个 `claude_home` 并把它也解一次 —— 那是
/// 「整棵树」那一档的需要。换成结构判定之后**配置根这个入参整个不需要了**：
/// 判定只看路径的段形状，`CLAUDE_CONFIG_DIR` 指到哪都判得一样。
/// ⇒ 少一个入参不是整理，是**少一条会错的依赖**。
///
/// ⚠ 父目录**必须已经在盘上**。路径解析不建目录（建目录是 `control::files_write::make_dir` 那一条的事），
/// 所以「父目录不在」是一条正常的拒绝理由，不是内部错误。
pub fn resolve_parent_in_root(root: &Path, target: &Path) -> Result<Resolved, String> {
    let real_root = Ro::canonicalize(root).map_err(|e| {
        copy_text(
            "beFilesWrite.path.unresolved",
            &[
                ("path", &root.display().to_string()),
                ("why", &io_reason(e.kind())),
            ],
        )
    })?;
    let parent = target.parent().ok_or_else(|| {
        copy_text(
            "beFilesWrite.path.noParent",
            &[("path", &target.display().to_string())],
        )
    })?;
    let name = target.file_name().ok_or_else(|| {
        copy_text(
            "beFilesWrite.path.noName",
            &[("path", &target.display().to_string())],
        )
    })?;
    let real_parent = Ro::canonicalize(parent).map_err(|e| {
        copy_text(
            "beFilesWrite.path.unresolved",
            &[
                ("path", &parent.display().to_string()),
                ("why", &io_reason(e.kind())),
            ],
        )
    })?;
    let resolved = real_parent.join(name);
    if !real_parent.starts_with(&real_root) {
        return Err(copy_text(
            "beFilesWrite.path.escaped",
            &[
                ("path", &real_parent.display().to_string()),
                ("root", &real_root.display().to_string()),
            ],
        ));
    }
    Ok(Resolved::vouch(resolved))
}

/// 两道路径解析串起来跑一遍，返回最终落点。**不碰盘上的内容，只解路径。**
///
/// 抽成单独一个函数，是为了让「两道真的被串起来了」这件事有一个可直接喂参数的入口
/// —— 判据不必为了验它而每次都真写一份文件。
///
/// 旧名 `fenced_target`：它今天**不拦任何数据**（用户「不需要任何围栏」），
/// 只做词法不越根 ＋ 父目录解链接后仍在根下 ⇒ 改成说它真在做的那件事。
pub fn resolve_in_root(root: &Path, rel: impl AsRef<Path>) -> Result<Resolved, String> {
    let lexical = lexical_in_root(root, rel)?;
    resolve_parent_in_root(root, &lexical)
}
