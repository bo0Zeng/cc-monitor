//! 聊天里随一句发的附件在会话那台的落点：`<工作目录>/.ccm-attach/<编号>/<原名>`，一份一个编号目录（名字原样、两份同名不撞）。
//! 编号 `<MMDD-HHMM>-<4 位小写十六进制>`：落下那一刻那台钟上的月日时分 ＋ 四位尾巴。
//! 写的一侧（`files/commit/session_attachment.rs`，文件管理那一块）与读的一侧（投进队时判在不在 · 投递时拼引用 · 各家适配层认记录里的引用）
//! 认同一个形状；两块之间零互相依赖，共用的只许住 `common/`（同 [`super::staging`]）。

/// 附件目录相对工作目录的名字。
pub const DIR: &str = ".ccm-attach";

/// 附件目录里自带的那一份忽略规则：只一行 `*`（整个目录不进 git，不碰用户自己的 `.gitignore`）。
pub const GITIGNORE: &str = ".gitignore";
pub const GITIGNORE_BODY: &str = "*\n";

/// 编号的长度（`MMDD-HHMM-xxxx`）。
pub const ID_LEN: usize = 14;

/// 编号：`month`/`day`/`hour`/`minute` 是那台钟上的那一刻，`tail` 是尾巴。
pub fn id_of(month: u32, day: u32, hour: u32, minute: u32, tail: u16) -> String {
    format!("{month:02}{day:02}-{hour:02}{minute:02}-{tail:04x}")
}

/// 这个名字是不是一个编号（只认 [`id_of`] 写得出的形状：别的名字一个不碰）。
pub fn is_id(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == ID_LEN
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 9 => *c == b'-',
            0..=8 => c.is_ascii_digit(),
            _ => c.is_ascii_digit() || (b'a'..=b'f').contains(c),
        })
}

/// 一份附件相对工作目录的路径。
pub fn rel_of(id: &str, name: &str) -> String {
    format!("{DIR}/{id}/{name}")
}

/// 一段路径 ⇒ 它指的那一份附件（编号, 原名）：相对工作目录（`.ccm-attach/<编号>/<原名>`）或绝对（`…/.ccm-attach/<编号>/<原名>`）都认；
/// 编号不合形、原名不是恰好一段 ⇒ `None`。
pub fn parse_rel(s: &str) -> Option<(&str, &str)> {
    let lead = format!("{DIR}/");
    let mid = format!("/{DIR}/");
    let rest = match s.strip_prefix(&lead) {
        Some(r) => r,
        None => &s[s.rfind(&mid)? + mid.len()..],
    };
    let (id, name) = rest.split_once('/')?;
    (is_id(id) && !name.is_empty() && !name.contains('/') && name != "." && name != "..")
        .then_some((id, name))
}

/// 工作目录 `cwd` 下编号 `id` 那一份的原名（编号目录里那一份普通文件）；编号不合形 · 目录不在 · 里面没有普通文件 ⇒ `None`。
pub fn find(cwd: &std::path::Path, id: &str) -> Option<String> {
    if !is_id(id) {
        return None;
    }
    crate::platform::fs::Ro::read_dir(cwd.join(DIR).join(id))
        .ok()?
        .flatten()
        .filter(|e| crate::platform::fs::Ro::symlink_metadata(e.path()).is_ok_and(|m| m.is_file()))
        .find_map(|e| e.file_name().to_str().map(str::to_string))
}

/// 工作目录 `cwd` 下找不到的那几个编号（照 `ids` 的先后；都在 ⇒ 空）。
pub fn missing(cwd: &std::path::Path, ids: &[String]) -> Vec<String> {
    ids.iter()
        .filter(|id| find(cwd, id).is_none())
        .cloned()
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/backend/common/attachment_dir_tests.rs"]
mod tests;
