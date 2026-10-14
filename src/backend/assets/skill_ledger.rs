//! **skill 装记录** —— 从别的机器「装到这台」的 skill，装时写进了哪几个文件（后端自有状态，经 `Own`）。
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
//! - `digest`：**装时写进去的那一份原文**的摘要（`common::digest::Fnv`，只答相同 / 不同，**不防篡改**）。
//! - `created`：装之前这个路径不在 ⇒ `true`；装时盖掉了这台原有的一份 ⇒ `false`（卸它回不到装之前那一份 ⇒ 卸时要问）。
//! - 同一个目录再装一次：并进去 —— 新路径加进来、已记的换新摘要、`created` 取第一次的。
//!
//! # 写口（全仓一个：[`answer_record`]，只从 `stream/inbound/` 进 —— `Own` 由分派那一层递）
//!
//! - `{op:"add", name, files:{path:{digest, created}}}`：装完（或装到一半停下）monitor 把**真写成了的那几个**原样交回来
//!   （摘要与 `created` 是这台后端自己在 `skill-install-plan` 里答的 `ledger` 那一格）。目录由本模块按 `skills 根 / name` 自己算，
//!   **不收调用方给的路径**。
//! - `{op:"drop", dir, paths}`：卸掉（或已经不在）的那几个从记录里摘；一条记录剩零个文件 ⇒ 整条摘掉。
//! - 读不懂 / 更新的格式 ⇒ **不覆盖**、拒（`ledger_unreadable`）。没变不写。
//! - 原子写（`Own::write`）：`O_EXCL` 建临时文件 → 写满 → `sync` → 原子挪过去；只建 `~/.cc-monitor` 那一层；失败删自己的临时文件。
//!
//! # 它**不**做什么
//!
//! 一个用户文件都不写、不删（删经 monitor → 那台后端 `files-delete` 带 `expect`）。判定（卸哪几个、要不要问）住 `skill_install.rs`。
//!
//! # 买不到
//!
//! - 两个后端**进程**同时写（常驻那一个 ＋ 一次性 CLI）：进程内那把锁挡不住，后写的整份盖掉先写的那一次（同 `asset_catalog` 登记的那一条）。

use crate::platform::fs::{Own, OwnLock, OwnPath};
use copy_core::copy_text;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 文件名（与 `backend.json` / `assets-catalog.json` 同一个家）。字面量只住契约 crate（`relay_route_core::SKILL_LEDGER_REL`，数据位置页按它列），写者只有本模块（判据钉）。
pub const FILE_NAME: &str = relay_route_core::file_name_of(relay_route_core::SKILL_LEDGER_REL);
/// 格式版本。读到更大的 ⇒ 不覆盖。
pub const FORMAT_V: u64 = 1;
/// 读一份记录的上限。一个文件一条约 100 字节 ⇒ 4 MiB 装得下四万个文件；超了当读不懂（不拿截断的当完整的）。
pub const MAX_BYTES: u64 = 4 * 1024 * 1024;
/// 一趟 `add` 最多收这么多个文件（与 skill 读 / 装同一个上限，不另起一份）。
use crate::assets::asset_catalog::SKILL_MAX_FILES as MAX_FILES;

/// 一个装时写进去的文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "LedgerFile"
    )
)]
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
    crate::common::digest::fnv_digest(&crate::assets::asset_catalog::canonical(def))
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
pub fn load_at(path: &Path) -> Result<Ledger, crate::stream::inbound::spec::Fail> {
    match read_at(path) {
        Read::Absent => Ok(Ledger::default()),
        Read::Present(l) => Ok(l),
        Read::Unreadable(why) => Err(crate::stream::inbound::spec::Fail::from((
            "ledger_unreadable",
            why,
        ))),
    }
}

/// **全仓唯一的写者**（经 `Own` 原子写；那一层目录由 [`record_at`] 在拿锁之前建）。
fn write_at(
    own: &Own,
    file: &OwnPath,
    held: &OwnLock,
    ledger: &Ledger,
) -> Result<(), crate::common::said::Said> {
    let body = crate::common::own_state::json_body(file.as_path(), ledger)?;
    own.write(file, held, &body)
}

//
// 读—改—写整段在那个目录的跨进程锁里（`platform/lock.rs`，[`record_at`] 开头拿）：两个后端进程同时记装记录时，
// 后写的整份会盖掉先写的一条 ⇒ 那一趟装的文件从此卸不掉。

/// `add` 的 `files`：非空、不超上限、每个路径都在 skill 里、摘要是 16 位小写十六进制。
fn check_files(
    files: &BTreeMap<String, Recorded>,
) -> Result<(), crate::stream::inbound::spec::Fail> {
    if files.is_empty() {
        return Err(crate::stream::inbound::spec::Fail::from((
            "bad_args",
            crate::common::contract::malformed("`files` is empty"),
        )));
    }
    if files.len() > MAX_FILES {
        return Err(crate::stream::inbound::spec::Fail::from((
            "too_large",
            copy_text(
                "beSkillLedger.add.tooMany",
                &[("max", &MAX_FILES.to_string())],
            ),
        )));
    }
    for (path, rec) in files {
        if !crate::assets::skill_install::valid_rel(path) {
            return Err(crate::stream::inbound::spec::Fail::from((
                "bad_args",
                crate::common::contract::malformed(&format!(
                    "{path:?} is not a relative path inside the skill"
                )),
            )));
        }
        if !valid_digest(&rec.digest) {
            return Err(crate::stream::inbound::spec::Fail::from((
                "bad_args",
                crate::common::contract::malformed(&format!(
                    "`digest` of {path:?} is not 16 lowercase hex digits"
                )),
            )));
        }
    }
    Ok(())
}

/// `skill-install-record` 的入参：按 `op` 分（多一个键 ⇒ `bad_args`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum RecordReq {
    /// 装完记：`files` = `skill-install-plan` 答的 `ledger` 里真写成了的那几个。
    Add {
        /// skill 的目录名。
        name: String,
        files: BTreeMap<String, Recorded>,
        /// 给了 ⇒ 目录按那个项目里的 skill 根算。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        project: Option<String>,
        /// 缺 ⇒ 目录按 skill 根算；`home` ⇒ 目录 = 记录所在那个家目录（装在家目录底下、不在 skill 根下的东西用）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        at: Option<RecordAt>,
    },
    /// 卸掉 / 已经不在的摘掉：`paths` 不在记录里 ⇒ `bad_args`，一个字节不动；摘到零个 ⇒ 整条记录摘掉。
    Drop { dir: String, paths: Vec<String> },
    /// MCP 装完记：`file` 配置文件的绝对路径（键）· 装进去的那一条的摘要。
    McpAdd {
        file: String,
        name: String,
        digest: String,
    },
    /// MCP 卸掉摘。
    McpDrop { file: String, name: String },
}

/// `add` 的 `at`（闭集，只此一个值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub(crate) enum RecordAt {
    Home,
}

/// `skill-install-record` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RecordReply {
    /// 这一条记录的目录（MCP：那份配置文件）。
    pub dir: String,
    pub name: String,
    /// 记录变没变（没变不写）。
    pub changed: bool,
    /// 这一条还剩几个文件（MCP：那份配置里还记着几条）。
    pub remaining: usize,
}

/// 装记录的写口（生产 = [`answer_record`]，由 `stream/inbound/` 递进来；装 · 卸那几处不直呼它）。
pub(crate) type Record<'a> =
    &'a dyn Fn(&RecordReq) -> Result<RecordReply, crate::stream::inbound::spec::Fail>;

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
) -> Result<usize, crate::stream::inbound::spec::Fail> {
    let entry = ledger.installs.get_mut(dir).ok_or((
        "not_found",
        copy_text(
            "beSkillLedger.drop.notRecorded",
            &[("dir", &dir.to_string())],
        ),
    ))?;
    if let Some(p) = paths.iter().find(|p| !entry.files.contains_key(*p)) {
        return Err(crate::stream::inbound::spec::Fail::from((
            "bad_args",
            crate::common::contract::malformed(&format!("{p:?} is not recorded under {dir}")),
        )));
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
pub(crate) fn record_at(
    own: &Own,
    skills_root: Option<&Path>,
    req: &RecordReq,
) -> Result<RecordReply, crate::stream::inbound::spec::Fail> {
    // 先建那一层目录（在就算了），再拿它的跨进程锁，锁住之后才读。
    let no_home = |_| {
        crate::stream::inbound::spec::Fail::from((
            "io_failed",
            copy_text("beSkillLedger.path.noHome", &[]),
        ))
    };
    let lock_dir = own.home_dir().map_err(no_home)?;
    let file = own
        .at_home(relay_route_core::SKILL_LEDGER_REL)
        .map_err(no_home)?;
    let path = file.as_path();
    // 只建那一层、建的那一下就是 0700。
    own.ensure_dir(&lock_dir).map_err(|e| {
        (
            "io_failed",
            crate::common::said::Said::with_raw(
                copy_text(
                    "beSkillLedger.write.mkdirFailed",
                    &[
                        ("dir", &lock_dir.as_path().display().to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                &e,
            ),
        )
    })?;
    let held = own.lock(&lock_dir).map_err(|e| {
        crate::stream::inbound::spec::Fail::from(("io_failed", crate::common::said::Said::from(e)))
    })?;
    let mut ledger = load_at(path)?;
    let (dir, name, changed, left) = match req {
        RecordReq::Add {
            name,
            files,
            project,
            at,
        } => {
            if !crate::assets::skill_install::valid_name(name) {
                return Err(crate::stream::inbound::spec::Fail::from((
                    "bad_args",
                    crate::common::contract::malformed(
                        "`name` is not a valid skill directory name",
                    ),
                )));
            }
            check_files(files)?;
            // 目录自己算：与 `skill-install-plan` 答 `dir` 的是同一个根（不收调用方给的路径）。
            // `at: "home"`：装的东西落在家目录底下、不在 skill 根下
            //   ⇒ 键 = 本记录自己所在的那个家（`<家>/.cc-monitor/<本文件>` 的上两层），
            //   `files` 的路径相对它。同一份账、同一个形（不另立第二份账）。
            let dir = match (at, project) {
                (Some(RecordAt::Home), _) => path
                    .parent()
                    .and_then(Path::parent)
                    .ok_or(("io_failed", copy_text("beSkillLedger.add.noRoot", &[])))?
                    .display()
                    .to_string(),
                // 项目级 skill：根由适配层按那个项目算（同 `skill-install-plan` 答 `dir` 的那一处）。
                (None, Some(p)) => {
                    let p = crate::assets::mcp_edit::project_root(p)?;
                    crate::agents::asset_kind()
                        .and_then(|k| crate::agents::skill_root_at(k, Some(Path::new(&p))))
                        .ok_or(("io_failed", copy_text("beSkillLedger.add.noRoot", &[])))?
                        .join(name)
                        .display()
                        .to_string()
                }
                (None, None) => {
                    let root = match skills_root {
                        Some(r) => r.to_path_buf(),
                        None => crate::agents::asset_kind()
                            .and_then(crate::agents::skills_root)
                            .ok_or(("io_failed", copy_text("beSkillLedger.add.noRoot", &[])))?,
                    };
                    root.join(name).display().to_string()
                }
            };
            let changed = add(&mut ledger, &dir, name, files.clone());
            let left = ledger.installs.get(&dir).map_or(0, |i| i.files.len());
            (dir, name.clone(), changed, left)
        }
        RecordReq::Drop { dir, paths } => {
            let name = ledger
                .installs
                .get(dir)
                .map(|i| i.name.clone())
                .unwrap_or_default();
            let left = drop_paths(&mut ledger, dir, paths)?;
            (dir.clone(), name, !paths.is_empty(), left)
        }
        RecordReq::McpAdd { file, name, .. } | RecordReq::McpDrop { file, name } => {
            if !Path::new(file).is_absolute() {
                return Err(crate::stream::inbound::spec::Fail::from((
                    "bad_args",
                    crate::common::contract::malformed("`file` is not an absolute path"),
                )));
            }
            if name.trim().is_empty() {
                return Err(crate::stream::inbound::spec::Fail::from((
                    "bad_args",
                    crate::common::contract::malformed("`name` is empty"),
                )));
            }
            let changed = if let RecordReq::McpAdd { digest, .. } = req {
                if !valid_digest(digest) {
                    return Err(crate::stream::inbound::spec::Fail::from((
                        "bad_args",
                        crate::common::contract::malformed(
                            "`digest` must be 16 lowercase hex digits",
                        ),
                    )));
                }
                let slot = ledger.mcp.entry(file.clone()).or_default();
                slot.insert(name.clone(), digest.clone()) != Some(digest.clone())
            } else {
                let gone = ledger
                    .mcp
                    .get_mut(file)
                    .and_then(|m| m.remove(name))
                    .is_some();
                if ledger.mcp.get(file).is_some_and(BTreeMap::is_empty) {
                    ledger.mcp.remove(file);
                }
                gone
            };
            let left = ledger.mcp.get(file).map_or(0, BTreeMap::len);
            (file.clone(), name.clone(), changed, left)
        }
    };
    if changed {
        write_at(own, &file, &held, &ledger)
            .map_err(|e| crate::stream::inbound::spec::Fail::from(("io_failed", e)))?;
    }
    Ok(RecordReply {
        dir,
        name,
        changed,
        remaining: left,
    })
}

/// `skill-install-record`：帧面入口（**写口**，只从 `stream/inbound/` 进）。
pub(crate) fn answer_record(
    req: &RecordReq,
    own: &Own,
) -> Result<RecordReply, crate::stream::inbound::spec::Fail> {
    record_at(own, None, req)
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/skill_ledger_tests.rs"]
mod tests;
