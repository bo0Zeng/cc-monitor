//! **skill 装记录** —— 从别的机器「装到这台」的 skill，装时写进了哪几个文件（第四层，后端自有状态）。
//!
//! # 要求
//!
//! 从别的机器「装到这台」的 skill 要能卸 —— 装的时候记下写了哪些文件，
//! 卸只删这些（装完用户自己改过的先问），足迹里看得见。
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
//! # 写口（全仓一个：[`answer_record`]，只从 `stream/inbound/` 进 —— `readonly_guard` 第四层 ④）
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

use copy_core::copy_text;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// 文件名（与 `backend.json` / `assets-catalog.json` 同一个家）。字面量只住契约 crate（`relay_route_core::SKILL_LEDGER_REL`，数据位置页按它列），写者只有本模块（判据钉）。
pub const FILE_NAME: &str = relay_route_core::file_name_of(relay_route_core::SKILL_LEDGER_REL);
/// 格式版本。读到更大的 ⇒ 不覆盖。
pub const FORMAT_V: u64 = 1;
/// 读一份记录的上限。一个文件一条约 100 字节 ⇒ 4 MiB 装得下四万个文件；超了当读不懂（不拿截断的当完整的）。
pub const MAX_BYTES: u64 = 4 * 1024 * 1024;
/// 一趟 `add` 最多收这么多个文件（与 skill 读 / 装同一个上限，不另起一份）。
use crate::assets::asset_catalog::SKILL_MAX_FILES as MAX_FILES;

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
    /// MCP 装记录：配置文件的绝对路径 → server 名 → 装进去的那一条的摘要（[`mcp_digest`]）。
    /// 没有一条时不写这一格（旧文件与新文件同形）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mcp: BTreeMap<String, BTreeMap<String, String>>,
}

impl Default for Ledger {
    fn default() -> Self {
        Ledger {
            v: FORMAT_V,
            installs: BTreeMap::new(),
            mcp: BTreeMap::new(),
        }
    }
}

/// 一条 MCP 定义的摘要（键序无关：按规范写法算）。装的时候记、卸的时候比。
pub fn mcp_digest(def: &Value) -> String {
    digest_of(&crate::assets::asset_catalog::canonical(def))
}

/// 一段原文的摘要（装时记、卸时比，两处同一个函数）。**纯**。
pub fn digest_of(text: &str) -> String {
    let mut f = crate::assets::asset_catalog::Fnv::default();
    f.part(text.as_bytes());
    f.hex()
}

fn valid_digest(d: &str) -> bool {
    d.len() == 16 && d.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// 这台机器上记录文件的路径。家目录：`HOME`，没有再退 `USERPROFILE`；都没有 ⇒ `None`（不猜）。
pub fn ledger_path() -> Option<PathBuf> {
    let home = crate::platform::paths::home_dir()?;
    Some(
        home.join(crate::control::exit_policy::DIR_NAME)
            .join(FILE_NAME),
    )
}

/// 读一次。三态：没有（还没装过）/ 读得懂 / 读不懂（**不覆盖**）。
pub type Read = crate::common::own_state::Read<Ledger>;

pub fn read_at(path: &Path) -> Read {
    crate::common::own_state::read_bytes(path, MAX_BYTES).and_then(|bytes| {
        // 先只看版本：更新的格式整份不认（它可能多了这一版不认得的格子），也不覆盖。
        let v = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|j| j.get("v").and_then(Value::as_u64));
        if let Some(v) = v.filter(|v| *v != FORMAT_V) {
            return Read::Unreadable(
                copy_text(
                    "beSkillLedger.read.otherVersion",
                    &[
                        ("path", &path.display().to_string()),
                        ("mine", &FORMAT_V.to_string()),
                        ("theirs", &v.to_string()),
                    ],
                )
                .into(),
            );
        }
        match serde_json::from_slice::<Ledger>(&bytes) {
            Ok(l) => Read::Present(l),
            Err(e) => Read::Unreadable(crate::common::said::Said::with_raw(
                copy_text(
                    "beSkillLedger.read.unknown",
                    &[("path", &path.display().to_string())],
                ),
                e,
            )),
        }
    })
}

/// 读成一份可用的记录：没有 ⇒ 空的；读不懂 ⇒ `ledger_unreadable`（读的人也不许把它说成「什么都没装过」）。
pub fn load_at(path: &Path) -> Result<Ledger, (&'static str, String)> {
    match read_at(path) {
        Read::Absent => Ok(Ledger::default()),
        Read::Present(l) => Ok(l),
        Read::Unreadable(why) => Err(("ledger_unreadable", why.said_logging_raw())),
    }
}

/// **全仓唯一的写者**（经 `own_state` 原子写；那一层目录由 [`record_at`] 在拿锁之前建）。
fn write_at(path: &Path, ledger: &Ledger) -> Result<(), crate::common::said::Said> {
    crate::common::own_state::write_json(path, ledger)
}

//
// 读—改—写整段在那个目录的跨进程锁里（`platform/lock.rs`，[`record_at`] 开头拿）：两个后端进程同时记装记录时，
// 后写的整份会盖掉先写的一条 ⇒ 那一趟装的文件从此卸不掉。

/// `add` 的入参：`files: {path: {digest, created}}`。
fn files_arg(args: &Value) -> Result<BTreeMap<String, Recorded>, (&'static str, String)> {
    let obj = args.get("files").and_then(Value::as_object).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `files` or it is not an object").to_string(),
    ))?;
    if obj.is_empty() {
        return Err((
            "bad_args",
            crate::common::contract::malformed("`files` is empty"),
        ));
    }
    if obj.len() > MAX_FILES {
        return Err((
            "too_large",
            copy_text(
                "beSkillLedger.add.tooMany",
                &[("max", &MAX_FILES.to_string())],
            ),
        ));
    }
    let mut out = BTreeMap::new();
    for (path, rec) in obj {
        if !crate::assets::skill_install::valid_rel(path) {
            return Err((
                "bad_args",
                crate::common::contract::malformed(&format!(
                    "{path:?} is not a relative path inside the skill"
                )),
            ));
        }
        let rec: Recorded = serde_json::from_value(rec.clone()).map_err(|e| {
            (
                "bad_args",
                crate::common::contract::malformed(&format!(
                    "entry {path:?} must be {{digest, created}}: {e}"
                )),
            )
        })?;
        if !valid_digest(&rec.digest) {
            return Err((
                "bad_args",
                crate::common::contract::malformed(&format!(
                    "`digest` of {path:?} is not 16 lowercase hex digits"
                )),
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
        copy_text(
            "beSkillLedger.drop.notRecorded",
            &[("dir", &dir.to_string())],
        ),
    ))?;
    if let Some(p) = paths.iter().find(|p| !entry.files.contains_key(*p)) {
        return Err((
            "bad_args",
            crate::common::contract::malformed(&format!("{p:?} is not recorded under {dir}")),
        ));
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
    let op = args.get("op").and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `op` (add, drop, mcp-add or mcp-drop)"),
    ))?;
    // 先建那一层目录（在就算了），再拿它的跨进程锁，锁住之后才读。
    let lock_dir = path.parent().ok_or((
        "io_failed",
        copy_text(
            "beSkillLedger.write.noParent",
            &[("path", &path.display().to_string())],
        ),
    ))?;
    // 只建那一层、建的那一下就是 0700（`own_dir`：后端建自家目录的那一个函数）。
    crate::common::own_dir::ensure_private_dir(lock_dir).map_err(|e| {
        (
            "io_failed",
            copy_text(
                "beSkillLedger.write.mkdirFailed",
                &[
                    ("dir", &lock_dir.display().to_string()),
                    ("e", &e.to_string()),
                ],
            ),
        )
    })?;
    let _g = crate::platform::lock::hold(lock_dir).map_err(|e| {
        (
            "io_failed",
            crate::common::said::Said::from(e).said_logging_raw(),
        )
    })?;
    let mut ledger = load_at(path)?;
    let (dir, name, changed, left) = match op {
        "add" => {
            let name = args
                .get("name")
                .and_then(Value::as_str)
                .filter(|n| crate::assets::skill_install::valid_name(n))
                .ok_or((
                    "bad_args",
                    crate::common::contract::malformed(
                        "missing `name` or it is not a valid skill directory name",
                    ),
                ))?
                .to_string();
            let files = files_arg(args)?;
            // 目录自己算：与 `skill-install-plan` 答 `dir` 的是同一个根（不收调用方给的路径）。
            // `at: "home"`（闭集，只此一个值）：装的东西落在家目录底下、不在 skill 根下
            //   ⇒ 键 = 本记录自己所在的那个家（`<家>/.cc-monitor/<本文件>` 的上两层），
            //   `files` 的路径相对它。同一份账、同一个形（不另立第二份账）。
            let project = args.get("project").and_then(Value::as_str);
            let dir = match args.get("at").and_then(Value::as_str) {
                // 项目级 skill：根由适配层按那个项目算（同 `skill-install-plan` 答 `dir` 的那一处）。
                None if project.is_some() => {
                    let p = crate::assets::mcp_edit::project_root(project.unwrap_or_default())?;
                    super::asset_kind()
                        .and_then(|k| crate::agents::skill_root_at(k, Some(Path::new(&p))))
                        .ok_or(("io_failed", copy_text("beSkillLedger.add.noRoot", &[])))?
                        .join(&name)
                        .display()
                        .to_string()
                }
                None => {
                    let root = match skills_root {
                        Some(r) => r.to_path_buf(),
                        None => super::asset_kind()
                            .and_then(crate::agents::skills_root)
                            .ok_or(("io_failed", copy_text("beSkillLedger.add.noRoot", &[])))?,
                    };
                    root.join(&name).display().to_string()
                }
                Some("home") => path
                    .parent()
                    .and_then(Path::parent)
                    .ok_or(("io_failed", copy_text("beSkillLedger.add.noRoot", &[])))?
                    .display()
                    .to_string(),
                Some(other) => {
                    return Err((
                        "bad_args",
                        crate::common::contract::malformed(&format!(
                            "`at` must be absent or \"home\", got {other:?}"
                        )),
                    ))
                }
            };
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
                    crate::common::contract::malformed("missing `dir`"),
                ))?
                .to_string();
            let paths: Vec<String> = args
                .get("paths")
                .and_then(Value::as_array)
                .ok_or((
                    "bad_args",
                    crate::common::contract::malformed("missing `paths` or it is not an array"),
                ))?
                .iter()
                .map(|p| {
                    p.as_str().map(str::to_string).ok_or((
                        "bad_args",
                        crate::common::contract::malformed("`paths` must contain only strings"),
                    ))
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
        "mcp-add" | "mcp-drop" => {
            let file = args
                .get("file")
                .and_then(Value::as_str)
                .filter(|f| Path::new(f).is_absolute())
                .ok_or((
                    "bad_args",
                    crate::common::contract::malformed("missing `file` (an absolute path)"),
                ))?
                .to_string();
            let name = args
                .get("name")
                .and_then(Value::as_str)
                .filter(|n| !n.trim().is_empty())
                .ok_or((
                    "bad_args",
                    crate::common::contract::malformed("missing `name`"),
                ))?
                .to_string();
            let changed = if op == "mcp-add" {
                let digest = args
                    .get("digest")
                    .and_then(Value::as_str)
                    .filter(|d| valid_digest(d))
                    .ok_or((
                        "bad_args",
                        crate::common::contract::malformed(
                            "`digest` must be 16 lowercase hex digits",
                        ),
                    ))?;
                let slot = ledger.mcp.entry(file.clone()).or_default();
                slot.insert(name.clone(), digest.to_string()) != Some(digest.to_string())
            } else {
                let gone = ledger
                    .mcp
                    .get_mut(&file)
                    .and_then(|m| m.remove(&name))
                    .is_some();
                if ledger.mcp.get(&file).is_some_and(BTreeMap::is_empty) {
                    ledger.mcp.remove(&file);
                }
                gone
            };
            let left = ledger.mcp.get(&file).map_or(0, BTreeMap::len);
            (file, name, changed, left)
        }
        other => {
            return Err((
                "bad_args",
                crate::common::contract::malformed(&format!(
                    "`op` must be add, drop, mcp-add or mcp-drop, got {other:?}"
                )),
            ))
        }
    };
    if changed {
        write_at(path, &ledger).map_err(|e| ("io_failed", e.said_logging_raw()))?;
    }
    Ok(json!({ "dir": dir, "name": name, "changed": changed, "remaining": left }))
}

/// `skill-install-record`：帧面入口（**写口**，只从 `stream/inbound/` 进）。
pub fn answer_record(args: &Value) -> Answer {
    let path = ledger_path().ok_or(("io_failed", copy_text("beSkillLedger.path.noHome", &[])))?;
    record_at(&path, None, args)
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/skill_ledger_tests.rs"]
mod tests;
