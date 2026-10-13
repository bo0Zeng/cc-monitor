//! 资产目录：每台后端把它看到的 skill 与 MCP（用户级 ＋ 这台上开过会话的项目里的）记成一份目录，目录在后端之间自动对上；
//! 装要用户点，装时内容原样拷 ＋ 标可疑项，不替用户改写。
//!
//! - 是后端自有状态（写经 `Own`）：`~/.cc-monitor/assets-catalog.json`，全仓只有本模块写它，写口（`answer_*` 三条）
//!   只从 `stream/inbound/` 那一扇门进来。不是用户文件 —— 用户的 skill / `.mcp.json` 本模块一个字节都不写。
//! - 只记「有哪些、定义是什么」：同步那条路上不往任何机器装东西。
//! - 目录里不带 MCP 的 `env` / `headers` 的值（只带键名），`digest` 按整条原文算：目录是在用户没点任何东西时自动抄到每台机器的，
//!   API key 不跟着走；「原样拷」发生在用户点「装」那一下、从来源那台现读。
//!
//! # 一台一快照 · 合并只有一句
//!
//! `machines[id]` 是那台机器自己扫出来的一整份（[`Snapshot`]），`gen` 是那台自己的代数（它自己那份变了才 +1）。
//! 同一台取 `gen` 大的那一份整份；自己那一格只认自己扫的（[`merge`]）。不比墙钟、删除随整份替换传播、幂等可交换 ⇒ 反复同步收敛。
//! 「这台缺什么」的判定（[`rows`]）也住这里。
//!
//! # 摘要
//!
//! FNV-1a 64（[`Fnv`]）—— 稳定、零新依赖。只答「相同 / 不同」，不防篡改。

use crate::common::said::IntoNote as _;
use crate::platform::fs::{Own, OwnLock, OwnPath};
use copy_core::copy_text;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// 目录文件名。字面量只住契约 crate（`relay_route_core::ASSET_CATALOG_REL`：monitor 的数据位置页按它列出），
/// 写者仍只有本模块（`asset_catalog_tests` 钉）。
pub const FILE_NAME: &str = relay_route_core::file_name_of(relay_route_core::ASSET_CATALOG_REL);

/// 文件格式版本。读到别的版本 ⇒ 不认、**不覆盖**（多半是更新的后端写的）。
pub const FORMAT_V: u64 = 1;

/// 条目种类闭集（线上 `kind`）。
pub const KINDS: &[&str] = &[KIND_SKILL, KIND_MCP];
pub const KIND_SKILL: &str = "skill";
pub const KIND_MCP: &str = "mcp";

/// 「这台对那一条」的三态闭集（线上 `rows[].state`）。
#[cfg(test)]
pub const HERE_STATES: &[&str] = &[HERE_MISSING, HERE_DIFFERS, HERE_SAME];
/// 这台一条同名的都没有。
pub const HERE_MISSING: &str = "missing";
/// 这台有同名的，但没有一条与别处的摘要相同。
pub const HERE_DIFFERS: &str = "differs";
/// 这台有一条同名且摘要相同的。
pub const HERE_SAME: &str = "same";

/// 一个 skill 最多走这么多个文件（超了 `summary.truncated`，摘要按已走到的算）。
pub const SKILL_MAX_FILES: usize = 512;
/// 一个文件最多读这么多字节进摘要（超了只记长度 ＋ `truncated`）。
pub const SKILL_MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
/// 读自己那份目录文件的上限（远超正常量级；超了当读不出来，不截断）。
pub const CATALOG_MAX_BYTES: u64 = 16 * 1024 * 1024;
/// 机器 id 的长度上限（入参校验用；本模块生成的是 16 个十六进制字符）。
const MAX_ID_BYTES: usize = 64;

/// MCP 定义里**原样**进目录的字段。其余字段只记键名（未知字段可能装着密钥）。
pub const MCP_VERBATIM_FIELDS: &[&str] = &["args", "command", "cwd", "type", "url"];
/// MCP 定义里**只记键名**的对象字段（值是密钥的常见住处）。
pub const MCP_KEYS_ONLY_FIELDS: &[&str] = &["env", "headers"];

/// 目录里的一条。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "CatalogAsset"
    )
)]
pub struct Asset {
    pub kind: String,
    pub name: String,
    /// 那台上的项目目录（项目级）；`None` = 用户级。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub project: Option<String>,
    /// 它在那台上住哪（skill：那个目录；MCP：那份配置文件）。只给人看、给「在文件窗口里打开」用，不参与判定。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub dir: Option<String>,
    pub digest: String,
    /// 给人看的摘要（skill：description / 文件数 / 字节数；MCP：去掉密钥值之后的定义）。
    #[cfg_attr(test, ts(type = "unknown"))]
    pub summary: Value,
}

/// 一台机器自己扫出来的一整份。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// 那台自报的称呼（`user@host`，尽力而为；只给人看）。
    pub label: String,
    /// 那台自己的代数：它自己那份变了才 +1。合并只比它。
    pub gen: u64,
    /// 那一代出生的时刻（那台自己的钟，unix 秒；只给人看，**不参与合并**）。
    pub seen_at: u64,
    pub assets: Vec<Asset>,
    /// 那台上开过会话的项目目录（装到项目时给人选；随整份一起换）。
    #[serde(default)]
    pub project_dirs: Vec<String>,
    /// 用户在这台上写下的备注（[`entry_key`] → 那一条）。随整份一起同步；生效的是各台里 `rev` 最大的那一条（[`note_of`]）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub notes: BTreeMap<String, Note>,
    /// 这台的用户级 MCP 是各账号共用的那一份（这台有账号库）⇒ 全局那一级能装、能卸；否则只读。随整份同步。
    #[serde(default, skip_serializing_if = "is_false")]
    pub shared_mcp: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// 一条备注。`rev` 是写它那一刻这台目录里同一条目各台备注的最大 `rev` ＋ 1（不比墙钟）；`text` 空 = 清掉了。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "CatalogNote"
    )
)]
pub struct Note {
    pub text: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub rev: u64,
}

/// 「上次来看扩展页」那两格（本机那一份自己记，不随同步走）：`prev` 之后第一次见到的条目算「新见到」。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Visits {
    pub prev: u64,
    pub last: u64,
}

/// 目录文件的全部内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Catalog {
    pub v: u64,
    /// 这台机器的 id（第一次记目录时生成，之后不变）。
    #[serde(rename = "self")]
    pub self_id: String,
    pub machines: BTreeMap<String, Snapshot>,
    /// 每个条目（[`entry_key`]）在这台目录里第一次出现的时刻（这台的钟，unix 秒）。只这台自己用，不随同步走。
    #[serde(default)]
    pub known: BTreeMap<String, u64>,
    #[serde(default)]
    pub visits: Visits,
}

/// 一台这一趟扫出来的全部：条目 · 开过会话的项目 · 读不出来的那几份。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scanned {
    pub assets: Vec<Asset>,
    pub projects: Vec<String>,
    pub problems: Vec<String>,
    /// 这台有账号库（用户级 MCP 是各账号共用的那一份）。
    pub shared_mcp: bool,
}

/// 一个条目的键（种类 ＋ 名字）：「新见到」按它记，扩展页一行也按它分。
pub fn entry_key(kind: &str, name: &str) -> String {
    format!("{kind}/{name}")
}

// ───────────────────────── 摘要（纯） ─────────────────────────

/// FNV-1a 64。每段先喂长度再喂字节 ⇒ 段与段之间不会粘成同一串。
#[derive(Clone, Copy)]
pub struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    fn byte(&mut self, b: u8) {
        self.0 ^= u64::from(b);
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
    }
    /// 喂一段（带长度前缀）。
    pub fn part(&mut self, bytes: &[u8]) -> &mut Self {
        for b in (bytes.len() as u64).to_le_bytes() {
            self.byte(b);
        }
        for &b in bytes {
            self.byte(b);
        }
        self
    }
    pub fn hex(&self) -> String {
        format!("{:016x}", self.0)
    }
}

/// JSON 的规范写法：对象按键排序（不依赖 `serde_json` 开没开 `preserve_order`）。
pub fn canonical(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            let body: Vec<String> = keys
                .iter()
                .map(|k| format!("{}:{}", Value::String((*k).clone()), canonical(&m[*k])))
                .collect();
            format!("{{{}}}", body.join(","))
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        other => other.to_string(),
    }
}

/// 一条 MCP 定义 → 目录条目。`digest` 按**整条原文**（含密钥值）的规范写法算；`summary` 去掉密钥值。
pub fn mcp_asset(project: Option<&str>, name: &str, def: &Value, file: Option<&Path>) -> Asset {
    let mut f = Fnv::default();
    f.part(KIND_MCP.as_bytes()).part(canonical(def).as_bytes());
    Asset {
        kind: KIND_MCP.to_string(),
        name: name.to_string(),
        project: project.map(str::to_string),
        dir: file.map(|p| p.display().to_string()),
        digest: f.hex(),
        summary: mcp_summary(def),
    }
}

/// 去掉密钥值的那一份：[`MCP_VERBATIM_FIELDS`] 原样；[`MCP_KEYS_ONLY_FIELDS`] 只留键名（排序）；
/// 其余字段只留键名（`otherKeys`）。定义本身不是对象 ⇒ `{"shape":"not-an-object"}`（原样值不进目录）。
pub fn mcp_summary(def: &Value) -> Value {
    let Some(obj) = def.as_object() else {
        return json!({ "shape": "not-an-object" });
    };
    let mut out = Map::new();
    let mut other: Vec<String> = Vec::new();
    for (k, v) in obj {
        if MCP_VERBATIM_FIELDS.contains(&k.as_str()) {
            out.insert(k.clone(), v.clone());
        } else if MCP_KEYS_ONLY_FIELDS.contains(&k.as_str()) {
            let mut keys: Vec<String> = v
                .as_object()
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default();
            keys.sort();
            out.insert(format!("{k}Keys"), json!(keys));
        } else {
            other.push(k.clone());
        }
    }
    if !other.is_empty() {
        other.sort();
        out.insert("otherKeys".into(), json!(other));
    }
    Value::Object(out)
}

/// 一个 skill 目录 → 目录条目。按（相对路径, 内容）逐个进摘要；文件链接按它指向的内容算（目录链接不下去）。
/// 可执行位不进摘要：读执行位是平台原语（只许住 `platform/`）⇒ 只差 `chmod +x` 的两份判成「相同」。
pub fn skill_asset(
    project: Option<&str>,
    name: &str,
    dir: &Path,
    description: Option<&str>,
) -> Asset {
    let mut f = Fnv::default();
    f.part(KIND_SKILL.as_bytes());
    let (mut files, mut bytes) = (0usize, 0u64);
    let mut binary = false;
    // 摘要没能按全部内容算的那几处（降级要说清是哪一个，不是只给一个布尔）。
    let mut notice: Vec<String> = Vec::new();
    let walk = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .min_depth(1)
        .sort_by_file_name();
    for ent in walk {
        let ent = match ent {
            Ok(e) => e,
            Err(e) => {
                let kind = e
                    .io_error()
                    .map_or(std::io::ErrorKind::Other, std::io::Error::kind);
                notice.push(
                    crate::common::said::Said::with_raw(
                        copy_text(
                            "beAssetCatalog.digest.walkFailed",
                            &[("why", &copy_core::io_reason(kind))],
                        ),
                        &e,
                    )
                    .into_note(),
                );
                continue;
            }
        };
        if ent.file_type().is_dir() {
            continue;
        }
        if files >= SKILL_MAX_FILES {
            notice.push(copy_text(
                "beAssetCatalog.digest.tooMany",
                &[("max", &SKILL_MAX_FILES.to_string())],
            ));
            break;
        }
        files += 1;
        let rel = ent
            .path()
            .strip_prefix(dir)
            .unwrap_or(ent.path())
            .to_string_lossy()
            .replace('\\', "/");
        f.part(rel.as_bytes());
        // 链接：跟到底看它是什么（装到别处时拷过去的是内容，不是链接）；指向目录的不下去。
        let len = match crate::platform::fs::Ro::metadata(ent.path()) {
            Ok(m) if m.is_file() => m.len(),
            _ => {
                notice.push(copy_text(
                    "beAssetCatalog.digest.notRegular",
                    &[("rel", &rel.to_string())],
                ));
                f.part(b"not-a-file");
                continue;
            }
        };
        bytes += len;
        match crate::platform::fs::Ro::read_regular_capped(ent.path(), SKILL_MAX_FILE_BYTES) {
            Ok(body) => {
                if body.contains(&0) || std::str::from_utf8(&body).is_err() {
                    binary = true;
                }
                f.part(&body);
            }
            Err(error) => {
                // 那一句只带原因词；系统原话进日志（摘要提示那一格没有详情位）。
                notice.push(
                    error
                        .wrap(|why| {
                            copy_text(
                                "beAssetCatalog.digest.lenOnly",
                                &[("rel", &rel), ("why", why)],
                            )
                        })
                        .into_note(),
                );
                f.part(b"len").part(&len.to_le_bytes());
            }
        }
    }
    Asset {
        kind: KIND_SKILL.to_string(),
        name: name.to_string(),
        project: project.map(str::to_string),
        dir: Some(dir.display().to_string()),
        digest: f.hex(),
        summary: json!({
            "description": description,
            "files": files,
            "bytes": bytes,
            "binary": binary,
            "truncated": !notice.is_empty(),
            "notice": notice,
        }),
    }
}

/// 适配层的原始事实 → 排好序的条目（skill 在前，同种按名字、MCP 再按项目）。
pub(crate) fn assets_from(sightings: &[crate::agents::Sightings]) -> (Vec<Asset>, Vec<String>) {
    let mut out = Vec::new();
    let mut problems = Vec::new();
    for s in sightings {
        for k in &s.skills {
            out.push(skill_asset(
                k.project.as_deref(),
                &k.name,
                &k.dir,
                k.description.as_deref(),
            ));
        }
        for m in &s.mcp {
            out.push(mcp_asset(
                m.project.as_deref(),
                &m.name,
                &m.def,
                Some(&m.file),
            ));
        }
        problems.extend(s.problems.iter().cloned());
    }
    out.sort_by(|a, b| (&a.kind, &a.name, &a.project).cmp(&(&b.kind, &b.name, &b.project)));
    (out, problems)
}

/// 这台上开过会话的项目目录：取历史清单那一份（项目清单每一行的 `projectPath`），不另起一份扫描。
/// 列不出来 ⇒ 空 ＋ 一句话（「这台没有项目」与「这台的项目列不出来」不许合成一句）。
pub(crate) fn session_projects() -> (Vec<String>, Option<String>) {
    session_projects_at(&crate::observe::history_query::agent_home())
}

/// [`session_projects`] 的本体：agent 的家由调用方给（判据拿临时家目录喂）。
pub(crate) fn session_projects_at(agent_home: &Path) -> (Vec<String>, Option<String>) {
    let mut buf = Vec::new();
    if let Err(e) = crate::observe::history_query::list_projects_into(agent_home, &mut buf) {
        return (Vec::new(), Some(e));
    }
    let mut dirs: Vec<String> = String::from_utf8_lossy(&buf)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| {
            v.get("projectPath")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .filter(|p| Path::new(p).is_absolute())
        .collect();
    dirs.sort();
    dirs.dedup();
    (dirs, None)
}

/// 这台现扫一次：先取项目清单，再交适配层扫用户级 ＋ 那几个项目。
pub(crate) fn scan_here() -> Scanned {
    let (projects, why) = session_projects();
    let user_mcp = crate::accounts::manage::mcp_share_exec::user_mcp_file();
    let (assets, mut problems) = assets_from(&crate::agents::asset_sightings(
        &projects,
        user_mcp.as_deref(),
    ));
    problems.extend(why);
    let shared_mcp = crate::platform::paths::home_dir()
        .and_then(|h| {
            crate::accounts::manage::mcp_share_exec::store_file_in(&h.display().to_string())
        })
        .is_some();
    Scanned {
        assets,
        projects,
        problems,
        shared_mcp,
    }
}

// ───────────────────────── 合并与判定（纯） ─────────────────────────

/// 一份新目录（这台第一次记）。
pub fn fresh(self_id: String) -> Catalog {
    Catalog {
        v: FORMAT_V,
        self_id,
        machines: BTreeMap::new(),
        known: BTreeMap::new(),
        visits: Visits::default(),
    }
}

/// 把这台刚扫出来的那一份放进自己那一格。**真变了**（条目 · 项目清单 · 称呼有一样不同）才 `gen + 1`、记时刻，回 `true`。
pub fn refresh_self(
    cat: &mut Catalog,
    assets: Vec<Asset>,
    projects: Vec<String>,
    label: &str,
    now: u64,
) -> bool {
    let id = cat.self_id.clone();
    if let Some(cur) = cat.machines.get(&id) {
        if cur.assets == assets && cur.project_dirs == projects && cur.label == label {
            return false;
        }
    }
    let gen = cat.machines.get(&id).map_or(0, |s| s.gen) + 1;
    let (notes, shared_mcp) = cat
        .machines
        .get(&id)
        .map(|s| (s.notes.clone(), s.shared_mcp))
        .unwrap_or_default();
    cat.machines.insert(
        id,
        Snapshot {
            label: label.to_string(),
            gen,
            seen_at: now,
            assets,
            project_dirs: projects,
            notes,
            shared_mcp,
        },
    );
    true
}

/// 这台有没有账号库记进自己那一格：变了才 `gen + 1`，回「变了没有」。自己那一格还没有 ⇒ 不记。
pub fn mark_shared_mcp(cat: &mut Catalog, shared: bool, now: u64) -> bool {
    let id = cat.self_id.clone();
    match cat.machines.get_mut(&id) {
        Some(me) if me.shared_mcp != shared => {
            me.shared_mcp = shared;
            me.gen += 1;
            me.seen_at = now;
            true
        }
        _ => false,
    }
}

/// 一个条目现在生效的备注：各台里 `rev` 最大的那一条（打平按机器 id）；没有 / 清掉了 ⇒ `None`。
pub fn note_of(cat: &Catalog, key: &str) -> Option<String> {
    cat.machines
        .iter()
        .filter_map(|(id, s)| s.notes.get(key).map(|n| (n.rev, id, n)))
        .max_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)))
        .map(|(_, _, n)| n.text.clone())
        .filter(|t| !t.is_empty())
}

/// 用户写 / 改 / 清（空串）一条备注：记进**这台自己那一格**（真变了才 `gen + 1`），随目录推到别的后端。回「变了没有」。
/// 自己那一格还没有 ⇒ 不记（调用方先现扫一次）。
pub fn set_note(cat: &mut Catalog, key: &str, text: &str, now: u64) -> bool {
    let text = text.trim();
    if note_of(cat, key).as_deref().unwrap_or("") == text {
        return false;
    }
    let rev = cat
        .machines
        .values()
        .filter_map(|s| s.notes.get(key).map(|n| n.rev))
        .max()
        .unwrap_or(0)
        + 1;
    let id = cat.self_id.clone();
    let Some(me) = cat.machines.get_mut(&id) else {
        return false;
    };
    me.notes.insert(
        key.to_string(),
        Note {
            text: text.to_string(),
            rev,
        },
    );
    me.gen += 1;
    me.seen_at = now;
    true
}

/// 目录里任何一台有、`known` 里还没有的条目记下第一次见到的时刻。回「记了没有」（只影响要不要落盘，不算目录变了）。
pub fn note_known(cat: &mut Catalog, now: u64) -> bool {
    let keys: Vec<String> = cat
        .machines
        .values()
        .flat_map(|s| s.assets.iter().map(|a| entry_key(&a.kind, &a.name)))
        .collect();
    let mut noted = false;
    for k in keys {
        if let std::collections::btree_map::Entry::Vacant(e) = cat.known.entry(k) {
            e.insert(now);
            noted = true;
        }
    }
    noted
}

/// 把别处传来的各台快照并进来：**同一台取 `gen` 大的整份**；这台自己那一格一律不收。回「真变了没有」。
pub fn merge(cat: &mut Catalog, incoming: BTreeMap<String, Snapshot>) -> bool {
    let mut changed = false;
    for (id, snap) in incoming {
        if id == cat.self_id {
            continue;
        }
        let newer = cat.machines.get(&id).is_none_or(|old| snap.gen > old.gen);
        if newer {
            cat.machines.insert(id, snap);
            changed = true;
        }
    }
    changed
}

/// 别处那一条（[`CatalogRow::from`]）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct CatalogFrom {
    /// 那台目录的 id。
    pub machine: String,
    /// 那台上的项目目录；用户级 ⇒ `null`。
    pub project: Option<String>,
    pub digest: String,
    #[cfg_attr(test, ts(type = "unknown"))]
    pub summary: Value,
}

/// 「别的机器有的，这台怎样」那一行：`state` 闭集 `missing` · `differs` · `same`（[`HERE_STATES`]）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct CatalogRow {
    pub kind: String,
    pub name: String,
    pub state: &'static str,
    pub from: Vec<CatalogFrom>,
}

/// 「别的机器有的，这台怎样」—— 每个（种类, 名字）一行，只列别处有的。判定只在这里。
pub fn rows(cat: &Catalog) -> Vec<CatalogRow> {
    let mine: Vec<&Asset> = cat
        .machines
        .get(&cat.self_id)
        .map(|s| s.assets.iter().collect())
        .unwrap_or_default();
    let mut by_key: BTreeMap<(String, String), Vec<CatalogFrom>> = BTreeMap::new();
    for (id, snap) in &cat.machines {
        if *id == cat.self_id {
            continue;
        }
        for a in &snap.assets {
            by_key
                .entry((a.kind.clone(), a.name.clone()))
                .or_default()
                .push(CatalogFrom {
                    machine: id.clone(),
                    project: a.project.clone(),
                    digest: a.digest.clone(),
                    summary: a.summary.clone(),
                });
        }
    }
    by_key
        .into_iter()
        .map(|((kind, name), from)| {
            let here: Vec<&&Asset> = mine
                .iter()
                .filter(|a| a.kind == kind && a.name == name)
                .collect();
            let state = if here.is_empty() {
                HERE_MISSING
            } else if here
                .iter()
                .any(|a| from.iter().any(|f| f.digest == a.digest))
            {
                HERE_SAME
            } else {
                HERE_DIFFERS
            };
            CatalogRow {
                kind,
                name,
                state,
                from,
            }
        })
        .collect()
}

// ───────────────────────── 线上形状 ─────────────────────────

/// 缺 · `null` ⇒ 空（另一台的整份应答：旧版没有这一格）。
fn null_is_empty<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// 目录线上形里的一台（两条命令共用；`assets-catalog-merge` 收的 `catalog.machines` 也是这个形状）。
/// 收的那一份是另一台后端的应答：多出来的键照收（对面可能更新）；`projectDirs` · `notes` · `sharedMcp` 缺或 `null` ⇒ 空。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct CatalogMachine {
    pub id: String,
    pub label: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub gen: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub seen_at: u64,
    pub assets: Vec<Asset>,
    #[serde(default, deserialize_with = "null_is_empty")]
    pub project_dirs: Vec<String>,
    #[serde(default, deserialize_with = "null_is_empty")]
    pub notes: BTreeMap<String, Note>,
    #[serde(default, deserialize_with = "null_is_empty")]
    pub shared_mcp: bool,
}

/// `assets-catalog` / `assets-catalog-merge` 的回包：整份目录 ＋「这台缺什么」。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct CatalogReply {
    /// 这台机器的 id。
    #[serde(rename = "self")]
    #[cfg_attr(test, ts(rename = "self"))]
    pub self_id: String,
    pub machines: Vec<CatalogMachine>,
    pub rows: Vec<CatalogRow>,
    /// 这一趟扫描读不出来的那几份（一句话一份）。
    pub problems: Vec<String>,
    /// 这一趟目录有没有变（变了才写盘）。
    pub changed: bool,
    /// 目录文件在这台上的路径。
    pub path: Option<String>,
}

/// 另一台的整份目录（`assets-catalog-merge` 的 `catalog`）：只读 `machines`，别的键照收。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct CatalogIn {
    pub machines: Vec<CatalogMachine>,
}

/// `assets-catalog-merge` 的入参。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub struct CatalogMergeReq {
    pub catalog: CatalogIn,
}

/// 一份目录的线上形状（两条命令共用）。
pub fn wire(
    cat: &Catalog,
    problems: &[String],
    changed: bool,
    path: Option<&Path>,
) -> CatalogReply {
    let machines = cat
        .machines
        .iter()
        .map(|(id, s)| CatalogMachine {
            id: id.clone(),
            label: s.label.clone(),
            gen: s.gen,
            seen_at: s.seen_at,
            assets: s.assets.clone(),
            project_dirs: s.project_dirs.clone(),
            notes: s.notes.clone(),
            shared_mcp: s.shared_mcp,
        })
        .collect();
    CatalogReply {
        self_id: cat.self_id.clone(),
        machines,
        rows: rows(cat),
        problems: problems.to_vec(),
        changed,
        path: path.map(|p| p.display().to_string()),
    }
}

/// 收进来的各台 ⇒ 快照（解码器管形状；这里只判解码器判不了的两样：id 空 / 太长 · 种类不在闭集 ⇒ `bad_args`，不猜）。
pub fn machines_of(incoming: Vec<CatalogMachine>) -> Result<BTreeMap<String, Snapshot>, String> {
    let mut out = BTreeMap::new();
    for m in incoming {
        if m.id.is_empty() || m.id.len() > MAX_ID_BYTES {
            return Err(crate::common::contract::malformed(
                "a machine `id` is missing, empty or too long",
            ));
        }
        if let Some(a) = m.assets.iter().find(|a| !KINDS.contains(&a.kind.as_str())) {
            return Err(crate::common::contract::malformed(&format!(
                "asset kind `{}` is not one of {KINDS:?}",
                a.kind
            )));
        }
        out.insert(
            m.id,
            Snapshot {
                label: m.label,
                gen: m.gen,
                seen_at: m.seen_at,
                assets: m.assets,
                project_dirs: m.project_dirs,
                notes: m.notes,
                shared_mcp: m.shared_mcp,
            },
        );
    }
    Ok(out)
}

/// 线上 `catalog`（JSON）→ 各台快照（判据用：形状不对 ⇒ 解码器那一句）。
#[cfg(test)]
pub fn machines_from_wire(v: &Value) -> Result<BTreeMap<String, Snapshot>, String> {
    let cat: CatalogIn = serde_json::from_value(v.clone())
        .map_err(|e| crate::common::contract::malformed(&e.to_string()))?;
    machines_of(cat.machines)
}

// ───────────────────────── 这台机器的事实 ─────────────────────────

fn home() -> Option<PathBuf> {
    crate::platform::paths::home_dir()
}

/// 这台自报的称呼：`user@host`（环境里取；取不到的那一半写 `?`）。只给人看，不参与任何判定。
pub fn machine_label() -> String {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "?".into());
    // ⚠ 只从环境取（`HOSTNAME` 在非交互 SSH 里常常没导出 ⇒ `?`）：读主机名的平台原语只许住 `platform/`，
    //   而这一格只给人看 —— 界面上够得到的机器用 monitor 那边的配置名称呼它，这里不为它开平台口子。
    let host = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "?".into());
    format!("{user}@{host}")
}

fn now_secs() -> u64 {
    crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 生成这台的 id：（家目录, 称呼, 纳秒, pid）的摘要。只在目录文件不存在时调一次。
fn new_machine_id() -> String {
    let nanos = crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut f = Fnv::default();
    f.part(
        home()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_default()
            .as_bytes(),
    )
    .part(machine_label().as_bytes())
    .part(&nanos.to_le_bytes())
    .part(&std::process::id().to_le_bytes());
    f.hex()
}

/// 读一次目录文件。三态：没有（第一次）/ 读得懂 / 读不出来（**不覆盖**）。更新的格式整份不认。
pub type Read = crate::common::own_state::Read<Catalog>;

pub fn read_at(path: &Path) -> Read {
    crate::common::own_state::read_json::<Catalog>(path, CATALOG_MAX_BYTES).and_then(|c| {
        if c.v == FORMAT_V {
            Read::Present(c)
        } else {
            Read::Unreadable(
                copy_text(
                    "beAssetCatalog.read.newer",
                    &[
                        ("path", &path.display().to_string()),
                        ("mine", &FORMAT_V.to_string()),
                        ("theirs", &c.v.to_string()),
                    ],
                )
                .into(),
            )
        }
    })
}

/// **全仓唯一的写者**（经 `own_state` 原子写）。目录由 [`update_at`] 在拿锁之前建（那一层）。
fn write_at(
    own: &Own,
    file: &OwnPath,
    held: &OwnLock,
    cat: &Catalog,
) -> Result<(), crate::common::said::Said> {
    let body = crate::common::own_state::json_body(file.as_path(), cat)?;
    own.write(file, held, &body)
}

//
// 读—改—写整段在那个目录的跨进程锁里（`platform/lock.rs`，[`update_at`] 开头拿）：首建时第二个进程读到第一个写下的 `self`、
// 不再生出第二个 id；同一台两个进程各自 +1 代数变成串行。

/// 这台机器现扫一次、并进 `incoming`（若有）、真变了才落盘。**可喂夹具**：路径与扫描结果由调用方给。
pub fn update_at(
    own: &Own,
    scanned: Scanned,
    label: &str,
    incoming: Option<BTreeMap<String, Snapshot>>,
) -> Result<CatalogReply, crate::stream::inbound::spec::Fail> {
    let (cat, problems, changed) = update_with(own, scanned, label, incoming, false, now_secs())?;
    let path = own.at_home(relay_route_core::ASSET_CATALOG_REL).ok();
    Ok(wire(
        &cat,
        &problems,
        changed,
        path.as_ref().map(OwnPath::as_path),
    ))
}

/// [`update_at`] 的本体：交回并好的整份（扩展页的表从它算）。`visit` ⇒ 这一趟算「来看了一次」（挪 [`Visits`]）。
/// 回 `(目录, 读不出来的那几份, 目录真变了没有)`；「新见到」的记账与来看那两格变了也落盘，但不算目录变了（不触发扇出）。
pub fn update_with(
    own: &Own,
    scanned: Scanned,
    label: &str,
    incoming: Option<BTreeMap<String, Snapshot>>,
    visit: bool,
    now: u64,
) -> Result<(Catalog, Vec<String>, bool), crate::stream::inbound::spec::Fail> {
    update_core(own, scanned, label, incoming, visit, None, now)
}

/// [`update_with`] 再加一笔备注（`(条目键, 正文)`，见 [`set_note`]）：同一把锁里现扫 · 记备注 · 落盘。
pub fn update_noting(
    own: &Own,
    scanned: Scanned,
    label: &str,
    note: (&str, &str),
    now: u64,
) -> Result<(Catalog, bool), crate::stream::inbound::spec::Fail> {
    update_core(own, scanned, label, None, false, Some(note), now).map(|(c, _, ch)| (c, ch))
}

fn update_core(
    own: &Own,
    scanned: Scanned,
    label: &str,
    incoming: Option<BTreeMap<String, Snapshot>>,
    visit: bool,
    note: Option<(&str, &str)>,
    now: u64,
) -> Result<(Catalog, Vec<String>, bool), crate::stream::inbound::spec::Fail> {
    let no_home = |_| {
        crate::stream::inbound::spec::Fail::from((
            "io_failed",
            copy_text("beAssetCatalog.write.noHome", &[]),
        ))
    };
    let dir = own.home_dir().map_err(no_home)?;
    let file = own
        .at_home(relay_route_core::ASSET_CATALOG_REL)
        .map_err(no_home)?;
    let path = file.as_path();
    own.ensure_dir(&dir).map_err(|e| {
        (
            "io_failed",
            crate::common::said::Said::with_raw(
                copy_text(
                    "beAssetCatalog.write.mkdirFailed",
                    &[
                        ("dir", &dir.as_path().display().to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                &e,
            ),
        )
    })?;
    let held = own.lock(&dir).map_err(|e| {
        crate::stream::inbound::spec::Fail::from(("io_failed", crate::common::said::Said::from(e)))
    })?;
    let mut cat = match read_at(path) {
        Read::Present(c) => c,
        Read::Absent => fresh(new_machine_id()),
        Read::Unreadable(why) => {
            return Err(crate::stream::inbound::spec::Fail::from((
                "catalog_unreadable",
                why,
            )))
        }
    };
    let Scanned {
        assets,
        projects,
        problems,
        shared_mcp,
    } = scanned;
    let mut changed = refresh_self(&mut cat, assets, projects, label, now);
    changed |= mark_shared_mcp(&mut cat, shared_mcp, now);
    if let Some(inc) = incoming {
        changed |= merge(&mut cat, inc);
    }
    if let Some((key, text)) = note {
        changed |= set_note(&mut cat, key, text, now);
    }
    let mut dirty = changed | note_known(&mut cat, now);
    if visit {
        cat.visits = Visits {
            prev: cat.visits.last,
            last: now,
        };
        dirty = true;
    }
    if dirty || !path.exists() {
        write_at(own, &file, &held, &cat)
            .map_err(|e| crate::stream::inbound::spec::Fail::from(("io_failed", e)))?;
    }
    Ok((cat, problems, changed))
}

fn update_now(
    own: &Own,
    incoming: Option<BTreeMap<String, Snapshot>>,
) -> Result<CatalogReply, crate::stream::inbound::spec::Fail> {
    update_at(own, scan_here(), &machine_label(), incoming)
}

/// 扩展页那一问（**写口**，只从 `stream/inbound/` 递出去）：这台现扫一次、记下，交回并好的整份（`visit` 见 [`update_with`]）。
pub(crate) fn answer_current(
    visit: bool,
    own: &Own,
) -> Result<(Catalog, Vec<String>), crate::stream::inbound::spec::Fail> {
    let (cat, problems, _) =
        update_with(own, scan_here(), &machine_label(), None, visit, now_secs())?;
    Ok((cat, problems))
}

/// 扩展页写备注那一问（**写口**，只从 `stream/inbound/` 递出去）：这台现扫一次、把备注记进自己那一格，交回整份。
pub(crate) fn answer_note(
    key: &str,
    text: &str,
    own: &Own,
) -> Result<Catalog, crate::stream::inbound::spec::Fail> {
    update_noting(own, scan_here(), &machine_label(), (key, text), now_secs()).map(|(c, _)| c)
}

/// `assets-catalog`：这台现扫一次、记下（变了才写），回整份目录 ＋「这台缺什么」。
pub fn answer_catalog(own: &Own) -> Result<CatalogReply, crate::stream::inbound::spec::Fail> {
    update_now(own, None)
}

/// `assets-catalog-merge`：同上，再把 `catalog`（别的后端的整份）并进来。
pub fn answer_merge(
    req: CatalogMergeReq,
    own: &Own,
) -> Result<CatalogReply, crate::stream::inbound::spec::Fail> {
    let incoming = machines_of(req.catalog.machines).map_err(|e| ("bad_args", e))?;
    update_now(own, Some(incoming))
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/asset_catalog_tests.rs"]
mod tests;
