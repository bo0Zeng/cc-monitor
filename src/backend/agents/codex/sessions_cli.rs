//! 删一条 Codex 会话交给 Codex 自己的命令（注册表 `RecordFace.delete` 那一格的「跑一条命令」那一形）：
//! `codex delete <uuid> --force`（Codex rust-v0.167.0 的 CLI）。走它，Codex 的状态库跟着一起改；后端不直接删 rollout 文件。
//!
//! 这里只拼那一条命令：起哪个程序（默认启动器 [`super::resume::DEFAULT_COMMAND`]）· 带哪几个参数 · 认哪个 Codex 家（`CODEX_HOME`，
//! 与找到那一份记录的是同一个家）。起它、等它、读退出码是通用层的事（删会话那道门）。

use std::path::Path;

/// 删会话那一条的子命令与「不再问一次」那个旗标。
const DELETE: &str = "delete";
const FORCE: &str = "--force";

use super::parse::HOME_ENV;

/// 注册表 `SessionDelete::Command` 那一格：会话所在那个 Codex 家（通用层照根表给）里有这个 sid 的会话 ⇒ 删它的那一条命令；
/// 日期树 · 归档根两处都认（归档了的照样删）；没有 ⇒ `None`（不是这一家的）。
pub(crate) fn delete_command(codex_home: &Path, sid: &str) -> Option<crate::agents::DeleteCommand> {
    let record = dated(codex_home, sid).or_else(|| parked(codex_home, sid))?;
    Some(crate::agents::DeleteCommand {
        program: super::resume::DEFAULT_COMMAND.to_string(),
        args: vec![DELETE.to_string(), sid.to_string(), FORCE.to_string()],
        env: vec![(HOME_ENV, codex_home.as_os_str().to_os_string())],
        record,
    })
}

/// 归档 · 取消归档那两条的子命令（codex-rs `rust-v0.167.0`：`codex archive <SESSION>` · `codex unarchive <SESSION>`，
/// `SESSION` 是 uuid 或会话名，uuid 先认）。Codex 自己在 `sessions/YYYY/MM/DD/` 与平铺的 `archived_sessions/` 之间挪 rollout，
/// 状态库跟着改；归档一条父会话时它派出的子线程一起归档。
const ARCHIVE: &str = "archive";
const UNARCHIVE: &str = "unarchive";

/// 注册表 `ArchiveFace.archive`：会话所在那个 Codex 家的日期树里有这个 sid 的会话 ⇒ 归档它的那一条命令；没有 · 已归档 ⇒ `None`。
pub(crate) fn archive_command(
    codex_home: &Path,
    sid: &str,
) -> Option<crate::agents::ArchiveCommand> {
    dated(codex_home, sid).map(|_| archive_cli(codex_home, ARCHIVE, sid))
}

/// 注册表 `ArchiveFace.unarchive`：会话所在那个 Codex 家的归档根里有这个 sid 的会话 ⇒ 取消归档的那一条命令；没有 ⇒ `None`。
pub(crate) fn unarchive_command(
    codex_home: &Path,
    sid: &str,
) -> Option<crate::agents::ArchiveCommand> {
    parked(codex_home, sid).map(|_| archive_cli(codex_home, UNARCHIVE, sid))
}

/// 这个家的日期树里那一份（没归档的）。
fn dated(codex_home: &Path, sid: &str) -> Option<std::path::PathBuf> {
    super::history::find_session(&super::parse::sessions_root(codex_home), sid).ok()
}

/// 这个家的归档根里那一份（归档了的）。
fn parked(codex_home: &Path, sid: &str) -> Option<std::path::PathBuf> {
    super::history::find_session(&super::parse::archived_root(codex_home), sid).ok()
}

fn archive_cli(codex_home: &Path, verb: &str, sid: &str) -> crate::agents::ArchiveCommand {
    crate::agents::ArchiveCommand {
        program: super::resume::DEFAULT_COMMAND.to_string(),
        args: vec![verb.to_string(), sid.to_string()],
        env: vec![(HOME_ENV, codex_home.as_os_str().to_os_string())],
    }
}

/// 注册表 `ArchiveFace.archived`：rollout 直接住在 `archived_sessions/` 底下 ⇒ 此刻是归档的（Codex 自己也照所在目录判，不读它的状态库）。
pub(crate) fn is_archived(p: &Path) -> bool {
    p.parent()
        .and_then(Path::file_name)
        .is_some_and(|n| n == super::parse::ARCHIVED_DIR)
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/sessions_cli_tests.rs"]
mod tests;
