//! 策略目录（`<家>/strategies/`）：一份策略 ＝ 一个 `.rhai` 文件，文件名去 `.rhai` 就是 id，也就是标题（§2.1）；全是用户自己的（没有随包策略）。
//! 目录的戳变了才重读；读不懂的那一份也列着、带原因。
//! 第三版的脚本升级时挪进 `v3/`，不加载（[`old_files`] 列出来，界面提示「N 份旧格式的策略没有加载」、点开看是哪几份）。

use super::script;
use crate::accounts::quota::strategy::{Fail, FailKind};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// 策略目录的名字（同 `rotation.json` 一层）。
pub(crate) const DIR_NAME: &str = "strategies";
/// 第三版脚本挪去的子目录。
pub(crate) const OLD_DIR: &str = "v3";
/// 一份文件至多多大。
pub(crate) const MAX_SOURCE_BYTES: usize = 64 * 1024;
/// 目录里至多读几份。
pub(crate) const MAX_FILES: usize = 64;
/// id 至多几个字。
pub(crate) const MAX_ID_CHARS: usize = 32;

/// 目录里的一份。
pub(crate) struct Shelved {
    pub(crate) id: String,
    pub(crate) source: String,
    /// 内容版本（`h:` ＋ 内容摘要的前 12 位）：存的时候比它，别处改过就对不上。
    pub(crate) rev: String,
    pub(crate) file: Result<script::File, Fail>,
}

impl std::fmt::Debug for Shelved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shelved")
            .field("id", &self.id)
            .field("rev", &self.rev)
            .field("ok", &self.file.is_ok())
            .finish_non_exhaustive()
    }
}

/// 文件名里除字母数字（含中文）以外收的几个字。
const NAME_PUNCT: &[char] = &[' ', '-', '_', '·', '(', ')', '（', '）', '+'];

/// Windows 上不能当文件名的几个（不分大小写）。
const RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

fn name_char(c: char) -> bool {
    c.is_alphanumeric() || NAME_PUNCT.contains(&c)
}

/// id（＝ 文件名 ＝ 标题）能不能用：1–32 字，字母数字（含中文）与 ` -_·()（）+`、至少一个字母数字，首尾不是空白，不是 Windows 的保留名。
/// `.` `/` `\` 都不在里面 ⇒ 拼不出 `..`，也出不了策略目录。
pub(crate) fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.chars().count() <= MAX_ID_CHARS
        && id.chars().all(name_char)
        && id.chars().any(char::is_alphanumeric)
        && id.trim() == id
        && !RESERVED.contains(&id.to_lowercase().as_str())
}

/// 一个名字照 [`id_ok`] 的写法改一遍：收不下的字换 `-`、截到 32 字、去首尾空白；改完还不成 ⇒ `None`。
pub(crate) fn id_from(name: &str) -> Option<String> {
    let s: String = name
        .chars()
        .map(|c| if name_char(c) { c } else { '-' })
        .take(MAX_ID_CHARS)
        .collect();
    let s = s.trim().to_string();
    id_ok(&s).then_some(s)
}

/// 一段字节的摘要（十六进制的 SHA-256）。
pub(crate) fn digest(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 内容版本。
pub(crate) fn rev_of(source: &str) -> String {
    format!("h:{}", &digest(source.as_bytes())[..12])
}

/// 本机策略目录：`<家>/strategies/`（家同 `rotation.json`）。
pub(crate) fn dir_from(rotation_path: &Path) -> Option<PathBuf> {
    rotation_path.parent().map(|p| p.join(DIR_NAME))
}

fn read_one(path: &Path) -> Result<String, Fail> {
    let gone = |m: String| Fail {
        kind: FailKind::Gone,
        line: None,
        col: None,
        msg: m,
        why: crate::accounts::quota::strategy::FailWhy::Gone,
    };
    match crate::platform::fs::Ro::metadata(path) {
        Ok(m) if m.len() > MAX_SOURCE_BYTES as u64 => {
            Err(gone(format!("file is over {MAX_SOURCE_BYTES} bytes")))
        }
        _ => crate::platform::fs::Ro::read_to_string(path).map_err(|e| gone(e.to_string())),
    }
}

/// 读一份原文成一项（读不懂的也成一项，带原因）。
pub(crate) fn shelve(id: &str, source: Result<String, Fail>) -> Shelved {
    match source {
        Ok(src) => Shelved {
            id: id.to_string(),
            rev: rev_of(&src),
            file: script::parse(&src),
            source: src,
        },
        Err(e) => Shelved {
            id: id.to_string(),
            rev: String::new(),
            source: String::new(),
            file: Err(e),
        },
    }
}

/// 目录的戳：每个文件的名 · 改动时刻 · 长度（变了才重读）。
type DirStamp = Vec<(String, Option<std::time::SystemTime>, u64)>;

fn dir_stamp(dir: &Path) -> DirStamp {
    let Ok(rd) = crate::platform::fs::Ro::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: DirStamp = rd
        .filter_map(Result::ok)
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((
                e.file_name().to_str()?.to_string(),
                m.modified().ok(),
                m.len(),
            ))
        })
        .collect();
    v.sort();
    v
}

fn load(dir: &Path) -> Vec<Arc<Shelved>> {
    let Ok(rd) = crate::platform::fs::Ro::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<(String, PathBuf)> = rd
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("rhai") {
                return None;
            }
            let id = p.file_stem()?.to_str()?.to_string();
            id_ok(&id).then_some((id, p))
        })
        .collect();
    names.sort();
    names.truncate(MAX_FILES);
    names
        .into_iter()
        .map(|(id, p)| Arc::new(shelve(&id, read_one(&p))))
        .collect()
}

/// ★ 此刻目录里的全部（按 id 排；目录的戳变了才重读）。`dir = None` ⇒ 家推不出，一份都没有。
pub(crate) fn all(dir: Option<&Path>) -> Arc<Vec<Arc<Shelved>>> {
    type DirSeen = std::collections::BTreeMap<PathBuf, (DirStamp, Arc<Vec<Arc<Shelved>>>)>;
    static SEEN: OnceLock<Mutex<DirSeen>> = OnceLock::new();
    let Some(dir) = dir else {
        return Arc::new(Vec::new());
    };
    let st = dir_stamp(dir);
    let cache = SEEN.get_or_init(|| Mutex::new(DirSeen::new()));
    let mut g = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((s, v)) = g.get(dir) {
        if *s == st {
            return v.clone();
        }
    }
    let v = Arc::new(load(dir));
    g.insert(dir.to_path_buf(), (st, v.clone()));
    v
}

/// 按 id 找一份。
pub(crate) fn find(all: &[Arc<Shelved>], id: &str) -> Option<Arc<Shelved>> {
    all.iter().find(|s| s.id == id).cloned()
}

/// `v3/` 里挪开的第三版脚本有几份（不加载）。
pub(crate) fn old_count(dir: Option<&Path>) -> usize {
    old_files(dir).len()
}

/// `v3/` 里挪开的第三版脚本（不加载），按文件名排。
pub(crate) fn old_files(dir: Option<&Path>) -> Vec<PathBuf> {
    let Some(dir) = dir else { return Vec::new() };
    let mut v: Vec<PathBuf> = crate::platform::fs::Ro::read_dir(&dir.join(OLD_DIR))
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("rhai"))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// 一份策略在目录里的文件（id 已照 [`id_ok`] 过，拼不出目录外）。
pub(crate) fn path_of(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.rhai"))
}

/// 帧面那一路写一份策略文件：`<家>/strategies/<id>.rhai`（目录只建这一层，整份原子写）。读不读得懂由调用方先判。
pub(crate) fn write_file(
    own: &crate::platform::fs::Own,
    id: &str,
    source: &str,
) -> Result<PathBuf, crate::common::said::Said> {
    use crate::accounts::quota::rotation::io_said;
    let dir = own.path(DIR_NAME).map_err(|_| {
        crate::common::said::Said::from(copy_core::copy_text("beRotation.read.noHome", &[]))
    })?;
    own.ensure_dir(&dir)
        .map_err(|e| io_said(dir.as_path(), &e))?;
    let file = dir.join(&format!("{id}.rhai"))?;
    let held = own.lock(&dir)?;
    own.write(&file, &held, source.as_bytes())?;
    let nodes = crate::accounts::quota::nodes::bus();
    crate::accounts::quota::nodes::emit(
        nodes,
        crate::accounts::quota::nodes::Source::Strategies,
        None,
    );
    Ok(file.as_path().to_path_buf())
}

/// 帧面那一路删一份策略文件（只删策略目录里的那一份）。
pub(crate) fn remove_file(
    own: &crate::platform::fs::Own,
    id: &str,
) -> Result<(), crate::common::said::Said> {
    use crate::accounts::quota::rotation::io_said;
    let dir = own.path(DIR_NAME).map_err(|_| {
        crate::common::said::Said::from(copy_core::copy_text("beRotation.read.noHome", &[]))
    })?;
    let file = dir.join(&format!("{id}.rhai"))?;
    own.delete_file(&file)
        .map_err(|e| io_said(file.as_path(), &e))?;
    let nodes = crate::accounts::quota::nodes::bus();
    crate::accounts::quota::nodes::emit(
        nodes,
        crate::accounts::quota::nodes::Source::Strategies,
        None,
    );
    Ok(())
}
