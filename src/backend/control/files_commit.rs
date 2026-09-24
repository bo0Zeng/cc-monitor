//! 〔F7c · 第三波 · 2026-09-24〕**上传的提交** —— 把暂存区里一份传完的件挪进用户指定的目标。
//!
//! # 为什么有这一份（用户逐字两句）
//!
//! 「**保留SFTP. 思考怎么干净**」＋「**现在只允许后端的文件管理部分写文件**」。
//! ⇒ `设计/60 §13`：SFTP 缩成只做传输，而上传**不直接写到目标** —— SFTP 只写进我们自己的暂存区
//! （`~/.cc-monitor/staging/<key>.part`，断点续传的本钱也住在那儿），传完由**这里**提交。
//! ⇒ 真正落进用户目录的那一下，只有后端文件管理这一处。
//!
//! # 它是文件管理那一面的成员，住 `control/` 是**层**的归属
//!
//! 同 `control/files_write.rs` 那条理由：它会改变世界 ⇒ 住「会改变世界」那一层；
//! 它属于文件管理后端这个模块（`tests/backend/files/module_boundary_guard.rs` 的成员名单）。
//! ⚠ 刻意**不住 `files/`**：那一族的头注与 `设计/96 §2.9` 边界①逐字「整族纯读」——
//! 往那棵树里放一个会改名的模块，那句话就当场变假。
//!
//! # 它在 `readonly_guard` 第三层上（第二个登记的模块）
//!
//! 第三层的四条对本模块逐条成立：改动动词只用闭集里那几个 · 表外改动照旧禁 ·
//! **每个会改动的函数先过围栏**（围栏本体借 `files_write` 那一份，一个字节不抄）·
//! 后端生产树里引用得到本模块的**只有** `inbound.rs` 那一条命令。
//!
//! # 提交的两支（`overwrite` 由调用方**显式**给，不给默认值）
//!
//! | `overwrite` | 做法 | 为什么 |
//! |---|---|---|
//! | `false` | 先在目标上用 `O_EXCL` **占一个位**（已在 ⇒ 拒），再把暂存件改名上位、盖掉那个占位 | 「目标已经在了就拒」要**原子地**判：先看一眼再改名，中间有一个窗（`files_write::rename_entry` 头注自陈的那一格）；占位把那个窗关上了 —— 占位成功的那一刻，目标**确实**此前不存在 |
//! | `true` | 暂存件直接改名上位 | 同一个盘上的改名是原子的：读的人要么看见旧的一整份，要么看见新的一整份 |
//!
//! ⚠ 暂存件的路径**由本模块自己拼**（`$HOME` ＋ 那个固定的相对段 ＋ `key`），`key` 只收
//! [`KEY_LEN`] 位小写十六进制 ⇒ 调用方**指不到**暂存区之外的任何一个文件当「源」。
//!
//! # 买不到什么（逐条）
//!
//! - **跨盘的提交做不到**：暂存区与目标不在同一个文件系统上 ⇒ 改名回 `EXDEV`，原样带回。
//!   要做就得「复制 ＋ 删」，而复制在第三层禁表里 —— 新形状，要单独论证。
//! - **「SFTP 的起始目录 == 这里的 `$HOME`」是前提**：`ChrootDirectory` / `internal-sftp -d`
//!   那种机器上两边拼出来的暂存件不是同一份 ⇒ 本命令答「暂存件不在」，出声但做不成。
//! - **占位与改名之间仍有一个窗**：别的进程在那一瞬往占位里写了东西，改名会把它盖掉。
//!   那个窗里的东西是「一份刚出现、0 字节起步的文件」，不是用户既有数据 —— 如实登记。
//! - **Windows 远端**：改名覆盖的语义不同（目标在就失败），`overwrite: true` 那一支在那边会拒。没量过。

use crate::control::files_write::{fenced_target, Answer, ManageCommand, WriteRefusal};
use std::path::{Path, PathBuf};

/// 暂存区相对 `$HOME` 的那一段。
///
/// 🔴 **桥那一侧有一份逐字相同的**（同名常量住 `sftp_pool.rs`，SFTP 往里写）。
/// 两个 crate 没有共享落点（同 `is_protected_session_path` 那两份的理由，`设计/60 §11.4`），
/// ⇒ 「两份逐字副本 ＋ 相等断言」：本侧的判据现读桥那一份源码，逐字节比。
pub const STAGING_DIR: &str = ".cc-monitor/staging";

/// 暂存件的键长（十六进制位数）。桥那一侧造键时同一个数，判据逐字比。
pub const KEY_LEN: usize = 32;

/// 暂存件的后缀。
pub const PART_SUFFIX: &str = ".part";

/// 🔴 **本面的命令表**（形状借写面那一个类型，理由同它：线上契约的数据形态）。
pub const COMMIT_COMMANDS: &[ManageCommand] = &[ManageCommand {
    name: "files-commit-upload",
    what:
        "把暂存区里一份传完的上传件挪进用户指定的目标（先过围栏；不覆盖时 `O_EXCL` 占位再改名上位）",
    args: &["key", "overwrite", "rel", "root"],
    fields: &["bytes", "path"],
    codes: &["bad_args", "bad_path", "io_failed", "refused"],
}];

/// 本面命令名（给 `readonly_guard` 第三层那条「门里够得到的命令」对拍用）。
pub fn commit_command_names() -> Vec<&'static str> {
    COMMIT_COMMANDS.iter().map(|c| c.name).collect()
}

/// `key` 合不合法：恰好 [`KEY_LEN`] 位、只有 `0-9a-f`。
pub fn is_key(key: &str) -> bool {
    key.len() == KEY_LEN
        && key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// 暂存件的绝对路径。`key` 不合法 ⇒ 拒（**不**猜、不清洗）。
pub fn staged_path(home: &Path, key: &str) -> Result<PathBuf, WriteRefusal> {
    if !is_key(key) {
        return Err(WriteRefusal::Fenced(format!(
            "refuse write: 暂存件的键只收 {KEY_LEN} 位小写十六进制（给的是 {key:?}）"
        )));
    }
    Ok(home.join(STAGING_DIR).join(format!("{key}{PART_SUFFIX}")))
}

/// **提交**：暂存件 → 目标。成功回 `(落点, 字节数)`。
///
/// 第一件事是过围栏（第三层 ③ 逐函数扫这个顺序）。
pub fn commit_upload(
    home: &Path,
    key: &str,
    root: &Path,
    rel: &str,
    overwrite: bool,
) -> Result<(PathBuf, u64), WriteRefusal> {
    let dest = fenced_target(root, rel).map_err(WriteRefusal::Fenced)?;
    let staged = staged_path(home, key)?;
    // 不跟链接地看暂存件一眼：它必须是一份普通文件（一条链接当「源」＝ 挪走链接指向之外的东西，不许）。
    let meta = std::fs::symlink_metadata(&staged).map_err(|e| {
        WriteRefusal::Io(format!(
            "refuse write: 暂存件不在（{}：{e}）—— 传输没跑完，或者 SFTP 的起始目录不是这台后端的 home",
            staged.display()
        ))
    })?;
    if !meta.file_type().is_file() {
        return Err(WriteRefusal::Fenced(format!(
            "refuse write: 暂存件不是一份普通文件（{}）",
            staged.display()
        )));
    }
    let bytes = meta.len();
    if overwrite {
        std::fs::rename(&staged, &dest).map_err(|e| {
            WriteRefusal::Io(format!(
                "refuse write: 暂存件改名上位到 {} 失败：{e}",
                dest.display()
            ))
        })?;
        return Ok((dest, bytes));
    }
    // 不覆盖：先占位（`O_EXCL`，目标已在 ⇒ 当场失败），再改名上位盖掉自己那个占位。
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&dest)
        .map_err(|e| {
            WriteRefusal::Io(format!(
                "refuse write: 目标已经在了、或者占不了位（{}：{e}）—— 不覆盖，先删掉它或换个名字",
                dest.display()
            ))
        })?;
    if let Err(e) = std::fs::rename(&staged, &dest) {
        // 撤掉自己那个 0 字节的占位（它是这一次刚建的，不是用户既有数据）。
        let _ = std::fs::remove_file(&dest);
        return Err(WriteRefusal::Io(format!(
            "refuse write: 暂存件改名上位到 {} 失败：{e}",
            dest.display()
        )));
    }
    Ok((dest, bytes))
}

/// 取一个路径参数（字符串 或 `{"b16": …}`，与写面同一口径）。
fn path_arg(args: &serde_json::Value, key: &str) -> Result<PathBuf, (&'static str, String)> {
    let v = args.get(key).ok_or(("bad_path", format!("少了 `{key}`")))?;
    let raw = crate::files::raw::from_json(v).ok_or((
        "bad_path",
        format!("`{key}` 的形状不对 —— 只认字符串或 `{{\"b16\": \"<十六进制>\"}}`"),
    ))?;
    if raw.is_empty() {
        return Err(("bad_path", format!("`{key}` 是空的")));
    }
    Ok(crate::files::raw::to_path_buf(&raw))
}

/// 取一个字符串参数。
fn str_arg<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str, (&'static str, String)> {
    args.get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or(("bad_args", format!("少了 `{key}`，或者它不是一个字符串")))
}

fn answer_commit(args: &serde_json::Value) -> Answer {
    let root = path_arg(args, "root")?;
    let rel = str_arg(args, "rel")?.to_string();
    let key = str_arg(args, "key")?.to_string();
    // 🔴 覆盖策略**必须显式给**：默认成哪一边都是替用户做了一个他没做的决定。
    let overwrite = args
        .get("overwrite")
        .and_then(serde_json::Value::as_bool)
        .ok_or((
            "bad_args",
            "少了 `overwrite`（true / false）—— 覆盖不覆盖不给默认值".to_string(),
        ))?;
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or((
        "io_failed",
        "这台后端不知道自己的 home（没有 `HOME`）—— 暂存区拼不出来".to_string(),
    ))?;
    let (landed, bytes) = commit_upload(&home, &key, &root, &rel, overwrite)
        .map_err(|e| (e.code(), e.message().to_string()))?;
    Ok(serde_json::json!({
        "path": crate::files::raw::to_json(crate::files::raw::path_bytes(&landed)),
        "bytes": bytes,
    }))
}

/// 这一面的**唯一入口**（形状照写面那一个）。
pub fn answer_wire(wire_name: &str, args: &serde_json::Value) -> Answer {
    match wire_name {
        "files-commit-upload" => answer_commit(args),
        other => Err(("bad_args", format!("`{other}` 不是上传提交那一面的命令"))),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/files_commit_tests.rs"]
mod tests;
