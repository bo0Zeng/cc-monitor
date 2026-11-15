//! **skill 装记录** —— 从别的机器「装到这台」的 skill，装时写进了哪几个文件（后端自有状态，经 `Own`）。
//!
//! # 要求
//!
//! 从别的机器「装到这台」的 skill 要能卸 —— 装的时候记下写了哪些文件，
//! 卸只删这些（装完用户自己改过的先问），足迹里看得见。
//!
//! # 文件（`~/.cc-monitor/skill-installs.json`，与资产目录同一个家、同一族写法）
//!
//! `{"version":3,"installs":{"<skill 目录绝对路径>":{"name":"demo","files":{"<相对路径>":{"digest":"<16 位十六进制>","created":true}}}},
//!   "mcp":{"<配置文件绝对路径>":{"<server>":"<摘要>"}},"touched":{"<配置文件绝对路径>":{"before":"<改之前的原文>"|null,"after":"<摘要>","names":["<server>"]}}}`
//!
//! - 版本照声明表（`relay_route_core::SKILL_LEDGER_FILE`，读写口 `common/own_state.rs` 管）：第 1 版把版本写在 `"v"` 里、第 2 版没有 `touched`，
//!   读的时候经升级步读成第 3 版；常驻起来时 [`bring_forward`] 写回一次，原文照字节留成 `skill-installs.json.v<N>`。
//! - `touched`：cc-monitor 改过的项目级 MCP 配置（装 · 推 · 拉 · 卸都记，不管那一条是不是它装的）：头一回改之前的原文（原来没有 ⇒ `null`）·
//!   最后一次写进去的整份摘要 · 改过哪几条；「改过的用户文件」照它列，撤回照它回到原文（`mcp_sync_flow::revert_project`）。
//! - **键是目录的绝对路径**：记的是「装时写进了哪几个文件」—— 落点是事实，名字只给人看。
//! - `digest`：**装时写进去的那一份原文**的摘要（`common::digest::Fnv`，只答相同 / 不同，**不防篡改**）。
//! - `created`：装之前这个路径不在 ⇒ `true`；装时盖掉了这台原有的一份 ⇒ `false`（卸它回不到装之前那一份 ⇒ 卸时要问）。
//! - 同一个目录再装一次：并进去 —— 新路径加进来、已记的换新摘要、`created` 取第一次的。
//!
//! # 写口（全仓一个：[`answer_record`]，只从 `stream/inbound/` 进 —— `Own` 由分派那一层递）
//!
//! - `{op:"add", name, files:{path:{digest, created}}}`：装完（或装到一半停下）把**装完与来源相同的那几个**（真写成了的 ＋ 本来就相同的）原样交回来
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

use crate::platform::fs::Own;
use copy_core::copy_text;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 文件名（与 `backend.json` / `assets-catalog.json` 同一个家）。字面量只住契约 crate（`relay_route_core::SKILL_LEDGER_REL`，数据位置页按它列），写者只有本模块（判据钉）。
pub(crate) const FILE_NAME: &str =
    relay_route_core::file_name_of(relay_route_core::SKILL_LEDGER_REL);

/// 装记录：状态（读不懂不覆盖；更新的格式也算读不懂）。第 1 · 2 版经 [`v1_to_v2`] · [`v2_to_v3`] 读成这一版；
/// 盘上那一份由 [`bring_forward`] 在常驻起来时写回成这一版。
const DOC: crate::common::own_state::Doc<Ledger> =
    crate::common::upgrade!(relay_route_core::SKILL_LEDGER_FILE, [v1_to_v2, v2_to_v3]);

/// 第 1 版 ⇒ 第 2 版：版本从 `"v"` 挪到顶层 `version`（读写口管），别的格子不变。`"v"` 不是 1 ⇒ 读不懂（不认的格式不覆盖）。
fn v1_to_v2(mut v: Value) -> Result<Value, crate::common::said::Said> {
    let Some(o) = v.as_object_mut() else {
        return Ok(v);
    };
    match o.remove("v") {
        None => Ok(v),
        Some(n) if n.as_u64() == Some(1) => Ok(v),
        Some(n) => Err(copy_text(
            "beSkillLedger.read.otherVersion",
            &[
                ("path", relay_route_core::SKILL_LEDGER_REL),
                ("mine", "1"),
                ("theirs", &n.to_string()),
            ],
        )
        .into()),
    }
}

/// 第 2 版 ⇒ 第 3 版：只多一格 `touched`（缺 ⇒ 空），别的格子不变。
fn v2_to_v3(v: Value) -> Result<Value, crate::common::said::Said> {
    Ok(v)
}

/// 常驻起来那一下调一次（升级只此一处，读口本身不写盘）：盘上是读得懂的旧版 ⇒ 在那一层目录的锁里照升级后的那一份写回成这一版
/// （原文留 `.v<N>`），回 `true`；已是这一版 · 没有 · 读不懂 · 比下限还老 ⇒ 不动盘，回 `false`。
pub fn bring_forward(own: &Own) -> Result<bool, crate::common::said::Said> {
    let lock_dir = own.home_dir()?;
    let file = own.at_home(DOC.rel)?;
    if !DOC.behind(file.as_path()) {
        return Ok(false);
    }
    let held = own.lock(&lock_dir)?;
    match read_at(file.as_path()) {
        Read::Present(l) => DOC.write(own, &lock_dir, &held, &l).map(|()| true),
        Read::Absent | Read::Unreadable(_) => Ok(false),
    }
}
/// 一趟 `add` 最多收这么多个文件（与 skill 读 / 装同一个上限，不另起一份）。
use crate::assets::asset_catalog::SKILL_MAX_FILES as MAX_FILES;

/// 一个装时写进去的文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "LedgerFile",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
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
pub(crate) struct Install {
    pub name: String,
    pub files: BTreeMap<String, Recorded>,
}

/// 整份文件。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ledger {
    pub installs: BTreeMap<String, Install>,
    /// MCP 装记录：配置文件的绝对路径 → server 名 → 装进去的那一条的摘要（[`mcp_digest`]）。
    /// 没有一条时不写这一格（旧文件与新文件同形）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mcp: BTreeMap<String, BTreeMap<String, String>>,
    /// cc-monitor 改过的项目级 MCP 配置：配置文件的绝对路径 → 改之前的原文 · 最后一次写进去的整份摘要 · 改过哪几条
    /// （「文件与数据 → 改过的用户文件」照它列、撤回照它回到改之前的原文）。没有一份时不写这一格。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub touched: BTreeMap<String, Touched>,
}

/// cc-monitor 改过的一份项目级 MCP 配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Touched {
    /// 头一回改之前那一份的原文（原来没有那份文件 ⇒ `None`）；再改不换。
    pub before: Option<String>,
    /// cc-monitor 最后一次写进去的整份原文的摘要（`common::digest::fnv_digest`）：撤回之前比它，不等 ⇒ 之后被改过。
    pub after: String,
    /// 改过（增 · 改 · 删）哪几条服务器。
    pub names: std::collections::BTreeSet<String>,
    /// 这一条链头一回改之前，装记录里这份文件已记着的那几条（server 名 → 摘要）：撤回把 `mcp` 那一格放回成它，
    /// 只摘这一回改动带进来的。没有 ⇒ 不写这一格。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub installed: BTreeMap<String, String>,
}

/// 记一笔「cc-monitor 改了这份项目级 MCP 配置」：头一回记下改之前的原文，再改只换 `after`、并进名字。回变没变。**纯**。
/// 链断了（这一回改之前那一份的摘要不等于记账的 `after`：两回之间用户手改过 · 删过）⇒ 另起：留底换成这一回的、名字只留这一回的，
/// 撤回只回到用户最后看到的那一份、不抹掉他手改的。
pub(crate) fn touch(
    ledger: &mut Ledger,
    file: &str,
    name: &str,
    before: Option<&str>,
    after: &str,
) -> bool {
    let installed = ledger.mcp.get(file).cloned().unwrap_or_default();
    let fresh = || Touched {
        before: before.map(str::to_string),
        after: after.to_string(),
        names: Default::default(),
        installed: installed.clone(),
    };
    let entry = ledger.touched.entry(file.to_string()).or_insert_with(fresh);
    let was = entry.clone();
    if before.map(crate::common::digest::fnv_digest).as_deref() != Some(was.after.as_str()) {
        *entry = fresh();
    }
    entry.after = after.to_string();
    entry.names.insert(name.to_string());
    *entry != was
}

/// 一条 MCP 定义的摘要（键序无关：按规范写法算）。装的时候记、卸的时候比。
pub(crate) fn mcp_digest(def: &Value) -> String {
    crate::common::digest::fnv_digest(&crate::assets::asset_catalog::canonical(def))
}

fn valid_digest(d: &str) -> bool {
    d.len() == 16 && d.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// 这台机器上记录文件的路径。家目录：`HOME`，没有再退 `USERPROFILE`；都没有 ⇒ `None`（不猜）。
pub(crate) fn ledger_path() -> Option<PathBuf> {
    let home = crate::platform::paths::home_dir()?;
    Some(home.join(crate::common::own_dir::DIR_NAME).join(FILE_NAME))
}

/// 读一次。三态：没有（还没装过）/ 读得懂 / 读不懂（**不覆盖**）。
pub(crate) type Read = crate::common::own_state::Read<Ledger>;

pub(crate) fn read_at(path: &Path) -> Read {
    // 版本由读写口先过（更新的格式整份不认、也不覆盖），这里只把这一版的原文解成记录。
    DOC.read_as::<Value>(path)
        .and_then(|v| match serde_json::from_value::<Ledger>(v) {
            Ok(l) => Read::Present(l),
            Err(e) => Read::Unreadable(crate::common::said::Said::with_raw(
                copy_text(
                    "beSkillLedger.read.unknown",
                    &[("path", &path.display().to_string())],
                ),
                e,
            )),
        })
}

/// 读成一份可用的记录：没有 ⇒ 空的；读不懂 ⇒ `ledger_unreadable`（读的人也不许把它说成「什么都没装过」）。
pub(crate) fn load_at(path: &Path) -> Result<Ledger, crate::common::said::Said> {
    match read_at(path) {
        Read::Absent => Ok(Ledger::default()),
        Read::Present(l) => Ok(l),
        Read::Unreadable(why) => Err(crate::wire::spec::fail("ledger_unreadable", why)),
    }
}

/// 这台装记录里 cc-monitor 改过的那几份项目级 MCP 配置（「文件与数据 → 改过的用户文件」读它）：家目录解不出 ⇒ 空；读不懂 ⇒ `ledger_unreadable`。
pub fn touched_here() -> Result<BTreeMap<String, Touched>, crate::common::said::Said> {
    match ledger_path() {
        Some(p) => Ok(load_at(&p)?.touched),
        None => Ok(BTreeMap::new()),
    }
}

// **写者两处**：[`record_at`] 末尾（记一趟），与常驻起来时的 [`bring_forward`]（旧版写回一次）；都经读写口整份原子写（带版本）；那一层目录由它在拿锁之前建。
//
// 读—改—写整段在那个目录的跨进程锁里（`platform/lock.rs`，[`record_at`] 开头拿）：两个后端进程同时记装记录时，
// 后写的整份会盖掉先写的一条 ⇒ 那一趟装的文件从此卸不掉。

/// `add` 的 `files`：非空、不超上限、每个路径都在 skill 里、摘要是 16 位小写十六进制。
fn check_files(files: &BTreeMap<String, Recorded>) -> Result<(), crate::common::said::Said> {
    if files.is_empty() {
        return Err(crate::wire::spec::fail(
            "bad_args",
            crate::common::contract::malformed("`files` is empty"),
        ));
    }
    if files.len() > MAX_FILES {
        return Err(crate::wire::spec::fail(
            "too_large",
            copy_text(
                "beSkillLedger.add.tooMany",
                &[("max", &MAX_FILES.to_string())],
            ),
        ));
    }
    for (path, rec) in files {
        if !crate::assets::skill_install::valid_rel(path) {
            return Err(crate::wire::spec::fail(
                "bad_args",
                crate::common::contract::malformed(&format!(
                    "{path:?} is not a relative path inside the skill"
                )),
            ));
        }
        if !valid_digest(&rec.digest) {
            return Err(crate::wire::spec::fail(
                "bad_args",
                crate::common::contract::malformed(&format!(
                    "`digest` of {path:?} is not 16 lowercase hex digits"
                )),
            ));
        }
    }
    Ok(())
}

/// 装记录写口的入参：按 `op` 分（多一个键 ⇒ `bad_args`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RecordReq {
    /// 装完记：`files` = `skill-install-plan` 答的 `ledger` 里真写成了的那几个。
    Add {
        /// skill 的目录名。
        name: String,
        files: BTreeMap<String, Recorded>,
        /// 给了 ⇒ 目录按那个项目里的 skill 根算。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<String>,
        /// 哪一家的（缺 = 默认那一家）：目录按那一家的 skill 根算。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent: Option<cmd_wire::AgentKind>,
    },
    /// 卸掉 / 已经不在的摘掉：`paths` 不在记录里 ⇒ `bad_args`，一个字节不动；摘到零个 ⇒ 整条记录摘掉。
    Drop { dir: String, paths: Vec<String> },
    /// MCP 装完记：`file` 配置文件的绝对路径（键）· 装进去的那一条的摘要 · 写之前那一份原文（不在 ⇒ `None`）· 写进去的整份摘要。
    McpAdd {
        file: String,
        name: String,
        digest: String,
        before: Option<String>,
        after: String,
    },
    /// MCP 卸掉摘（不是 cc-monitor 装的那一条也记：它改了这份文件）：写之前那一份原文 · 写进去的整份摘要。
    McpDrop {
        file: String,
        name: String,
        before: Option<String>,
        after: String,
    },
    /// 撤回了那一份（回到改之前的原文）：在锁里核记着的「最后写进去的摘要」还是 `after`（不是 ⇒ `stale`、一个字节不动），
    /// 摘掉改过那一格、装过那一格放回成头一回改之前的样子（只摘这一回改动带进来的）。
    McpRevert { file: String, after: String },
    /// 先记了账、文件却没写上：记着的「最后写进去的摘要」还是 `after`（撤回那一笔：`None` ＝ 已摘掉）⇒ 那两格放回成 `prior`
    /// （不是 ⇒ `stale`、不动）。
    McpUndo {
        file: String,
        #[serde(default)]
        after: Option<String>,
        prior: McpPrior,
    },
}

/// 装记录写口的回包。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct RecordReply {
    /// 这一条记录的目录（MCP：那份配置文件）。
    pub dir: String,
    pub name: String,
    /// 记录变没变（没变不写）。
    pub changed: bool,
    /// 这一条还剩几个文件（MCP：那份配置里还记着几条）。
    pub remaining: usize,
    /// MCP 那几笔：记之前这份文件的那两格（先记账再写文件，文件写不上时照它放回，[`RecordReq::McpUndo`]）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prior: Option<McpPrior>,
}

/// 一份项目级 MCP 配置在装记录里的那两格（改过 · 装过），记一笔之前的样子。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpPrior {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub touched: Option<Touched>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<BTreeMap<String, String>>,
}

/// 装记录的写口（生产 = [`answer_record`]，由 `stream/inbound/` 递进来；装 · 卸那几处不直呼它）。
pub(crate) type Record<'a> =
    &'a dyn Fn(&RecordReq) -> Result<RecordReply, crate::common::said::Said>;

/// `add`：并进那个目录的记录。回 `(目录, 变没变)`。**纯**（目录由调用方算好）。
pub(crate) fn add(
    ledger: &mut Ledger,
    dir: &str,
    name: &str,
    files: BTreeMap<String, Recorded>,
) -> bool {
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
pub(crate) fn drop_paths(
    ledger: &mut Ledger,
    dir: &str,
    paths: &[String],
) -> Result<usize, crate::common::said::Said> {
    let entry = ledger
        .installs
        .get_mut(dir)
        .ok_or(crate::common::said::Said::new(
            "not_found",
            copy_text("beSkillLedger.drop.notRecorded", &[("dir", dir)]),
        ))?;
    if let Some(p) = paths.iter().find(|p| !entry.files.contains_key(*p)) {
        return Err(crate::wire::spec::fail(
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

/// [`answer_record`] 的本体：写句柄（记录文件在它的家下）· 认家的注册表与用户级 skill 根都可喂
/// （生产 `Roots::HERE`；判据拿替身家 · 临时目录喂，不改进程环境）。
pub(crate) fn record_at(
    own: &Own,
    roots: crate::assets::skill_install::Roots<'_>,
    req: &RecordReq,
) -> Result<RecordReply, crate::common::said::Said> {
    // 先建那一层目录（在就算了），再拿它的跨进程锁，锁住之后才读。
    let no_home =
        |_| crate::wire::spec::fail("io_failed", copy_text("beSkillLedger.path.noHome", &[]));
    let lock_dir = own.home_dir().map_err(no_home)?;
    let file = own.at_home(DOC.rel).map_err(no_home)?;
    let path = file.as_path();
    // 只建那一层、建的那一下就是 0700。
    own.ensure_dir(&lock_dir).map_err(|e| {
        crate::wire::spec::fail(
            "io_failed",
            crate::common::said::Said::with_raw(
                copy_text(
                    "reason.fs.writeFailed",
                    &[
                        ("path", &lock_dir.as_path().display().to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                &e,
            ),
        )
    })?;
    let held = own
        .lock(&lock_dir)
        .map_err(|e| crate::wire::spec::fail("io_failed", e))?;
    let mut ledger = load_at(path)?;
    // MCP 那几笔记之前这份文件的那两格（回包带回去：文件写不上时照它放回）。
    let prior = match req {
        RecordReq::McpAdd { file, .. }
        | RecordReq::McpDrop { file, .. }
        | RecordReq::McpRevert { file, .. } => Some(McpPrior {
            touched: ledger.touched.get(file).cloned(),
            mcp: ledger.mcp.get(file).cloned(),
        }),
        _ => None,
    };
    let (dir, name, changed, left) = match req {
        RecordReq::McpUndo { file, after, prior } => {
            if ledger.touched.get(file).map(|t| &t.after) != after.as_ref() {
                return Err(crate::wire::spec::fail(
                    "stale",
                    copy_text("beMcpSyncFlow.revert.changedSince", &[]),
                ));
            }
            match &prior.touched {
                Some(t) => ledger.touched.insert(file.clone(), t.clone()),
                None => ledger.touched.remove(file),
            };
            match &prior.mcp {
                Some(m) => ledger.mcp.insert(file.clone(), m.clone()),
                None => ledger.mcp.remove(file),
            };
            let left = ledger.mcp.get(file).map_or(0, BTreeMap::len);
            (file.clone(), String::new(), true, left)
        }
        RecordReq::Add {
            name,
            files,
            project,
            agent,
        } => {
            if !crate::assets::skill_install::valid_name(name) {
                return Err(crate::wire::spec::fail(
                    "bad_args",
                    crate::common::contract::malformed(
                        "`name` is not a valid skill directory name",
                    ),
                ));
            }
            check_files(files)?;
            // 目录自己算：与 `skill-install-plan` 答 `dir` 的是同一个根（不收调用方给的路径）。
            let dir = crate::assets::skill_install::skill_root_of(
                roots,
                agent.as_deref(),
                project.as_deref(),
            )?
            .ok_or(crate::common::said::Said::new(
                "io_failed",
                copy_text("beSkillLedger.add.noRoot", &[]),
            ))?
            .join(name)
            .display()
            .to_string();
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
        RecordReq::McpRevert { file, after } => {
            // 出错就整趟返回、不写盘：内存里摘下来的不用放回。
            let Some(t) = ledger.touched.remove(file) else {
                return Err(crate::wire::spec::fail(
                    "not_found",
                    copy_text("beMcpSyncFlow.revert.notRecorded", &[]),
                ));
            };
            if &t.after != after {
                return Err(crate::wire::spec::fail(
                    "stale",
                    copy_text("beMcpSyncFlow.revert.changedSince", &[]),
                ));
            }
            if t.installed.is_empty() {
                ledger.mcp.remove(file);
            } else {
                ledger.mcp.insert(file.clone(), t.installed);
            }
            let left = ledger.mcp.get(file).map_or(0, BTreeMap::len);
            (file.clone(), String::new(), true, left)
        }
        RecordReq::McpAdd {
            file,
            name,
            before,
            after,
            ..
        }
        | RecordReq::McpDrop {
            file,
            name,
            before,
            after,
        } => {
            if !valid_digest(after) {
                return Err(crate::wire::spec::fail(
                    "bad_args",
                    crate::common::contract::malformed("`after` must be 16 lowercase hex digits"),
                ));
            }
            if !Path::new(file).is_absolute() {
                return Err(crate::wire::spec::fail(
                    "bad_args",
                    crate::common::contract::malformed("`file` is not an absolute path"),
                ));
            }
            if name.trim().is_empty() {
                return Err(crate::wire::spec::fail(
                    "bad_args",
                    crate::common::contract::malformed("`name` is empty"),
                ));
            }
            // 先记改过那一格：头一回改之前装记录里这份已记着的那几条照改之前的样子留进去（撤回放回成它）。
            let touched = touch(&mut ledger, file, name, before.as_deref(), after);
            let changed = if let RecordReq::McpAdd { digest, .. } = req {
                if !valid_digest(digest) {
                    return Err(crate::wire::spec::fail(
                        "bad_args",
                        crate::common::contract::malformed(
                            "`digest` must be 16 lowercase hex digits",
                        ),
                    ));
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
            (file.clone(), name.clone(), changed || touched, left)
        }
    };
    if changed {
        DOC.write(own, &lock_dir, &held, &ledger)
            .map_err(|e| crate::wire::spec::fail("io_failed", e))?;
    }
    Ok(RecordReply {
        dir,
        name,
        changed,
        remaining: left,
        prior,
    })
}

/// 装记录的写口（本进程里记；写句柄只从 `stream/inbound/` 递进来）。
pub fn answer_record(req: &RecordReq, own: &Own) -> Result<RecordReply, crate::common::said::Said> {
    record_at(own, crate::assets::skill_install::Roots::HERE, req)
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/skill_ledger_tests.rs"]
mod tests;
