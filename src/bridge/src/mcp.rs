//! F87（#50+#51）本机 / **F87b+F89a（#52 跨机）** MCP 管理。**SS-14 读写分界**（doc/../plan SS-14 + INVARIANTS §1 例外 #5）：
//! - **读**：〔SH1 · V137〕本机远端都问那台后端的 `mcp-read`（user / local / project 三段成品，布局知识住后端适配层）；
//!   远端项目段另有 `read_remote_project_mcp`（经 `files-peek`）。**宽容读**（INVARIANTS §18）：缺/坏/字段缺跳过、server 原样。
//! - **写**：**只** `<dir>/.mcp.json`（增/改/删）。**绝不写 `~/.claude.json` / `settings.json`**——本机经 `mcp_json_path`
//!   硬编码 / **远端**经 `remote_mcp_json_path`+`is_safe_remote_mcp_json` 守卫。SS-G：写仅用户显式触发。
//!   🔴 **〔RW1 · 第四波 09-24〕落盘不在本进程**：两侧都经那台机器的后端（`edit_project_mcp` →
//!   `user_files::edit` → `files-peek` / `files-put`），本机与远端同一条路、只差 origin；
//!   上线的两条命令是 `write_project_mcp_server` / `remove_project_mcp_server`（各吃一个 `origin`）。
//!
//! 〔SH1〕`~/.claude.json` 找哪一份从此只住后端（`agents/claudecode/assets.rs::claude_json`）；monitor 那份三候选〔散文墓碑〕删了。

use crate::copy_table::copy_text;
use crate::origin::{Origin, Route};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// 一条 MCP server 展示项。`server` 原样保留（宽容，未知字段不丢）。camelCase 上 wire。
#[derive(serde::Serialize, Debug, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct McpServerEntry {
    /// `"user"` | `"local"` | `"project"`
    pub scope: String,
    pub name: String,
    /// server 配置原样（`{command,args,env}` 或 `{type:"sse"|"http",url,...}`）。
    // `serde_json::Value` 用 `unknown` 而不是 `any`——前端必须先形状守卫才能读
    // （同 C04c 对 `ApiMessage.content` 的处置；§18 宽容 schema 的读法）。
    #[cfg_attr(test, ts(type = "unknown"))]
    pub server: Value,
    /// 来源文件绝对路径（展示 / 诊断用）。
    pub source_path: String,
}

/// 从一个 `mcpServers` 对象抽条目（宽容：非对象 → 不加）。**纯**，供单测。
fn push_servers(servers: Option<&Value>, scope: &str, source: &str, out: &mut Vec<McpServerEntry>) {
    if let Some(obj) = servers.and_then(Value::as_object) {
        for (name, server) in obj {
            out.push(McpServerEntry {
                scope: scope.to_string(),
                name: name.clone(),
                server: server.clone(),
                source_path: source.to_string(),
            });
        }
    }
}

/// **纯核心**（供单测，不碰文件系统）：给定已解析的 `~/.claude.json`、项目 `.mcp.json` 两个可选
/// Value + 项目目录，合并出三 scope 条目。缺失传 None → 该 scope 空。
fn collect_entries(
    claude_json: Option<&Value>,
    claude_json_src: &str,
    project_mcp: Option<&Value>,
    project_mcp_src: &str,
    project_dir: Option<&str>,
) -> Vec<McpServerEntry> {
    let mut out = Vec::new();
    if let Some(cj) = claude_json {
        // 用户 scope：顶层 mcpServers
        push_servers(cj.get("mcpServers"), "user", claude_json_src, &mut out);
        // local scope：projects[<dir>].mcpServers（用 get 链，dir 作精确 key，免 JSON pointer 转义）
        if let Some(dir) = project_dir {
            let local = cj
                .get("projects")
                .and_then(|p| p.get(dir))
                .and_then(|proj| proj.get("mcpServers"));
            push_servers(local, "local", claude_json_src, &mut out);
        }
    }
    // 项目 scope：<dir>/.mcp.json 顶层 mcpServers
    push_servers(
        project_mcp.and_then(|m| m.get("mcpServers")),
        "project",
        project_mcp_src,
        &mut out,
    );
    out
}

/// 〔SH1 · V137〕MCP 列表问那台后端（`mcp-read`，本机远端同一条路）：`.claude.json` 的布局只住后端适配层那一格
/// （`agents/claudecode/mcp.rs`），monitor 这边从前那两个读者（本机直读三候选 · 远端 SSH `cat`）〔散文墓碑〕退役。
pub(crate) struct McpRead {
    pub(crate) entries: Vec<McpServerEntry>,
    pub(crate) dirs: Vec<String>,
}

/// 问的期限（读一份 `.claude.json`，重度用户可数 MB）。
const MCP_READ_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);

/// `mcp-read` 的成品 → 条目 ＋ 项目目录 —— 纯函数，按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 两端契约对不上）；
/// `problems` 那几句写进日志（读法宽容：坏的那份照旧当空段，但说出来）。
pub(crate) fn decode_mcp_read(who: &str, v: &Value) -> Result<McpRead, String> {
    let bad = |why: &str| copy_text("rsMcp.read.shape", &[("who", who), ("why", why)]);
    let o = v
        .as_object()
        .filter(|o| o.len() == 3)
        .ok_or_else(|| bad("not exactly {entries, dirs, problems}"))?;
    let arr = |k: &str| o.get(k).and_then(Value::as_array).ok_or_else(|| bad(k));
    let strs = |k: &str| -> Result<Vec<String>, String> {
        arr(k)?
            .iter()
            .map(|x| x.as_str().map(str::to_string).ok_or_else(|| bad(k)))
            .collect()
    };
    let mut entries = Vec::new();
    for e in arr("entries")? {
        let e = e
            .as_object()
            .filter(|e| e.len() == 4)
            .ok_or_else(|| bad("entry"))?;
        let s = |k: &str| {
            e.get(k)
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| bad(k))
        };
        let scope = s("scope")?;
        if !matches!(scope.as_str(), "user" | "local" | "project") {
            return Err(bad("scope"));
        }
        entries.push(McpServerEntry {
            scope,
            name: s("name")?,
            server: e.get("server").cloned().ok_or_else(|| bad("server"))?,
            source_path: s("sourcePath")?,
        });
    }
    for p in strs("problems")? {
        tracing::warn!("MCP：{who} 那份读不出来 —— {p}");
    }
    Ok(McpRead {
        entries,
        dirs: strs("dirs")?,
    })
}

/// 问 `origin` 那台机器的后端要 MCP 列表（本机 = `<local>` 长连接）。
pub(crate) async fn mcp_on(origin: &Origin, project_dir: Option<&str>) -> Result<McpRead, String> {
    let who = crate::backend::control::frame_query::who(origin);
    let args = match project_dir.map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => serde_json::json!({ "projectDir": d }),
        None => serde_json::json!({}),
    };
    let v = crate::backend::control::frame_query::call(
        origin,
        "mcp-read",
        args,
        crate::backend::control::frame_query::Deadline::within(MCP_READ_BUDGET),
    )
    .await?;
    decode_mcp_read(&who, &v)
}

/// F87 读命令：跨 scope 展示项目的 MCP servers（本机）。宽容——缺/坏文件返回空段。〔SH1〕问本机后端 `mcp-read`。
#[tauri::command]
pub async fn read_mcp_servers(project_dir: Option<String>) -> Result<Vec<McpServerEntry>, String> {
    Ok(mcp_on(&Origin::local(), project_dir.as_deref())
        .await?
        .entries)
}

/// F87 读命令：候选项目目录（`~/.claude.json` 的 `projects` 键，排序）——前端 datalist 自动补全用。
/// 设置窗独立于主窗口、拿不到活跃会话 cwd，故让用户从「用过的项目」里选/补全。宽容：缺/坏 → 空。§10 spawn_blocking。
///
/// 🔴 **〔步 12·C 2026-09-20〕这里原先是两条命令**（`list_mcp_project_dirs` ＋
/// `list_remote_mcp_project_dirs`〔散文墓碑〕），今天是一条带 `origin` 的。`设计/00 §2.5 ①` 逐字
/// 「同义双份命令合成一条带 origin 参数的」。
///
/// 两侧问的是**同一份文件的同一个键**；〔SH1 · V137〕今天连「字节从哪来」都是同一条路（那台后端的 `mcp-read`），
/// 这正是 `INVARIANTS §40`「本地 ＝ 不走 ssh 的远端」那一句在命令面上的样子。
///
/// ⚠ **本机是 `Origin::local()`（线上 `"<local>"`），不是 `null`** ——
/// 「没说」那一支由 [`Origin::route`] 当场拒掉，理由见它的头注。
#[tauri::command]
pub async fn list_mcp_project_dirs(origin: Origin) -> Result<Vec<String>, String> {
    // 〔SH1 · V137〕本机远端同一条路：问那台后端的 `mcp-read`（它的 `dirs`）。`route` 只为当场拒掉「没说」那一形。
    origin.route("list_mcp_project_dirs")?;
    Ok(mcp_on(&origin, None).await?.dirs)
}

/// F87b③：跨机读远端 MCP 的 user 段（顶层 `mcpServers` = 机器全局 MCP）。**只读**。
/// 〔SH1 · V137〕问那台后端的 `mcp-read`（不带项目目录 ⇒ 只有 user 段），不再经拨号链路 `cat ~/.claude.json`。
#[tauri::command]
pub async fn read_remote_mcp_servers(origin: String) -> Result<Vec<McpServerEntry>, String> {
    Ok(mcp_on(&Origin(origin), None).await?.entries)
}

/// F87b③：机器选择器用——返回**已配置且启用**的远端 origin（**canonical** `origin_label()`，后端口径）。
/// 前端据此直接下发给 `read_remote_mcp_servers`，**不自行从原始 config 重推 origin**——避免与后端解析口径
/// 漂移：空 label 回退 host / 重名去重（`box`→`box (#2)`）/ 不完整主机丢弃 都由后端 `load_remote_configs`
/// 统一定义，这里返回的正是 `load_remote_config_by_label` 能解析的那批 label。§10 spawn_blocking（读 config 文件）。
#[tauri::command]
pub async fn list_remote_mcp_origins() -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(|| {
        crate::load_remote_configs()
            .iter()
            .map(|c| c.origin_label())
            .collect()
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))
}

/// 本机 `<dir>/.mcp.json`（**写侧唯一出口，硬编码 `.mcp.json`**）。
/// 〔RW1 · 第四波 09-24〕多一道：项目目录必须是**绝对路径** —— 这条路径交给后端去解析，
/// 相对路径在后端那个进程里指的是别处。
fn mcp_json_path(project_dir: &str) -> Result<PathBuf, String> {
    let d = project_dir.trim();
    if d.is_empty() {
        return Err(copy_text("rsMcp.path.emptyDir", &[]).into());
    }
    if !Path::new(d).is_absolute() {
        return Err(copy_text(
            "rsMcp.path.notAbsolute",
            &[("dir", &format!("{:?}", d))],
        ));
    }
    Ok(Path::new(d).join(".mcp.json"))
}

/// **纯核心**（F89a 抽出，本机/远端复用、可测）：把一条 server upsert 进 `.mcp.json` Value。名空拒。
pub(crate) fn upsert_mcp_server_value(
    root: &mut Value,
    name: String,
    server: Value,
) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err(copy_text("rsMcp.upsert.emptyName", &[]).into());
    }
    let obj = root
        .as_object_mut()
        .ok_or_else(|| copy_text("rsMcp.mcpJson.rootNotObject", &[]))?;
    let servers = obj
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    let smap = servers
        .as_object_mut()
        .ok_or_else(|| copy_text("rsMcp.mcpJson.serversNotObject", &[]))?;
    smap.insert(name, server);
    Ok(())
}

/// **纯核心**（F89a 抽出，本机/远端复用、可测）：从 `.mcp.json` Value 删一条 server，返回是否真删。
fn remove_mcp_server_value(root: &mut Value, name: &str) -> Result<bool, String> {
    let obj = root
        .as_object_mut()
        .ok_or_else(|| copy_text("rsMcp.mcpJson.rootNotObject", &[]))?; // 根对象守卫（对齐 write）
    Ok(obj
        .get_mut("mcpServers")
        .and_then(|m| m.as_object_mut())
        .map(|smap| smap.remove(name).is_some())
        .unwrap_or(false))
}

/// F89a：远端 `.mcp.json` 写路径守卫（SS-14 远端对端 + SS-G 用户显式触发）。绝对路径 + 尾 `/.mcp.json` + 无 `..`。
fn is_safe_remote_mcp_json(path: &str) -> bool {
    !path.contains("..") && path.starts_with('/') && path.ends_with("/.mcp.json") && {
        // 尾 `/.mcp.json` 前须有非空项目目录段（不接受裸 `/.mcp.json`）。
        path.len() > "/.mcp.json".len()
    }
}

/// F89a：`<remote_dir>/.mcp.json` 远端路径（**写侧唯一出口**，硬编码 `.mcp.json` + `is_safe_remote_mcp_json` 守卫，
/// SS-14 远端对端：绝不写 `~/.claude.json`/settings.json）。
fn remote_mcp_json_path(project_dir: &str) -> Result<String, String> {
    let d = project_dir.trim().trim_end_matches('/');
    if d.is_empty() || !d.starts_with('/') {
        return Err(copy_text("rsMcp.remote.pathNotAbsolute", &[]).into());
    }
    let p = format!("{d}/.mcp.json");
    if !is_safe_remote_mcp_json(&p) {
        return Err(copy_text(
            "rsMcp.remote.pathIllegal",
            &[("path", &p.to_string())],
        ));
    }
    Ok(p)
}

/// 🔴 〔RW1 · 第四波 · 2026-09-24〕**项目 `.mcp.json` 的唯一写法**：本机与远端同一条路 ——
/// 经那台机器的后端（`user_files::edit` → `files-peek` / `files-put`），只差 origin。
///
/// 用户裁「只允许后端的文件管理部分写文件」**也管本机**、远端 F89a「按推荐改」⇒ 从前两份落盘
/// （本机 `write_json_atomic`〔散文墓碑〕走 `ReplaceFileW`、远端 SFTP `upload_atomic`）都走了，
/// 改那份 JSON 的纯核心照旧只有一份（[`upsert_mcp_server_value`] / [`remove_mcp_server_value`]）。
///
/// - 已存在但解析失败 ⇒ **拒绝覆盖**（不拿骨架盖掉现有的 server，与从前两侧同一句承诺）。
/// - 不存在 ⇒ 从骨架 `{"mcpServers":{}}` 算起；`change` 回 `false`（删一条本来就不在的）⇒ 一个字节不写、不建文件。
/// - 不留备份文件：`.mcp.json` 住在用户的仓里，每改一次留一份带时间戳的备份就是往仓里撒垃圾；
///   替换本身是原子的、写后回读逐字比对、不符回滚（规则住后端）。⚠ 从前本机那份 `.json.bak` 因此不再出现。
async fn edit_project_mcp(
    door: &impl crate::user_files::Door,
    target: &str,
    mut change: impl FnMut(&mut Value) -> Result<bool, String>,
) -> Result<(), String> {
    let (root, rel) = split_target(target)?;
    crate::user_files::edit(door, root, rel, false, false, |existing| {
        plan_project_mcp(target, existing, &mut change)
    })
    .await
    .map(|_| ())
}

/// 落点由两个出口之一给（[`mcp_json_path`] / [`remote_mcp_json_path`]），这里只把它切成「根 ＋ 那一段」交给后端。
///
/// 〔AS1 · 第四波 4B〕从 [`edit_project_mcp`] 里原样抽出来（行为逐字不变）：推 / 拉（`mcp_sync.rs`）用同一把刀切。
pub(crate) fn split_target(target: &str) -> Result<(&str, &str), String> {
    target
        .rsplit_once(['/', '\\'])
        .filter(|(r, n)| !r.is_empty() && !n.is_empty())
        .ok_or_else(|| {
            copy_text(
                "rsMcp.target.noProjectDir",
                &[("target", &target.to_string())],
            )
        })
}

/// 🔴 **`.mcp.json` 的原文怎么变成新原文 —— 只有这一处**（纯）：不存在 ⇒ 从骨架算起；已存在但解析失败 ⇒
/// **拒绝覆盖**；`change` 回 `false` ⇒ 没事可做（`None`）。
///
/// 〔AS1 · 第四波 4B〕从 [`edit_project_mcp`] 的规划闭包里原样抽出来（行为逐字不变）：单条「添加 / 更新 / 删」
/// 与推 / 拉（`mcp_sync.rs`）走同一份 —— 推 / 拉合进去的每一条也是 [`upsert_mcp_server_value`]。
pub(crate) fn plan_project_mcp(
    what: &str,
    existing: Option<&str>,
    change: &mut impl FnMut(&mut Value) -> Result<bool, String>,
) -> Result<Option<String>, String> {
    let mut v = match existing {
        None => serde_json::json!({ "mcpServers": {} }),
        Some(t) => serde_json::from_str(t.trim_start_matches('\u{feff}')).map_err(|e| {
            copy_text(
                "rsMcp.plan.parseFailed",
                &[("what", &what.to_string()), ("e", &e.to_string())],
            )
        })?,
    };
    if !change(&mut v)? {
        return Ok(None);
    }
    serde_json::to_string_pretty(&v)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// 〔AS1 · 第四波 4B〕按 origin 取项目 `.mcp.json` 的落点 —— 两个出口（[`mcp_json_path`] / [`remote_mcp_json_path`]）
/// 照旧是仅有的两个，这里只按「本机 / 远端」挑一个（与两条写命令里的挑法同形）。推 / 拉用。
pub(crate) fn project_mcp_target(
    origin: &Origin,
    command: &str,
    project_dir: &str,
) -> Result<String, String> {
    match origin.route(command)? {
        Route::Local => Ok(mcp_json_path(project_dir)?.to_string_lossy().into_owned()),
        Route::Remote(_) => remote_mcp_json_path(project_dir),
    }
}

/// F87 写命令：增 / 改项目 `.mcp.json` 里一条 MCP server。**只碰 `<dir>/.mcp.json`。**
///
/// 🔴 **〔步 12·C 收尾 2026-09-20〕两条合成了一条带 `origin` 的**（本机那条 ＋ 远端那条）。
/// `设计/00 §2.5 ①` 逐字「同义双份命令合成一条带 origin 参数的」。
/// 〔RW1 · 第四波 09-24〕合并时留下的那一处差别（「字节走哪条路」）也没了：两侧都经那台机器的后端写
/// （[`edit_project_mcp`]）；两支今天只差**落点怎么验**（本机可以是盘符路径，远端只收 POSIX 绝对路径）与门开在哪个 origin。
///
/// ⚠ 本机 ＝ `Origin::local()`（线上 `"<local>"`），**不是 `null`** ——
/// 「没说」那一支由 [`Origin::route`] 当场拒掉，理由见它的头注。
#[tauri::command]
pub async fn write_project_mcp_server(
    origin: Origin,
    project_dir: String,
    name: String,
    server: Value,
) -> Result<(), String> {
    match origin.route("write_project_mcp_server")? {
        Route::Local => {
            let target = mcp_json_path(&project_dir)?.to_string_lossy().into_owned();
            let door = crate::user_files::BackendDoor::new(Origin::local());
            edit_project_mcp(&door, &target, |v| {
                upsert_mcp_server_value(v, name.clone(), server.clone()).map(|()| true)
            })
            .await
        }
        Route::Remote(host) => write_remote_mcp_server(host, project_dir, name, server).await,
    }
}

/// F87 写命令：删项目 `.mcp.json` 里一条 MCP server。**只碰 `<dir>/.mcp.json`。**
///
/// 凭据与 [`write_project_mcp_server`] 同形，纯核心换成 [`remove_mcp_server_value`]。
/// 「不存在就 no-op」两侧同一句：文件不在或条目不在 ⇒ 规划回「没事可做」，一个字节不写、不建文件。
#[tauri::command]
pub async fn remove_project_mcp_server(
    origin: Origin,
    project_dir: String,
    name: String,
) -> Result<(), String> {
    match origin.route("remove_project_mcp_server")? {
        Route::Local => {
            let target = mcp_json_path(&project_dir)?.to_string_lossy().into_owned();
            let door = crate::user_files::BackendDoor::new(Origin::local());
            edit_project_mcp(&door, &target, |v| remove_mcp_server_value(v, &name)).await
        }
        Route::Remote(host) => remove_remote_mcp_server(host, project_dir, name).await,
    }
}

// ───────────────────────── F89a：远端项目级 MCP 读写 ─────────────────────────
// 只读铁律边界：读=只读；**写/删仅用户显式触发（SS-G 豁免）**、写面**只** `<dir>/.mcp.json`
// （`remote_mcp_json_path` 硬编码 + `is_safe_remote_mcp_json` 守卫，SS-14 远端对端）。
// 〔RW1〕写经那台远端的后端（`user_files`）；〔SR1b · 2026-09-24〕**读也经它**（`files-peek`，与写同一个家、同一道围栏）。
// 〔墓碑 —— 从前读这一半是界面进程自己开一条 SFTP 读（`read_remote_mcp_value`〔散文墓碑〕：`try_exists` 失败即拒 ·
//  path 缺而 `.bak` 在就从 `.bak` 恢复）。那条「崩溃恢复」治的是旧 SFTP 原子写中断留下的 `.bak`；写改经后端之后
//  那一形不再产生（后端那一份序列自己回滚），读也就不必替它兜。〕

/// F89a：读远端某项目的 `.mcp.json`（project scope 条目）。**只读**。缺/坏 → 空段（宽容，读侧不阻断）。
#[tauri::command]
pub async fn read_remote_project_mcp(
    origin: String,
    project_dir: String,
) -> Result<Vec<McpServerEntry>, String> {
    let cfg = crate::load_remote_config_by_label(&origin).ok_or_else(|| {
        copy_text(
            "rsMcp.remote.notConfigured",
            &[("machine", &origin.to_string())],
        )
    })?;
    let path = remote_mcp_json_path(&project_dir)?;
    let (root_dir, rel) = path
        .rsplit_once('/')
        .filter(|(r, n)| !r.is_empty() && !n.is_empty())
        .ok_or_else(|| {
            copy_text(
                "rsMcp.target.pathNoProjectDir",
                &[("path", &path.to_string())],
            )
        })?;
    let door = crate::user_files::BackendDoor::new(crate::origin::Origin(cfg.origin_label()));
    // 读侧宽容：不存在 / 读不出 / 坏 → 空（不像写侧那样 Err）。
    // 〔W5-VIS · E 吞错普查点名〕读不出 / 坏那两形**说出来**（原先三个 `.ok()` 折成「空」，与「没有」同形、一句话都没有）。
    let root = match crate::user_files::Door::peek(&door, root_dir, rel).await {
        Ok(p) => match p.text {
            None => None,
            Some(t) => match serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')) {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!(
                        "MCP：[{}] {path} 不是合法 JSON（{e}），这一份当作没有",
                        cfg.origin_label()
                    );
                    None
                }
            },
        },
        Err(e) => {
            tracing::warn!(
                "MCP：读不出 [{}] {path}（{e}），这一份当作没有",
                cfg.origin_label()
            );
            None
        }
    }
    .unwrap_or_else(|| serde_json::json!({ "mcpServers": {} }));
    let src = format!("[{}] {path}", cfg.origin_label());
    Ok(collect_entries(None, "", Some(&root), &src, None))
}

/// F89a：增/改远端项目 `.mcp.json` 一条 server —— [`write_project_mcp_server`] 的远端那一支。
///
/// 🔴 〔RW1 · 第四波 09-24〕F89a 按用户裁「按推荐改」：**经那台远端的后端写**（[`edit_project_mcp`]），
/// 不再 SFTP 读改写 ＋ `upload_atomic`。它与本机那一支只差两件：落点过远端那道守卫
/// （[`remote_mcp_json_path`]：绝对 + 尾 `/.mcp.json` + 无 `..` + 非裸）· 门开在远端那个 origin 上。
/// ⚠ 参数名叫 `host`（不叫 `origin`）：它拿到的是**已经分过本机**的机器名（理由同 `ORIGIN_MIGRATION_CEILING` 上方那一段）。
pub(crate) async fn write_remote_mcp_server(
    host: &str,
    project_dir: String,
    name: String,
    server: Value,
) -> Result<(), String> {
    let target = remote_mcp_json_path(&project_dir)?;
    let door = crate::user_files::BackendDoor::new(Origin(host.to_string()));
    edit_project_mcp(&door, &target, |v| {
        upsert_mcp_server_value(v, name.clone(), server.clone()).map(|()| true)
    })
    .await
}

/// F89a：删远端项目 `.mcp.json` 一条 server —— [`remove_project_mcp_server`] 的远端那一支（同上：经远端后端写）。
pub(crate) async fn remove_remote_mcp_server(
    host: &str,
    project_dir: String,
    name: String,
) -> Result<(), String> {
    let target = remote_mcp_json_path(&project_dir)?;
    let door = crate::user_files::BackendDoor::new(Origin(host.to_string()));
    edit_project_mcp(&door, &target, |v| remove_mcp_server_value(v, &name)).await
}

#[cfg(test)]
#[path = "../../../tests/bridge/mcp_tests.rs"]
mod tests;
