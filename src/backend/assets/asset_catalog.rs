//! 资产目录：每台后端把它看到的 skill 与 MCP（用户级 ＋ 这台上开过会话的项目里的）记成一份目录，目录在后端之间自动对上；
//! 装要用户点，装时内容原样拷 ＋ 标可疑项，不替用户改写。
//!
//! - 是后端自有状态（`readonly_guard` 第四层）：`~/.cc-monitor/assets-catalog.json`，全仓只有本模块写它，写口（`answer_*` 三条）
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
pub struct Asset {
    pub kind: String,
    pub name: String,
    /// 那台上的项目目录（项目级）；`None` = 用户级。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// 它在那台上住哪（skill：那个目录；MCP：那份配置文件）。只给人看、给「在文件窗口里打开」用，不参与判定。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    pub digest: String,
    /// 给人看的摘要（skill：description / 文件数 / 字节数；MCP：去掉密钥值之后的定义）。
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
pub struct Note {
    pub text: String,
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
                notice.push(copy_text(
                    "beAssetCatalog.digest.walkFailed",
                    &[("e", &e.to_string())],
                ));
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
        let len = match std::fs::metadata(ent.path()) {
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
        match crate::common::fs::read_regular_capped(ent.path(), SKILL_MAX_FILE_BYTES) {
            Ok(body) => {
                if body.contains(&0) || std::str::from_utf8(&body).is_err() {
                    binary = true;
                }
                f.part(&body);
            }
            Err(error) => {
                notice.push(copy_text(
                    "beAssetCatalog.digest.lenOnly",
                    &[("rel", &rel.to_string()), ("e", &error.to_string())],
                ));
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

/// 这台上开过会话的项目目录：取历史清单那一份（`--list-projects` 的每一行的 `projectPath`），不另起一份扫描。
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

/// 「别的机器有的，这台怎样」—— 每个（种类, 名字）一行，只列别处有的。判定只在这里。
pub fn rows(cat: &Catalog) -> Vec<Value> {
    let mine: Vec<&Asset> = cat
        .machines
        .get(&cat.self_id)
        .map(|s| s.assets.iter().collect())
        .unwrap_or_default();
    let mut by_key: BTreeMap<(String, String), Vec<Value>> = BTreeMap::new();
    for (id, snap) in &cat.machines {
        if *id == cat.self_id {
            continue;
        }
        for a in &snap.assets {
            by_key
                .entry((a.kind.clone(), a.name.clone()))
                .or_default()
                .push(json!({
                    "machine": id,
                    "project": a.project,
                    "digest": a.digest,
                    "summary": a.summary,
                }));
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
            } else if here.iter().any(|a| {
                from.iter()
                    .any(|f| f.get("digest").and_then(Value::as_str) == Some(a.digest.as_str()))
            }) {
                HERE_SAME
            } else {
                HERE_DIFFERS
            };
            json!({ "kind": kind, "name": name, "state": state, "from": from })
        })
        .collect()
}

// ───────────────────────── 线上形状 ─────────────────────────

/// 一份目录的线上形状（两条命令共用；`assets-catalog-merge` 收的 `catalog` 也是这个形状）。
pub fn wire(cat: &Catalog, problems: &[String], changed: bool, path: Option<&Path>) -> Value {
    let machines: Vec<Value> = cat
        .machines
        .iter()
        .map(|(id, s)| {
            json!({
                "id": id,
                "label": s.label,
                "gen": s.gen,
                "seenAt": s.seen_at,
                "assets": s.assets,
                "projectDirs": s.project_dirs,
                "notes": s.notes,
                "sharedMcp": s.shared_mcp,
            })
        })
        .collect();
    json!({
        "self": cat.self_id,
        "machines": machines,
        "rows": rows(cat),
        "problems": problems,
        "changed": changed,
        "path": path.map(|p| p.display().to_string()),
    })
}

/// 线上 `catalog` → 各台快照（入参校验：缺格 / 类型不对 / 种类不在闭集 ⇒ `bad_args`，不猜）。
pub fn machines_from_wire(v: &Value) -> Result<BTreeMap<String, Snapshot>, String> {
    let arr =
        v.get("machines")
            .and_then(Value::as_array)
            .ok_or(crate::common::contract::malformed(
                "`catalog.machines` missing or not an array",
            ))?;
    let mut out = BTreeMap::new();
    for m in arr {
        let id = m
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty() && s.len() <= MAX_ID_BYTES)
            .ok_or(crate::common::contract::malformed(
                "a machine `id` is missing, empty or too long",
            ))?;
        let label =
            m.get("label")
                .and_then(Value::as_str)
                .ok_or(crate::common::contract::malformed(
                    "a machine `label` is missing or not a string",
                ))?;
        let gen =
            m.get("gen")
                .and_then(Value::as_u64)
                .ok_or(crate::common::contract::malformed(
                    "a machine `gen` is missing or not a non-negative integer",
                ))?;
        let seen_at =
            m.get("seenAt")
                .and_then(Value::as_u64)
                .ok_or(crate::common::contract::malformed(
                    "a machine `seenAt` is missing or not a non-negative integer",
                ))?;
        let assets: Vec<Asset> = serde_json::from_value(m.get("assets").cloned().ok_or(
            crate::common::contract::malformed("a machine `assets` is missing"),
        )?)
        .map_err(|e| {
            crate::common::contract::malformed(&format!(
                "a machine `assets` has the wrong shape: {e}"
            ))
        })?;
        let project_dirs: Vec<String> = match m.get("projectDirs") {
            None | Some(Value::Null) => Vec::new(),
            Some(v) => serde_json::from_value(v.clone()).map_err(|e| {
                crate::common::contract::malformed(&format!(
                    "a machine `projectDirs` must be an array of strings: {e}"
                ))
            })?,
        };
        let notes: BTreeMap<String, Note> = match m.get("notes") {
            None | Some(Value::Null) => BTreeMap::new(),
            Some(v) => serde_json::from_value(v.clone()).map_err(|e| {
                crate::common::contract::malformed(&format!(
                    "a machine `notes` must map entries to {{text, rev}}: {e}"
                ))
            })?,
        };
        let shared_mcp = match m.get("sharedMcp") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => {
                return Err(crate::common::contract::malformed(
                    "a machine `sharedMcp` must be a boolean",
                ))
            }
        };
        if let Some(a) = assets.iter().find(|a| !KINDS.contains(&a.kind.as_str())) {
            return Err(crate::common::contract::malformed(&format!(
                "asset kind `{}` is not one of {KINDS:?}",
                a.kind
            )));
        }
        out.insert(
            id.to_string(),
            Snapshot {
                label: label.to_string(),
                gen,
                seen_at,
                assets,
                project_dirs,
                notes,
                shared_mcp,
            },
        );
    }
    Ok(out)
}

// ───────────────────────── 这台机器的事实 ─────────────────────────

fn home() -> Option<PathBuf> {
    crate::platform::paths::home_dir()
}

/// 这台机器上目录文件的路径（`~/.cc-monitor/` 与 `backend.json` 同一个家）。
pub fn catalog_path() -> Option<PathBuf> {
    Some(
        home()?
            .join(crate::control::exit_policy::DIR_NAME)
            .join(FILE_NAME),
    )
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
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 生成这台的 id：（家目录, 称呼, 纳秒, pid）的摘要。只在目录文件不存在时调一次。
fn new_machine_id() -> String {
    let nanos = std::time::SystemTime::now()
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
fn write_at(path: &Path, cat: &Catalog) -> Result<(), crate::common::said::Said> {
    crate::common::own_state::write_json(path, cat)
}

//
// 读—改—写整段在那个目录的跨进程锁里（`platform/lock.rs`，[`update_at`] 开头拿）：首建时第二个进程读到第一个写下的 `self`、
// 不再生出第二个 id；同一台两个进程各自 +1 代数变成串行。

/// 这台机器现扫一次、并进 `incoming`（若有）、真变了才落盘。**可喂夹具**：路径与扫描结果由调用方给。
pub fn update_at(
    path: &Path,
    scanned: Scanned,
    label: &str,
    incoming: Option<BTreeMap<String, Snapshot>>,
) -> Result<Value, (&'static str, String)> {
    let (cat, problems, changed) = update_with(path, scanned, label, incoming, false, now_secs())?;
    Ok(wire(&cat, &problems, changed, Some(path)))
}

/// [`update_at`] 的本体：交回并好的整份（扩展页的表从它算）。`visit` ⇒ 这一趟算「来看了一次」（挪 [`Visits`]）。
/// 回 `(目录, 读不出来的那几份, 目录真变了没有)`；「新见到」的记账与来看那两格变了也落盘，但不算目录变了（不触发扇出）。
pub fn update_with(
    path: &Path,
    scanned: Scanned,
    label: &str,
    incoming: Option<BTreeMap<String, Snapshot>>,
    visit: bool,
    now: u64,
) -> Result<(Catalog, Vec<String>, bool), (&'static str, String)> {
    update_core(path, scanned, label, incoming, visit, None, now)
}

/// [`update_with`] 再加一笔备注（`(条目键, 正文)`，见 [`set_note`]）：同一把锁里现扫 · 记备注 · 落盘。
pub fn update_noting(
    path: &Path,
    scanned: Scanned,
    label: &str,
    note: (&str, &str),
    now: u64,
) -> Result<(Catalog, bool), (&'static str, String)> {
    update_core(path, scanned, label, None, false, Some(note), now).map(|(c, _, ch)| (c, ch))
}

fn update_core(
    path: &Path,
    scanned: Scanned,
    label: &str,
    incoming: Option<BTreeMap<String, Snapshot>>,
    visit: bool,
    note: Option<(&str, &str)>,
    now: u64,
) -> Result<(Catalog, Vec<String>, bool), (&'static str, String)> {
    let dir = path.parent().ok_or((
        "io_failed",
        copy_text(
            "beAssetCatalog.write.noParent",
            &[("path", &path.display().to_string())],
        ),
    ))?;
    // 只建那一层、建的那一下就是 0700（`own_dir`：后端建自家目录的那一个函数）。挪到拿锁之前：锁的是这个目录，它得先在。
    crate::common::own_dir::ensure_private_dir(dir).map_err(|e| {
        (
            "io_failed",
            copy_text(
                "beAssetCatalog.write.mkdirFailed",
                &[("dir", &dir.display().to_string()), ("e", &e.to_string())],
            ),
        )
    })?;
    let _lock = crate::platform::lock::hold(dir).map_err(|e| {
        (
            "io_failed",
            crate::common::said::Said::from(e).said_logging_raw(),
        )
    })?;
    let mut cat = match read_at(path) {
        Read::Present(c) => c,
        Read::Absent => fresh(new_machine_id()),
        Read::Unreadable(why) => return Err(("catalog_unreadable", why.said_logging_raw())),
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
        write_at(path, &cat).map_err(|e| ("io_failed", e.said_logging_raw()))?;
    }
    Ok((cat, problems, changed))
}

fn update_now(
    incoming: Option<BTreeMap<String, Snapshot>>,
) -> Result<Value, (&'static str, String)> {
    let path =
        catalog_path().ok_or(("io_failed", copy_text("beAssetCatalog.write.noHome", &[])))?;
    update_at(&path, scan_here(), &machine_label(), incoming)
}

/// 扩展页那一问（**写口**，只从 `stream/inbound/` 递出去）：这台现扫一次、记下，交回并好的整份（`visit` 见 [`update_with`]）。
pub(crate) fn answer_current(
    visit: bool,
) -> Result<(Catalog, Vec<String>), (&'static str, String)> {
    let path =
        catalog_path().ok_or(("io_failed", copy_text("beAssetCatalog.write.noHome", &[])))?;
    let (cat, problems, _) = update_with(
        &path,
        scan_here(),
        &machine_label(),
        None,
        visit,
        now_secs(),
    )?;
    Ok((cat, problems))
}

/// 扩展页写备注那一问（**写口**，只从 `stream/inbound/` 递出去）：这台现扫一次、把备注记进自己那一格，交回整份。
pub(crate) fn answer_note(key: &str, text: &str) -> Result<Catalog, (&'static str, String)> {
    let path =
        catalog_path().ok_or(("io_failed", copy_text("beAssetCatalog.write.noHome", &[])))?;
    update_noting(
        &path,
        scan_here(),
        &machine_label(),
        (key, text),
        now_secs(),
    )
    .map(|(c, _)| c)
}

/// `assets-catalog`：这台现扫一次、记下（变了才写），回整份目录 ＋「这台缺什么」。
pub fn answer_catalog(_args: &Value) -> Result<Value, (&'static str, String)> {
    update_now(None)
}

/// `assets-catalog-merge`：同上，再把 `args.catalog`（别的后端的整份）并进来。
pub fn answer_merge(args: &Value) -> Result<Value, (&'static str, String)> {
    let cat = args.get("catalog").ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `catalog`"),
    ))?;
    let incoming = machines_from_wire(cat).map_err(|e| ("bad_args", e))?;
    update_now(Some(incoming))
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/asset_catalog_tests.rs"]
mod tests;
