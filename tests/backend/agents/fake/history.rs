//! 替身家的**合成历史面**与它的子运行住址：会话不住记录树、子运行的记录与父记录平铺在同一个目录里
//! （不住父记录底下），是不是子运行、父是谁只写在记录开头那一行 —— 与 Claude（`<父>/subagents/…`）·
//! Codex（按日期分目录的 rollout）· 本假家的运行面（`<父>.lanes/…`，`runs.rs`）都不同形。
//! 历史清单的判据拿它跑：清单要是靠目录形状认子运行、而不是问 `ChildFace.owner`，这里的子运行就会混进清单。
//!
//! | 格 | 本替身 |
//! |---|---|
//! | 记录根 | 每个家两个：`<家>/chats/` 与归档根 `<家>/shelf/`，各平铺一层 |
//! | 归档 | 住 `shelf/` 底下 ⇒ 已归档；归档 · 取消归档 · 删都交替身程序（`fake/record.rs` 的 `cli_command`，带会话所在那个家） |
//! | 会话文件 | `chat-<sid>.ndjson` |
//! | 开头那一行 | `{"chat": <sid>, "dir": <cwd>, "said": <第一句>}`；子运行另带 `"under": <父 sid>` |

use crate::agents::{AccountHome, ChildFace, HistoryFace, RunFaces, SynthSession};
use crate::platform::fs::Ro;
use serde_json::Value;
use std::path::{Path, PathBuf};

const ROOT: &str = "chats";
const PREFIX: &str = "chat-";
const EXT: &str = "ndjson";

/// 这一家在清单里的名字（不进注册表，只在判据里递）。
pub const KIND: &str = "fake-chats";

/// `<家>/chats/`。
pub fn records_root(home: &Path) -> PathBuf {
    home.join(ROOT)
}

/// sid ⇒ 会话文件名。
pub fn file_name(sid: &str) -> String {
    format!("{PREFIX}{sid}.{EXT}")
}

/// 文件名 ⇒ sid（不是这一形 ⇒ `None`）。
fn sid_of(p: &Path) -> Option<&str> {
    p.file_name()?
        .to_str()?
        .strip_prefix(PREFIX)?
        .strip_suffix(&format!(".{EXT}"))
}

/// 开头那一行（读不出 ⇒ `None`）。
fn head(p: &Path) -> Option<Value> {
    let text = Ro::read_string_capped(p, crate::agents::fake::FAKE_FILE_CAP).ok()?;
    serde_json::from_str(text.lines().next()?).ok()
}

fn cell(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

/// 记录根里每一份会话文件（按名字排）。
fn files(root: &Path) -> Vec<PathBuf> {
    let Ok(rd) = Ro::read_dir(root) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && sid_of(p).is_some())
        .collect();
    v.sort();
    v
}

/// 各号的家底下每一份会话（父 · 子运行都列：认子运行是 `owner` 那一问的事，合成历史面只管列）。
fn sessions(homes: &[AccountHome]) -> Vec<SynthSession> {
    homes
        .iter()
        .flat_map(|h| {
            roots(std::slice::from_ref(h))
                .into_iter()
                .flat_map(|r| files(&r))
                .map(|p| {
                    let v = head(&p).unwrap_or_default();
                    SynthSession {
                        sid: cell(&v, "chat").unwrap_or_default(),
                        cwd: cell(&v, "dir").unwrap_or_default(),
                        mtime_ms: Ro::mtime_ms(&p),
                        path: p,
                    }
                })
        })
        .collect()
}

fn excerpt(p: &Path) -> String {
    head(p).and_then(|v| cell(&v, "said")).unwrap_or_default()
}

/// 本替身的合成历史面。
pub const HISTORY: HistoryFace = HistoryFace {
    sessions,
    excerpt,
    roots,
};

/// 开头写着 `under` ⇒ 同目录里它父那一份该在的路径（只看形状，不管在不在）。
fn under(p: &Path) -> Option<PathBuf> {
    sid_of(p)?;
    let parent = cell(&head(p)?, "under")?;
    Some(p.with_file_name(file_name(&parent)))
}

/// 这一份若是子运行、它父那一份还在盘上 ⇒ 那一份；不是子运行 · 父已不在（孤儿）⇒ `None`。
fn owner(p: &Path) -> Option<PathBuf> {
    under(p).filter(|parent| parent.is_file())
}

/// 父那一份 ⇒ 同目录里开头写着它的那几份。
fn sources(parent: &Path) -> Vec<PathBuf> {
    let Some(dir) = parent.parent() else {
        return Vec::new();
    };
    files(dir)
        .into_iter()
        .filter(|p| owner(p).as_deref() == Some(parent))
        .collect()
}

/// 记录根底下 id 为这个的子运行。
fn find(root: &Path, child_id: &str) -> Option<PathBuf> {
    let p = root.join(file_name(child_id));
    under(&p).is_some().then_some(p)
}

/// 父记录里说到子运行的行本替身不写，多判无妨 ⇒ 每一行都算。
fn hint(_: &str) -> bool {
    true
}

/// 记录里不写时刻。
fn written(_: &Value) -> Option<u64> {
    None
}

/// 派出那一刻不落盘（`ChildFace.spawned` 可答可不答）。
fn spawned(_: &Path) -> Option<crate::agents::ChildLink> {
    None
}

/// 本替身的运行面：只有子运行住址那一格。
pub const RUNS: RunFaces = RunFaces {
    children: Some(ChildFace {
        sources,
        owner,
        find,
        hint,
        written,
        spawned,
    }),
    ..RunFaces::NONE
};

/// 归档根：`<家>/shelf/`，平铺一层（与日期树那一根同一个家的第二个根）。
pub fn shelf_root(home: &Path) -> PathBuf {
    home.join(SHELF)
}

const SHELF: &str = "shelf";

/// 各号的家 ⇒ 每个家两个根：`chats/` 与归档根 `shelf/`。
fn roots(homes: &[AccountHome]) -> Vec<PathBuf> {
    homes
        .iter()
        .flat_map(|h| [records_root(&h.home), shelf_root(&h.home)])
        .collect()
}

/// 按 sid 找：那一根底下平铺的 `chat-<sid>.ndjson`。
fn find_session(root: &Path, sid: &str) -> Result<PathBuf, String> {
    let p = root.join(file_name(sid));
    if p.is_file() {
        Ok(p)
    } else {
        Err(format!("fake-chats: no {sid} under {}", root.display()))
    }
}

/// 住归档根底下 ⇒ 已归档。
fn archived(p: &Path) -> bool {
    p.parent()
        .and_then(Path::file_name)
        .is_some_and(|n| n == SHELF)
}

/// 那个家里 `sid` 那一份（两根之一）。
fn found_in(home: &Path, sid: &str) -> Option<PathBuf> {
    roots(&[AccountHome {
        account: None,
        home: home.to_path_buf(),
    }])
    .iter()
    .find_map(|r| find_session(r, sid).ok())
}

/// 删会话交命令那一形：那个家里两根之一有这个 sid ⇒ 删它的那一条（带那个家）。
fn delete_command(home: &Path, sid: &str) -> Option<crate::agents::DeleteCommand> {
    let record = found_in(home, sid)?;
    let cli = super::super::record::cli_command("forget", home, sid);
    Some(crate::agents::DeleteCommand {
        program: cli.program,
        args: cli.args,
        env: cli.env,
        record,
    })
}

/// 本替身的记录面：两根平铺 · 归档照住在哪一根认 · 删与归档都交替身程序（带会话所在那个家）。
pub fn records() -> crate::agents::RecordFace {
    crate::agents::RecordFace {
        sid: |p| sid_of(p).map(str::to_string),
        is_session_file: |p| sid_of(p).is_some(),
        tree: None,
        find_session,
        delete: Some(crate::agents::SessionDelete::Command(delete_command)),
        archive: Some(crate::agents::ArchiveFace {
            // 没归档的才出归档、归档了的才出取消归档（那个家里那一份住哪一根认）。
            archive: |home, sid| {
                let p = found_in(home, sid)?;
                (!archived(&p)).then(|| super::super::record::cli_command("shelve", home, sid))
            },
            unarchive: |home, sid| {
                let p = found_in(home, sid)?;
                archived(&p).then(|| super::super::record::cli_command("unshelve", home, sid))
            },
            archived,
        }),
        ..super::super::full_row(|| None)
            .records
            .expect("替身家有记录面")
    }
}

/// 本替身那一行：合成历史面 ＋ 记录面，`home` 是账号 0 的家。
pub fn row(home: fn() -> Option<PathBuf>) -> crate::agents::Adapter {
    crate::agents::Adapter {
        kind: KIND,
        history: Some(HISTORY),
        records: Some(records()),
        ..super::super::bare_row(home)
    }
}
