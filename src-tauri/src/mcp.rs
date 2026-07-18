//! F87（#50+#51）本机 / **F87b+F89a（#52 跨机）** MCP 管理。**SS-14 读写分界**（doc/../plan SS-14 + INVARIANTS §1 例外 #5）：
//! - **读**：本机跨 scope（用户 `~/.claude.json` 顶层 + local `projects[<dir>]` + 项目 `<dir>/.mcp.json`）+
//!   **远端** user scope（`read_remote_mcp_servers`：SSH-exec cat 远端 `~/.claude.json`）+ **远端项目**
//!   （`read_remote_project_mcp`：SFTP 读远端 `<dir>/.mcp.json`）。**宽容读**（INVARIANTS §18）：缺/坏/字段缺跳过、server 原样。
//! - **写**：**只** `<dir>/.mcp.json`（增/改/删）。**绝不写 `~/.claude.json` / `settings.json`**——本机经 `mcp_json_path`
//!   硬编码 / **远端**经 `remote_mcp_json_path`+`is_safe_remote_mcp_json` 守卫 + `sftp::upload_atomic`（本文件承载
//!   `write_remote_mcp_server`/`remove_remote_mcp_server`）。SS-G：远端写仅用户显式触发。
//!
//! `~/.claude.json` 路径有变体（`CLAUDE_CONFIG_DIR` vs `$HOME`），故 `claude_json_candidates` 取多候选、
//! 读第一个存在的——防御式，schema 真机可能变，不硬假设完整。

use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// 一条 MCP server 展示项。`server` 原样保留（宽容，未知字段不丢）。camelCase 上 wire。
#[derive(serde::Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerEntry {
    /// `"user"` | `"local"` | `"project"`
    pub scope: String,
    pub name: String,
    /// server 配置原样（`{command,args,env}` 或 `{type:"sse"|"http",url,...}`）。
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
#[tauri::command]
pub async fn list_mcp_project_dirs() -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(list_mcp_project_dirs_impl)
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))
}

/// F87b③：跨机读远端 MCP。**只读**（守 §1：SSH exec `cat` 远端**用户自己**的 `~/.claude.json`，
/// 不写、不驱动远端 agent；同 daemonless `find`/`tail` 读法）。**不依赖未建的 daemon**。
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

/// F87b③ 抽出（F89a 复用）：SSH exec `cat` 远端 `~/.claude.json` → 宽容解析（缺/坏 → None）。**只读**。
/// 定值命令、无用户输入拼接 → 零注入面；多候选（CLAUDE_CONFIG_DIR 优先、否则 $HOME）；30s 超时 + 32MB 上限；
/// 大解析进 spawn_blocking（对齐 §10）。
async fn fetch_remote_claude_json(
    cfg: &crate::ssh_source::RemoteConfig,
) -> Result<Option<Value>, String> {
    use tokio::io::AsyncReadExt;
    const CMD: &str = r#"{ [ -n "$CLAUDE_CONFIG_DIR" ] && cat "$CLAUDE_CONFIG_DIR/.claude.json" 2>/dev/null; } || cat "$HOME/.claude.json" 2>/dev/null || true"#;
    let read = async {
        let stream = crate::ssh_source::connect_and_exec_cmd(cfg, CMD).await?;
        let mut buf = Vec::new();
        stream
            .take(32 * 1024 * 1024)
            .read_to_end(&mut buf)
            .await
            .map_err(|e| format!("读取远端 ~/.claude.json 失败: {e}"))?;
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
#[tauri::command]
pub async fn list_remote_mcp_project_dirs(origin: String) -> Result<Vec<String>, String> {
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("远端 '{origin}' 未配置或未启用"))?;
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

/// **只**返回 `<dir>/.mcp.json` 路径——写侧唯一出口，硬编码 `.mcp.json`，杜绝误写
/// `~/.claude.json` / `settings.json`（SS-14 铁律；grep 门禁：本文件写路径只此一处）。
fn mcp_json_path(project_dir: &str) -> Result<PathBuf, String> {
    let d = project_dir.trim();
    if d.is_empty() {
        return Err("project_dir 为空，拒绝写".into());
    }
    Ok(Path::new(d).join(".mcp.json"))
}

/// 读 `.mcp.json`（存在则解析，不存在给骨架 `{"mcpServers":{}}`）。§3 剥 BOM。
fn read_or_skeleton(mcp: &Path) -> Result<Value, String> {
    if mcp.is_file() {
        let raw =
            std::fs::read_to_string(mcp).map_err(|e| format!("read {}: {e}", mcp.display()))?;
        serde_json::from_str(raw.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("parse {}: {e}", mcp.display()))
    } else {
        Ok(serde_json::json!({ "mcpServers": {} }))
    }
}

/// **纯核心**（F89a 抽出，本机/远端复用、可测）：把一条 server upsert 进 `.mcp.json` Value。名空拒。
fn upsert_mcp_server_value(root: &mut Value, name: String, server: Value) -> Result<(), String> {
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

/// §4 安全写**用户文件** `.mcp.json`：① 项目目录须**已存在**（真实项目根，不 `create_dir_all` typo
/// 路径生成垃圾目录树）；② 写前 backup（dst 存在则备到 `.bak`）；③ **ReplaceFileW** 原子替换——
/// 复用 `profile_installer::atomic_write_string`（保留 dst ACL），**不**用 config 的 `MoveFileExW`
/// （§4 明令 MoveFileExW 会把 tmp 的 ACL 写到 dst，写用户文件是错的）；④ 写后**回读校验**（确认落盘 + 可解析）。
fn write_json_atomic(path: &Path, value: &Value) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "bad path".to_string())?;
    if !parent.is_dir() {
        return Err(format!(
            "项目目录不存在：{}（请填已存在的项目根）",
            parent.display()
        ));
    }
    let pretty = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    if path.is_file() {
        let bak = path.with_extension("json.bak");
        let _ = std::fs::copy(path, &bak); // best-effort backup
    }
    crate::profile_installer::atomic_write_string(&path.to_path_buf(), &pretty)
        .map_err(|e| format!("write {}: {e}", path.display()))?;
    let back =
        std::fs::read_to_string(path).map_err(|e| format!("readback {}: {e}", path.display()))?;
    serde_json::from_str::<Value>(back.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("readback parse {}: {e}", path.display()))?;
    Ok(())
}

fn write_project_mcp_server_impl(
    project_dir: String,
    name: String,
    server: Value,
) -> Result<(), String> {
    let mcp = mcp_json_path(&project_dir)?;
    let mut root = read_or_skeleton(&mcp)?;
    upsert_mcp_server_value(&mut root, name, server)?;
    write_json_atomic(&mcp, &root)
}

/// F87 写命令：增 / 改项目 `.mcp.json` 里一条 MCP server。**只碰 `<dir>/.mcp.json`。**§10 spawn_blocking。
#[tauri::command]
pub async fn write_project_mcp_server(
    project_dir: String,
    name: String,
    server: Value,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || write_project_mcp_server_impl(project_dir, name, server))
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
}

fn remove_project_mcp_server_impl(project_dir: String, name: String) -> Result<(), String> {
    let mcp = mcp_json_path(&project_dir)?;
    if !mcp.is_file() {
        return Ok(()); // 无文件即无条目
    }
    let mut root = read_or_skeleton(&mcp)?;
    if !remove_mcp_server_value(&mut root, &name)? {
        return Ok(()); // no-op（条目不存在）→ 不重写、不 churn
    }
    write_json_atomic(&mcp, &root)
}

/// F87 写命令：删项目 `.mcp.json` 里一条 MCP server。**只碰 `<dir>/.mcp.json`。**§10 spawn_blocking。
#[tauri::command]
pub async fn remove_project_mcp_server(project_dir: String, name: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || remove_project_mcp_server_impl(project_dir, name))
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
}

// ───────────────────────── F89a：远端项目级 MCP 读写（SFTP） ─────────────────────────
// 只读铁律边界：读=只读；**写/删仅用户显式触发（SS-G 豁免）**、写面**只** `<dir>/.mcp.json`
// （`remote_mcp_json_path` 硬编码 + `is_safe_remote_mcp_json` 守卫，SS-14 远端对端）。SFTP `upload_atomic`
// 原子 tmp+rename；**已存在但读/解析失败 → 报错拒绝覆盖**（不 skeleton 盖掉现有 server，对齐本机 read_or_skeleton）。

/// SFTP 读远端 `.mcp.json` Value：不存在→骨架；存在但读/解析失败→Err（拒绝后续覆盖丢数据）。
/// batch20 审计修（两处）：① `try_exists` **Err → Err 拒写**（原 `unwrap_or(false)` 把瞬态 stat 失败当「不存在」→
/// 会 skeleton 覆盖丢其余 server）；② **崩溃恢复**——path 缺但 `<path>.bak` 在 = 上次写在末段 rename 中断（`upload_atomic`
/// 备份了旧文件、tmp→path 失败），此时读骨架会让重试丢掉备份里的其余 server → 改为**从 `.bak` 恢复**旧内容。
async fn read_remote_mcp_value(
    sftp: &russh_sftp::client::SftpSession,
    path: &str,
) -> Result<Value, String> {
    let exists = sftp
        .try_exists(path.to_string())
        .await
        .map_err(|e| format!("检查远端 {path} 存在性失败（拒绝写以免丢数据）: {e}"))?;
    if !exists {
        // 崩溃恢复：path 缺但 .bak 在 → 从 .bak 读回上次写中断前的内容（否则重试骨架覆盖丢 server）。
        let bak = format!("{path}.bak");
        if sftp.try_exists(bak.clone()).await.unwrap_or(false) {
            if let Ok(bytes) = sftp.read(bak.clone()).await {
                if let Ok(v) = serde_json::from_str::<Value>(
                    String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}'),
                ) {
                    return Ok(v); // 从备份恢复
                }
            }
        }
        return Ok(serde_json::json!({ "mcpServers": {} }));
    }
    let bytes = sftp
        .read(path.to_string())
        .await
        .map_err(|e| format!("读远端 {path} 失败: {e}"))?;
    let text = String::from_utf8_lossy(&bytes);
    serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("远端 {path} 解析失败（拒绝覆盖）: {e}"))
}

/// F89a：读远端某项目的 `.mcp.json`（project scope 条目）。**只读**。缺/坏 → 空段（宽容，读侧不阻断）。
#[tauri::command]
pub async fn read_remote_project_mcp(
    origin: String,
    project_dir: String,
) -> Result<Vec<McpServerEntry>, String> {
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("远端 '{origin}' 未配置或未启用"))?;
    let path = remote_mcp_json_path(&project_dir)?;
    let conn = crate::sftp::connect_sftp(&cfg).await?;
    // 读侧宽容：不存在/坏 → 空（不像写侧那样 Err）。
    let root = read_remote_mcp_value(&conn.sftp, &path)
        .await
        .unwrap_or_else(|_| serde_json::json!({ "mcpServers": {} }));
    let src = format!("[{}] {path}", cfg.origin_label());
    Ok(collect_entries(None, "", Some(&root), &src, None))
}

/// F89a：增/改远端项目 `.mcp.json` 一条 server。**SS-G 用户显式触发 + SS-14 只碰 .mcp.json。**SFTP 原子 RMW。
#[tauri::command]
pub async fn write_remote_mcp_server(
    origin: String,
    project_dir: String,
    name: String,
    server: Value,
) -> Result<(), String> {
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("远端 '{origin}' 未配置或未启用"))?;
    let path = remote_mcp_json_path(&project_dir)?;
    let conn = crate::sftp::connect_sftp(&cfg).await?;
    let mut root = read_remote_mcp_value(&conn.sftp, &path).await?; // 已存在坏文件 → Err 拒覆盖
    upsert_mcp_server_value(&mut root, name, server)?;
    let pretty = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    crate::sftp::upload_atomic(&conn.sftp, &path, pretty.as_bytes(), 0o600).await
}

/// F89a：删远端项目 `.mcp.json` 一条 server。**SS-G 用户显式触发 + SS-14 只碰 .mcp.json。**SFTP 原子 RMW。
#[tauri::command]
pub async fn remove_remote_mcp_server(
    origin: String,
    project_dir: String,
    name: String,
) -> Result<(), String> {
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("远端 '{origin}' 未配置或未启用"))?;
    let path = remote_mcp_json_path(&project_dir)?;
    let conn = crate::sftp::connect_sftp(&cfg).await?;
    if !conn.sftp.try_exists(path.clone()).await.unwrap_or(false) {
        return Ok(()); // 无文件即无条目
    }
    let mut root = read_remote_mcp_value(&conn.sftp, &path).await?;
    if !remove_mcp_server_value(&mut root, &name)? {
        return Ok(()); // no-op
    }
    let pretty = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    crate::sftp::upload_atomic(&conn.sftp, &path, pretty.as_bytes(), 0o600).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn collect_merges_three_scopes() {
        let claude = json!({
            "mcpServers": { "user-srv": { "command": "u" } },
            "projects": { "/proj": { "mcpServers": { "local-srv": { "command": "l" } } } }
        });
        let proj = json!({ "mcpServers": { "proj-srv": { "type": "http", "url": "x" } } });
        let out = collect_entries(Some(&claude), "cj", Some(&proj), "mcp", Some("/proj"));
        assert_eq!(out.len(), 3);
        let scopes: Vec<&str> = out.iter().map(|e| e.scope.as_str()).collect();
        assert!(
            scopes.contains(&"user") && scopes.contains(&"local") && scopes.contains(&"project")
        );
        // server 原样保留
        let proj_e = out.iter().find(|e| e.scope == "project").unwrap();
        assert_eq!(proj_e.server["url"], json!("x"));
    }

    #[test]
    fn remote_read_takes_user_scope_only() {
        // F87b③：跨机读的契约——`collect_entries(cj, src, None, "", None)` 只出 user scope
        // （顶层 mcpServers = 机器全局 MCP）。有 projects（local scope 候选）也不取（project_dir=None）。
        let remote_claude = json!({
            "mcpServers": { "global-a": { "command": "a" }, "global-b": { "type": "http", "url": "u" } },
            "projects": { "/remote/proj": { "mcpServers": { "local-x": { "command": "x" } } } }
        });
        let out = collect_entries(Some(&remote_claude), "[pi] ~/.claude.json", None, "", None);
        assert_eq!(out.len(), 2, "只取 user scope 两条，不含 local/project");
        assert!(out.iter().all(|e| e.scope == "user"));
        let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"global-a") && names.contains(&"global-b"));
        assert!(!names.contains(&"local-x"), "local scope 不该跨机取");
        // 来源路径标了 origin（前端据此显只读远端来源）
        assert!(out.iter().all(|e| e.source_path == "[pi] ~/.claude.json"));
    }

    #[test]
    fn collect_tolerant_missing_and_bad_shapes() {
        // 全 None → 空
        assert!(collect_entries(None, "", None, "", None).is_empty());
        // mcpServers 非对象 → 跳过不崩
        let claude = json!({ "mcpServers": "not-an-object" });
        assert!(collect_entries(Some(&claude), "cj", None, "", Some("/p")).is_empty());
        // 有 user 无 local（projects 缺该 dir）
        let claude2 = json!({ "mcpServers": { "u": {} }, "projects": {} });
        let out = collect_entries(Some(&claude2), "cj", None, "", Some("/p"));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].scope, "user");
    }

    #[test]
    fn write_and_remove_only_touch_mcp_json() {
        let tmp = std::env::temp_dir().join(format!("ccm-mcp-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let dir = tmp.to_string_lossy().into_owned();
        // 放一个假 .claude.json 在临时目录，断言写后它不变
        let fake_claude = tmp.join(".claude.json");
        std::fs::write(&fake_claude, "{\"mcpServers\":{\"keep\":{}}}").unwrap();

        // 写：建骨架 + 加条目（测同步 _impl；命令是 async 薄封装）
        write_project_mcp_server_impl(dir.clone(), "srv".into(), json!({ "command": "c" }))
            .unwrap();
        let mcp = tmp.join(".mcp.json");
        assert!(mcp.is_file());
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&mcp).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["srv"]["command"], json!("c"));
        // .claude.json 一字未动
        assert_eq!(
            std::fs::read_to_string(&fake_claude).unwrap(),
            "{\"mcpServers\":{\"keep\":{}}}"
        );

        // 删
        remove_project_mcp_server_impl(dir.clone(), "srv".into()).unwrap();
        let v2: Value = serde_json::from_str(&std::fs::read_to_string(&mcp).unwrap()).unwrap();
        assert!(v2["mcpServers"].get("srv").is_none());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn write_rejects_empty_project_dir_and_nonexistent() {
        assert!(write_project_mcp_server_impl("  ".into(), "n".into(), json!({})).is_err());
        assert!(mcp_json_path("").is_err());
        // 项目目录不存在 → 拒写（不 create_dir_all typo 路径）
        let ghost = std::env::temp_dir().join(format!("ccm-mcp-ghost-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ghost);
        let r = write_project_mcp_server_impl(
            ghost.to_string_lossy().into_owned(),
            "n".into(),
            json!({ "command": "c" }),
        );
        assert!(r.is_err(), "不存在的项目目录应拒写");
        assert!(!ghost.exists(), "拒写后不该建出 typo 目录");
    }

    #[test]
    fn project_dirs_from_sorted_and_tolerant() {
        let cj = json!({ "projects": { "/b": {}, "/a": {} } });
        assert_eq!(
            project_dirs_from(&cj),
            vec!["/a".to_string(), "/b".to_string()]
        );
        assert!(project_dirs_from(&json!({})).is_empty()); // 缺 projects
        assert!(project_dirs_from(&json!({ "projects": "bad" })).is_empty()); // 非对象
    }

    #[test]
    fn remote_mcp_path_guard_rejects_traversal_and_nonabsolute() {
        // F89a：远端写路径守卫——只接受 绝对 + 尾 /.mcp.json + 无 .. 的非裸路径。
        assert_eq!(
            remote_mcp_json_path("/home/pi/proj").unwrap(),
            "/home/pi/proj/.mcp.json"
        );
        assert_eq!(
            remote_mcp_json_path("/home/pi/proj/").unwrap(), // 尾斜杠归一
            "/home/pi/proj/.mcp.json"
        );
        assert!(remote_mcp_json_path("relative/proj").is_err()); // 非绝对
        assert!(remote_mcp_json_path("").is_err()); // 空
        assert!(remote_mcp_json_path("/a/../../etc").is_err()); // 路径穿越 → 拼后含 ..
        assert!(remote_mcp_json_path("/").is_err()); // 根 → 拼成 /.mcp.json 被守卫拒（裸）
                                                     // 守卫本体：只 /.mcp.json 结尾、无 .. 、绝对、非裸
        assert!(is_safe_remote_mcp_json("/x/y/.mcp.json"));
        assert!(!is_safe_remote_mcp_json("/.mcp.json")); // 裸（无项目段）
        assert!(!is_safe_remote_mcp_json("/x/.claude.json")); // 非 .mcp.json（SS-14 铁）
        assert!(!is_safe_remote_mcp_json("/x/settings.json")); // 非 .mcp.json
        assert!(!is_safe_remote_mcp_json("/x/../y/.mcp.json")); // 穿越
        assert!(!is_safe_remote_mcp_json("x/.mcp.json")); // 非绝对
    }

    #[test]
    fn upsert_and_remove_value_cores() {
        // F89a：本机/远端复用的纯核心——upsert/remove Value 变换。
        let mut root = json!({ "mcpServers": { "a": { "command": "x" } } });
        upsert_mcp_server_value(&mut root, "b".into(), json!({ "type": "http", "url": "u" }))
            .unwrap();
        assert_eq!(root["mcpServers"]["a"]["command"], json!("x")); // 原有不动
        assert_eq!(root["mcpServers"]["b"]["url"], json!("u")); // 新增
        upsert_mcp_server_value(&mut root, "a".into(), json!({ "command": "y" })).unwrap();
        assert_eq!(root["mcpServers"]["a"]["command"], json!("y")); // 同名覆盖
        assert!(upsert_mcp_server_value(&mut root, "  ".into(), json!({})).is_err()); // 名空拒
                                                                                      // 骨架缺 mcpServers → upsert 自建
        let mut skel = json!({});
        upsert_mcp_server_value(&mut skel, "c".into(), json!({})).unwrap();
        assert!(skel["mcpServers"]["c"].is_object());
        // remove
        assert!(remove_mcp_server_value(&mut root, "a").unwrap()); // 真删
        assert!(root["mcpServers"].get("a").is_none());
        assert!(!remove_mcp_server_value(&mut root, "nope").unwrap()); // 不存在 → false
    }
}
