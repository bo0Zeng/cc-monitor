//! 〔SU1 · 第四波 4C · V116〕**skill 装记录** —— 从别的机器「装到这台」的 skill，装时写进了哪几个文件（第四层，后端自有状态）。
//!
//! # 用户裁决（逐字）
//!
//! V116〔选〕「**要，只删装时写进去的文件**」：从别的机器「装到这台」的 skill 要能卸 —— 装的时候记下写了哪些文件，
//! 卸只删这些（装完用户自己改过的先问），足迹里看得见。设计全文 `调研/第四波记录/SU1.md §1`。
//!
//! # 文件（`~/.cc-monitor/skill-installs.json`，与资产目录同一个家、同一族写法）
//!
//! `{"v":1,"installs":{"<skill 目录绝对路径>":{"name":"demo","files":{"<相对路径>":{"digest":"<16 位十六进制>","created":true}}}}}`
//!
//! - **键是目录的绝对路径**：记的是「装时写进了哪几个文件」—— 落点是事实，名字只给人看。
//! - `digest`：**装时写进去的那一份原文**的摘要（`asset_catalog::Fnv`，只答相同 / 不同，**不防篡改**）。
//! - `created`：装之前这个路径不在 ⇒ `true`；装时盖掉了这台原有的一份 ⇒ `false`（卸它回不到装之前那一份 ⇒ 卸时要问）。
//! - 同一个目录再装一次：并进去 —— 新路径加进来、已记的换新摘要、`created` 取第一次的。
//!
//! # 写口（全仓一个：[`answer_record`]，只从 `inbound.rs` 进 —— `readonly_guard` 第四层 ④）
//!
//! - `{op:"add", name, files:{path:{digest, created}}}`：装完（或装到一半停下）monitor 把**真写成了的那几个**原样交回来
//!   （摘要与 `created` 是这台后端自己在 `skill-install-plan` 里答的 `ledger` 那一格）。目录由本模块按 `skills 根 / name` 自己算，
//!   **不收调用方给的路径**。
//! - `{op:"drop", dir, paths}`：卸掉（或已经不在）的那几个从记录里摘；一条记录剩零个文件 ⇒ 整条摘掉。
//! - 读不懂 / 更新的格式 ⇒ **不覆盖**、拒（`ledger_unreadable`）。没变不写。
//! - 第四层写法：`O_EXCL` 建临时文件 → 写满 → `sync` → 原子挪过去；只建 `~/.cc-monitor` 那一层；失败删自己的临时文件。
//!
//! # 它**不**做什么
//!
//! 一个用户文件都不写、不删（删经 monitor → 那台后端 `files-delete` 带 `expect`）。判定（卸哪几个、要不要问）住 `skill_install.rs`。
//!
//! # 买不到
//!
//! - 两个后端**进程**同时写（常驻那一个 ＋ 一次性 CLI）：进程内那把锁挡不住，后写的整份盖掉先写的那一次（同 `asset_catalog` 登记的那一条）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// 文件名（与 `backend.json` / `assets-catalog.json` 同一个家）。在全部生产代码里只有本模块这一个家（判据钉）。
pub const FILE_NAME: &str = "skill-installs.json";
/// 格式版本。读到更大的 ⇒ 不覆盖。
pub const FORMAT_V: u64 = 1;
/// 读一份记录的上限。一个文件一条约 100 字节 ⇒ 4 MiB 装得下四万个文件；超了当读不懂（不拿截断的当完整的）。
pub const MAX_BYTES: u64 = 4 * 1024 * 1024;
/// 一趟 `add` 最多收这么多个文件（与 skill 读 / 装同一个上限，不另起一份）。
use crate::asset_catalog::SKILL_MAX_FILES as MAX_FILES;

/// 这一族的应答。
pub type Answer = Result<Value, (&'static str, String)>;

/// 一个装时写进去的文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recorded {
    pub digest: String,
    pub created: bool,
}

/// 一个 skill 目录的记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Install {
    pub name: String,
    pub files: BTreeMap<String, Recorded>,
}

/// 整份文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ledger {
    pub v: u64,
    pub installs: BTreeMap<String, Install>,
}

impl Default for Ledger {
    fn default() -> Self {
        Ledger {
            v: FORMAT_V,
            installs: BTreeMap::new(),
        }
    }
}

/// 一段原文的摘要（装时记、卸时比，两处同一个函数）。**纯**。
pub fn digest_of(text: &str) -> String {
    let mut f = crate::asset_catalog::Fnv::default();
    f.part(text.as_bytes());
    f.hex()
}

fn valid_digest(d: &str) -> bool {
    d.len() == 16 && d.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// 这台机器上记录文件的路径。家目录：`HOME`，没有再退 `USERPROFILE`；都没有 ⇒ `None`（不猜）。
pub fn ledger_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").filter(|h| !h.is_empty()))?;
    Some(
        PathBuf::from(home)
            .join(crate::control::exit_policy::DIR_NAME)
            .join(FILE_NAME),
    )
}

/// 读一次。三态：没有（还没装过）/ 读得懂 / 读不懂（**不覆盖**）。
#[derive(Debug)]
pub enum Read {
    Absent,
    Ok(Ledger),
    Unreadable(String),
}

pub fn read_at(path: &Path) -> Read {
    let bytes = match crate::common::fs::read_regular_capped(path, MAX_BYTES) {
        Ok(b) => b,
        Err(_) if !path.exists() => return Read::Absent,
        Err(e) => return Read::Unreadable(format!("读 {} 失败：{e}", path.display())),
    };
    // 先只看版本：更新的格式整份不认（它可能多了这一版不认得的格子），也不覆盖。
    let v = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|j| j.get("v").and_then(Value::as_u64));
    if let Some(v) = v.filter(|v| *v != FORMAT_V) {
        return Read::Unreadable(format!(
            "{} 是另一个版本的后端写的（格式 {v}，这个后端只认 {FORMAT_V}），没有动它",
            path.display()
        ));
    }
    match serde_json::from_slice::<Ledger>(&bytes) {
        Ok(l) => Read::Ok(l),
        Err(e) => Read::Unreadable(format!(
            "{} 不是这个后端认得的 skill 装记录：{e}",
            path.display()
        )),
    }
}

/// 读成一份可用的记录：没有 ⇒ 空的；读不懂 ⇒ `ledger_unreadable`（读的人也不许把它说成「什么都没装过」）。
pub fn load_at(path: &Path) -> Result<Ledger, (&'static str, String)> {
    match read_at(path) {
        Read::Absent => Ok(Ledger::default()),
        Read::Ok(l) => Ok(l),
        Read::Unreadable(why) => Err(("ledger_unreadable", why)),
    }
}

/// **全仓唯一的写者**：`O_EXCL` 新建临时文件 → 写满 → `sync` → 原子挪过去；目录不在就建那一层；失败删临时文件。
fn write_at(path: &Path, ledger: &Ledger) -> Result<(), String> {
    use std::io::Write as _;
    let dir = path
        .parent()
        .ok_or_else(|| format!("{} 没有父目录", path.display()))?;
    if let Err(e) = std::fs::create_dir(dir) {
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(format!("建 {} 失败：{e}", dir.display()));
        }
    }
    let body = serde_json::to_string(ledger).map_err(|e| format!("装记录序列化失败：{e}"))?;
    let tmp = dir.join(format!("{FILE_NAME}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| format!("建临时文件 {} 失败：{e}", tmp.display()))?;
        f.write_all(body.as_bytes())
            .and_then(|()| f.write_all(b"\n"))
            .and_then(|()| f.sync_all())
            .map_err(|e| format!("写临时文件 {} 失败：{e}", tmp.display()))?;
        drop(f);
        std::fs::rename(&tmp, path)
            .map_err(|e| format!("把 {} 挪到 {} 失败：{e}", tmp.display(), path.display()))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

// 〔HX2 · 第四波 4D〕墓碑：这里从前是一把**进程内** `Mutex`（「同一进程里的读—改—写串起来」）。两个后端进程
//   （两台 monitor 各自连着这台时的两条远端流 ＋ 一次性 CLI）同时记装记录 ⇒ 后写的整份盖掉先写的一条，
//   **那一趟装的文件从此卸不掉、而且没人说**（审计 `E-compat.md` §E6）。今天读—改—写整段在那个目录的**跨进程**锁里
//   （`platform/lock.rs`，[`record_at`] 开头拿）。

/// `add` 的入参：`files: {path: {digest, created}}`。
fn files_arg(args: &Value) -> Result<BTreeMap<String, Recorded>, (&'static str, String)> {
    let obj = args.get("files").and_then(Value::as_object).ok_or((
        "bad_args",
        "少了 `files`（装时真写成了的那几个 `{路径: {digest, created}}`），或它不是对象"
            .to_string(),
    ))?;
    if obj.is_empty() {
        return Err(("bad_args", "`files` 是空的 —— 没有要记的".to_string()));
    }
    if obj.len() > MAX_FILES {
        return Err(("too_large", format!("一趟最多记 {MAX_FILES} 个文件")));
    }
    let mut out = BTreeMap::new();
    for (path, rec) in obj {
        if !crate::skill_install::valid_rel(path) {
            return Err(("bad_args", format!("「{path}」不是 skill 里的相对路径")));
        }
        let rec: Recorded = serde_json::from_value(rec.clone()).map_err(|e| {
            (
                "bad_args",
                format!("「{path}」那一格只收 `{{digest, created}}`：{e}"),
            )
        })?;
        if !valid_digest(&rec.digest) {
            return Err((
                "bad_args",
                format!("「{path}」的 `digest` 不是 16 位小写十六进制"),
            ));
        }
        out.insert(path.clone(), rec);
    }
    Ok(out)
}

/// `add`：并进那个目录的记录。回 `(目录, 变没变)`。**纯**（目录由调用方算好）。
pub fn add(ledger: &mut Ledger, dir: &str, name: &str, files: BTreeMap<String, Recorded>) -> bool {
    let entry = ledger
        .installs
        .entry(dir.to_string())
        .or_insert_with(|| Install {
            name: name.to_string(),
            files: BTreeMap::new(),
        });
    let before = entry.clone();
    entry.name = name.to_string();
    for (path, rec) in files {
        match entry.files.get_mut(&path) {
            // 已记的：换成这一次写进去的摘要；`created` 取第一次的（第一次是新建的，就一直算装进来的）。
            Some(old) => old.digest = rec.digest,
            None => {
                entry.files.insert(path, rec);
            }
        }
    }
    *entry != before
}

/// `drop`：摘掉那几个路径；剩零个 ⇒ 整条摘掉。回剩几个。路径不在记录里 ⇒ `Err`（不静默当成功）。**纯**。
pub fn drop_paths(
    ledger: &mut Ledger,
    dir: &str,
    paths: &[String],
) -> Result<usize, (&'static str, String)> {
    let entry = ledger.installs.get_mut(dir).ok_or((
        "not_found",
        format!("这台没有记着从别处装到 {dir} 的 skill"),
    ))?;
    if let Some(p) = paths.iter().find(|p| !entry.files.contains_key(*p)) {
        return Err(("bad_args", format!("「{p}」不在 {dir} 那一条记录里")));
    }
    for p in paths {
        entry.files.remove(p);
    }
    let left = entry.files.len();
    if left == 0 {
        ledger.installs.remove(dir);
    }
    Ok(left)
}

/// [`answer_record`] 的本体：记录文件路径与 skill 根都可喂（判据拿临时目录喂，不改进程环境）。
pub fn record_at(path: &Path, skills_root: Option<&Path>, args: &Value) -> Answer {
    let op = args
        .get("op")
        .and_then(Value::as_str)
        .ok_or(("bad_args", "少了 `op`（`add` 或 `drop`）".to_string()))?;
    // 〔HX2〕先建那一层目录（在就算了），再拿它的跨进程锁，锁住之后才读。
    let lock_dir = path
        .parent()
        .ok_or(("io_failed", format!("{} 没有父目录", path.display())))?;
    if let Err(e) = std::fs::create_dir(lock_dir) {
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(("io_failed", format!("建 {} 失败：{e}", lock_dir.display())));
        }
    }
    let _g = crate::platform::lock::hold(lock_dir).map_err(|e| ("io_failed", e))?;
    let mut ledger = load_at(path)?;
    let (dir, name, changed, left) = match op {
        "add" => {
            let name = args
                .get("name")
                .and_then(Value::as_str)
                .filter(|n| crate::skill_install::valid_name(n))
                .ok_or((
                    "bad_args",
                    "少了 `name`，或它不是一个能当 skill 目录名的名字".to_string(),
                ))?
                .to_string();
            let files = files_arg(args)?;
            // 目录自己算：与 `skill-install-plan` 答 `dir` 的是同一个根（不收调用方给的路径）。
            let root = match skills_root {
                Some(r) => r.to_path_buf(),
                None => crate::agents::skills_root().ok_or((
                    "io_failed",
                    "这台机器说不出 skill 的根在哪 —— 不猜一个路径去记".to_string(),
                ))?,
            };
            let dir = root.join(&name).display().to_string();
            let changed = add(&mut ledger, &dir, &name, files);
            let left = ledger.installs.get(&dir).map_or(0, |i| i.files.len());
            (dir, name, changed, left)
        }
        "drop" => {
            let dir = args
                .get("dir")
                .and_then(Value::as_str)
                .ok_or((
                    "bad_args",
                    "少了 `dir`（记录里那个 skill 目录）".to_string(),
                ))?
                .to_string();
            let paths: Vec<String> = args
                .get("paths")
                .and_then(Value::as_array)
                .ok_or(("bad_args", "少了 `paths`，或它不是数组".to_string()))?
                .iter()
                .map(|p| {
                    p.as_str()
                        .map(str::to_string)
                        .ok_or(("bad_args", "`paths` 里只收字符串".to_string()))
                })
                .collect::<Result<_, _>>()?;
            let name = ledger
                .installs
                .get(&dir)
                .map(|i| i.name.clone())
                .unwrap_or_default();
            let left = drop_paths(&mut ledger, &dir, &paths)?;
            (dir, name, !paths.is_empty(), left)
        }
        other => {
            return Err((
                "bad_args",
                format!("`op` 只收 `add` / `drop`，给的是「{other}」"),
            ))
        }
    };
    if changed {
        write_at(path, &ledger).map_err(|e| ("io_failed", e))?;
    }
    Ok(json!({ "dir": dir, "name": name, "changed": changed, "remaining": left }))
}

/// `skill-install-record`：帧面入口（**写口**，只从 `inbound.rs` 进）。
pub fn answer_record(args: &Value) -> Answer {
    let path = ledger_path().ok_or((
        "io_failed",
        "家目录解析不出来（HOME / USERPROFILE 都没有）—— 不猜一个路径去记".to_string(),
    ))?;
    record_at(&path, None, args)
}

#[cfg(test)]
#[path = "../../tests/backend/skill_ledger_tests.rs"]
mod tests;
