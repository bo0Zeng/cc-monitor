//! F87（#50+#51）本机 / **F87b+F89a（#52 跨机）** MCP 管理。**SS-14 读写分界**（doc/../plan SS-14 + INVARIANTS §1 例外 #5）：
//! - **读**：本机跨 scope（用户 `~/.claude.json` 顶层 + local `projects[<dir>]` + 项目 `<dir>/.mcp.json`）+
//!   **远端** user scope（`read_remote_mcp_servers`：SSH-exec cat 远端 `~/.claude.json`）+ **远端项目**
//!   （`read_remote_project_mcp`：SFTP 读远端 `<dir>/.mcp.json`）。**宽容读**（INVARIANTS §18）：缺/坏/字段缺跳过、server 原样。
//! - **写**：**只** `<dir>/.mcp.json`（增/改/删）。**绝不写 `~/.claude.json` / `settings.json`**——本机经 `mcp_json_path`
//!   硬编码 / **远端**经 `remote_mcp_json_path`+`is_safe_remote_mcp_json` 守卫。SS-G：写仅用户显式触发。
//!   🔴 **〔RW1 · 第四波 09-24〕落盘不在本进程**：两侧都经那台机器的后端（`edit_project_mcp` →
//!   `user_files::edit` → `files-peek` / `files-put`），本机与远端同一条路、只差 origin；
//!   上线的两条命令是 `write_project_mcp_server` / `remove_project_mcp_server`（各吃一个 `origin`）。
//!
//! `~/.claude.json` 路径有变体（`CLAUDE_CONFIG_DIR` vs `$HOME`），故 `claude_json_candidates` 取多候选、
//! 读第一个存在的——防御式，schema 真机可能变，不硬假设完整。

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

/// `~/.claude.json` 候选路径（防御多变体：`CLAUDE_CONFIG_DIR` / `$HOME`）。取第一个存在的。
fn claude_json_candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(cfg) = std::env::var("CLAUDE_CONFIG_DIR") {
        let p = PathBuf::from(&cfg);
        v.push(p.join(".claude.json")); // $CLAUDE_CONFIG_DIR/.claude.json
        if let Some(parent) = p.parent() {
            v.push(parent.join(".claude.json"));
        }
    }
    if let Some(home) = dirs::home_dir() {
        v.push(home.join(".claude.json")); // 经典位置
    }
    v
}

fn first_existing(cands: &[PathBuf]) -> Option<PathBuf> {
    cands.iter().find(|p| p.is_file()).cloned()
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

/// 宽容读一个 JSON 文件为 Value（缺 / 坏 → None，不报错）。§3：解析前剥 BOM（Claude 写的
/// 文件偶带 UTF-8 BOM，全库读端统一剥，见 parser.rs/tasks.rs/history.rs 等）。
fn read_json_lenient(path: &Path) -> Option<Value> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(raw.trim_start_matches('\u{feff}')).ok()
}

/// **纯核心**（供单测）：从 `~/.claude.json` Value 抽 `projects` 键（排序）。宽容：非对象 → 空。
fn project_dirs_from(claude_json: &Value) -> Vec<String> {
    let mut dirs: Vec<String> = claude_json
        .get("projects")
        .and_then(Value::as_object)
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    dirs.sort();
    dirs
}

fn read_mcp_servers_impl(project_dir: Option<String>) -> Vec<McpServerEntry> {
    let claude_path = first_existing(&claude_json_candidates());
    let claude_json = claude_path.as_deref().and_then(read_json_lenient);
    let claude_src = claude_path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();

    let (project_mcp, project_src) = match project_dir.as_deref() {
        Some(dir) if !dir.trim().is_empty() => {
            let mcp = Path::new(dir).join(".mcp.json");
            let src = mcp.to_string_lossy().into_owned();
            (read_json_lenient(&mcp), src)
        }
        _ => (None, String::new()),
    };

    collect_entries(
        claude_json.as_ref(),
        &claude_src,
        project_mcp.as_ref(),
        &project_src,
        project_dir.as_deref(),
    )
}

/// F87 读命令：跨 scope 展示项目的 MCP servers。宽容——缺/坏文件返回空段。
/// §10：文件 IO（`~/.claude.json` 重度用户可数 MB）走 `spawn_blocking`，不阻塞 IPC 派发线程。
#[tauri::command]
pub async fn read_mcp_servers(project_dir: Option<String>) -> Result<Vec<McpServerEntry>, String> {
    tokio::task::spawn_blocking(move || read_mcp_servers_impl(project_dir))
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))
}

fn list_mcp_project_dirs_impl() -> Vec<String> {
    first_existing(&claude_json_candidates())
        .as_deref()
        .and_then(read_json_lenient)
        .map(|v| project_dirs_from(&v))
        .unwrap_or_default()
}

/// F87 读命令：候选项目目录（`~/.claude.json` 的 `projects` 键，排序）——前端 datalist 自动补全用。
/// 设置窗独立于主窗口、拿不到活跃会话 cwd，故让用户从「用过的项目」里选/补全。宽容：缺/坏 → 空。§10 spawn_blocking。
///
/// 🔴 **〔步 12·C 2026-09-20〕这里原先是两条命令**（`list_mcp_project_dirs` ＋
/// `list_remote_mcp_project_dirs`），今天是一条带 `origin` 的。`设计/00 §2.5 ①` 逐字
/// 「同义双份命令合成一条带 origin 参数的」。
///
/// **凭什么说这一对是同一件事**（不是按名字判的，按 `真相源/97 §二b` 那两条判据判的）：
/// 两侧问的是**同一份文件的同一个键**（`~/.claude.json` 的 `projects`），
/// 而且**算它的那一份代码本来就只有一份** —— [`project_dirs_from`]。
/// 两侧的差别只在「那个文件的字节从哪来」：本机 `read_json_lenient` 直接读盘，
/// 远端 `fetch_remote_claude_json` 走 SSH `cat`。⇒ 这正是 `INVARIANTS §40`
/// 「本地 ＝ 不走 ssh 的远端」那一句在命令面上的样子。
///
/// ⚠ **本机是 `Origin::local()`（线上 `"<local>"`），不是 `null`** ——
/// 「没说」那一支由 [`Origin::route`] 当场拒掉，理由见它的头注。
#[tauri::command]
pub async fn list_mcp_project_dirs(origin: Origin) -> Result<Vec<String>, String> {
    match origin.route("list_mcp_project_dirs")? {
        Route::Local => tokio::task::spawn_blocking(list_mcp_project_dirs_impl)
            .await
            .map_err(|e| format!("spawn_blocking: {e}")),
        Route::Remote(host) => list_remote_mcp_project_dirs(host).await,
    }
}

/// F87b③：跨机读远端 MCP。**只读**（守 §1：SSH exec `cat` 远端**用户自己**的 `~/.claude.json`，
/// 不写、不驱动远端 agent）。**不依赖未建的 backend**。
/// 命令是**定值、无用户输入插值**（origin 只用于解析 cfg）→ 零注入面；远端 shell 展开变量。
/// 复用纯核心 `collect_entries` 取 **user scope**（顶层 mcpServers = 机器全局 MCP）。local/project scope 是
/// per-项目、跨机无稳定映射，**不取**（见 F87b 计划）。带 30s 超时 + 32MB 上限（config 重度用户可数 MB）。
/// 宽容：缺/坏文件 → 空段（cat 失败 stdout 空 → 解析 None → 空 Vec）。
/// F87b-fix(batch18 审计修)：① **多候选路径**——先试 `$CLAUDE_CONFIG_DIR/.claude.json`、再回退 `$HOME/.claude.json`
/// （覆盖本机 `claude_json_candidates` 的**两个主候选**；原只试单一 `${CLAUDE_CONFIG_DIR:-$HOME}`，当用户把
/// CLAUDE_CONFIG_DIR 指向数据目录却把 .claude.json 留在 $HOME 时静默误报空）。**注**：本机还有第三候选
/// `parent($CLAUDE_CONFIG_DIR)/.claude.json`，跨机侧未覆盖（极罕见：CFGDIR 指子目录、.claude.json 在其父且父≠$HOME）。
/// ② 大解析进 spawn_blocking（对齐 §10）。
#[tauri::command]
pub async fn read_remote_mcp_servers(origin: String) -> Result<Vec<McpServerEntry>, String> {
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("远端 '{origin}' 未配置或未启用"))?;
    let claude_json = fetch_remote_claude_json(&cfg).await?;
    let src = format!("[{}] ~/.claude.json", cfg.origin_label());
    Ok(collect_entries(claude_json.as_ref(), &src, None, "", None))
}

/// 读远端 `~/.claude.json` 的上限〔devbench F10b 提成具名常量〕。
const REMOTE_CLAUDE_JSON_CAP: u64 = 32 * 1024 * 1024;

/// F87b③ 抽出（F89a 复用）：SSH exec `cat` 远端 `~/.claude.json` → 宽容解析（缺/坏 → None）。**只读**。
/// 定值命令、无用户输入拼接 → 零注入面；多候选（CLAUDE_CONFIG_DIR 优先、否则 $HOME）；30s 超时 + 32MB 上限
/// （⚠ **超限是拒收+回错**，devbench F10b —— 不是「宽容解析」那一档：截断的 JSON 解析失败会
/// 报「解析失败」而不是「超限」，那是误导性的错误）；
/// 大解析进 spawn_blocking（对齐 §10）。
async fn fetch_remote_claude_json(
    cfg: &crate::ssh_source::RemoteConfig,
) -> Result<Option<Value>, String> {
    use tokio::io::AsyncReadExt;
    const CMD: &str = r#"{ [ -n "$CLAUDE_CONFIG_DIR" ] && cat "$CLAUDE_CONFIG_DIR/.claude.json" 2>/dev/null; } || cat "$HOME/.claude.json" 2>/dev/null || true"#;
    let read = async {
        let stream = crate::ssh_source::connect_and_exec_cmd(cfg, CMD).await?;
        let mut buf = Vec::new();
        // `+ 1` 见 src/backend/common/fs.rs：不多读一个字节就分不清
        // 「刚好读满」与「其实还有」。⚠ 截断的 JSON 会在下面解析失败，用户看到的是
        // 「解析失败」而不是「超限」—— 那是**误导性的错误**，不是诚实的降级。
        stream
            .take(REMOTE_CLAUDE_JSON_CAP + 1)
            .read_to_end(&mut buf)
            .await
            .map_err(|e| format!("读取远端 ~/.claude.json 失败: {e}"))?;
        if buf.len() as u64 > REMOTE_CLAUDE_JSON_CAP {
            return Err(format!(
                "远端 ~/.claude.json 超过 {REMOTE_CLAUDE_JSON_CAP} 字节上限 —— 拒收，不拿截断的 JSON 去解析"
            ));
        }
        Ok::<Vec<u8>, String>(buf)
    };
    let raw = tokio::time::timeout(std::time::Duration::from_secs(30), read)
        .await
        .map_err(|_| format!("远端 '{}' 读取超时（30s）", cfg.origin_label()))??;
    tokio::task::spawn_blocking(move || {
        let text = String::from_utf8_lossy(&raw);
        serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')).ok()
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))
}

/// F89a：列远端项目目录（`~/.claude.json` 的 `projects` 键，排序）——前端远端项目选择器 datalist 用。**只读**。
///
/// 🔴 **〔步 12·C〕它不再是一条 Tauri 命令** —— 上线的那一条是
/// [`list_mcp_project_dirs`]，本函数是它的远端那一支。名字**刻意没改**：
/// `local_origin_registry::TRIAGE_DEBT` 按「文件::函数」登记着这一处，
/// 改名会让那张表静默失配（那条判据的 `stale` 断言逐字治这件事）。
pub(crate) async fn list_remote_mcp_project_dirs(host: &str) -> Result<Vec<String>, String> {
    let cfg = crate::load_remote_config_by_label(host)
        .ok_or_else(|| format!("远端 '{host}' 未配置或未启用"))?;
    let claude_json = fetch_remote_claude_json(&cfg).await?;
    Ok(claude_json
        .map(|v| project_dirs_from(&v))
        .unwrap_or_default())
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
        return Err("project_dir 为空，拒绝写".into());
    }
    if !Path::new(d).is_absolute() {
        return Err(format!("项目目录须为绝对路径（实得 {d:?}）"));
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
        return Err("server 名为空".into());
    }
    let obj = root
        .as_object_mut()
        .ok_or_else(|| ".mcp.json 根不是对象".to_string())?;
    let servers = obj
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    let smap = servers
        .as_object_mut()
        .ok_or_else(|| "mcpServers 不是对象".to_string())?;
    smap.insert(name, server);
    Ok(())
}

/// **纯核心**（F89a 抽出，本机/远端复用、可测）：从 `.mcp.json` Value 删一条 server，返回是否真删。
fn remove_mcp_server_value(root: &mut Value, name: &str) -> Result<bool, String> {
    let obj = root
        .as_object_mut()
        .ok_or_else(|| ".mcp.json 根不是对象".to_string())?; // 根对象守卫（对齐 write）
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
        return Err("远端项目目录须为绝对路径".into());
    }
    let p = format!("{d}/.mcp.json");
    if !is_safe_remote_mcp_json(&p) {
        return Err(format!("拒绝写非法远端路径：{p}"));
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
        .ok_or_else(|| format!("拒绝写：{target} 切不出项目目录"))
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
        Some(t) => serde_json::from_str(t.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("{what} 解析失败（拒绝覆盖）: {e}"))?,
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
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("远端 '{origin}' 未配置或未启用"))?;
    let path = remote_mcp_json_path(&project_dir)?;
    let (root_dir, rel) = path
        .rsplit_once('/')
        .filter(|(r, n)| !r.is_empty() && !n.is_empty())
        .ok_or_else(|| format!("{path} 切不出项目目录"))?;
    let door = crate::user_files::BackendDoor::new(crate::origin::Origin(cfg.origin_label()));
    // 读侧宽容：不存在 / 读不出 / 坏 → 空（不像写侧那样 Err）。
    let root = crate::user_files::Door::peek(&door, root_dir, rel)
        .await
        .ok()
        .and_then(|p| p.text)
        .and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok())
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
