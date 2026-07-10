//! Batch14-F47：SFTP 文件面板后端——per-host 常驻连接池 + 文件操作命令。
//!
//! ## 与既有 SFTP 写的关系（INVARIANT §1）
//! §1「monitor 零侵入 Claude 数据源」管的是 **monitor 作为监视器**不改坏 Claude 的
//! jsonl/pidfile。本模块是**用户亲自驱动**的通用文件传输面板,与数据源只读契约**正交**
//! （2026-07-10 用户拍板:SFTP 属独立文件传输功能,不算 monitor 写)。防误伤守卫见
//! [`is_protected_claude_data_path`]:SFTP 写命令拒碰 Claude 数据源文件(往正被 Claude
//! 打开的 jsonl 写会损坏会话)——这是防手滑,不是合规。
//!
//! ## 连接分离
//! SFTP 面板连接走**独立 utility 池**,与 daemon 数据源流连接(`ssh_source` 长连接)
//! 分离,不共用——面板操作永不影响会话流。池按 origin 键、取用时校活性、死则重建。

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::Mutex;

use crate::sftp::{connect_sftp, SftpConn};
use crate::ssh_source::RemoteConfig;

/// 目录项（前端渲染 + 排序用）。
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SftpEntry {
    pub name: String,
    /// 绝对路径（父目录 + name，SFTP 恒用 `/`）。
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    /// 非 UTF-8 文件名有损显示（russh-sftp 按 UTF-8 解）→ 标记,前端拒对其写操作。
    pub lossy_name: bool,
}

/// 单文件/目录 stat。
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SftpStat {
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

/// F47 防误伤守卫:该远端路径是否 Claude 数据源文件(jsonl / pidfile)。
/// SFTP 写命令拒碰这些——往正被 Claude 打开的会话文件写会损坏会话;要管这些用历史浏览器
/// （F11 删除带确认),不走文件面板。纯 substring 判定(与 sftp::is_safe_remote_jsonl 同风格)。
pub fn is_protected_claude_data_path(path: &str) -> bool {
    let p = path.replace('\\', "/");
    (p.contains("/.claude/projects/") && p.ends_with(".jsonl"))
        || (p.contains("/.claude/sessions/") && p.ends_with(".json"))
}

/// 判定文件名是否含非 UTF-8 有损替换字符（russh-sftp 已把无效字节转成 U+FFFD）。
fn is_lossy_name(name: &str) -> bool {
    name.contains('\u{FFFD}')
}

// === 连接池 ===

type Slot = Arc<Mutex<Option<SftpConn>>>;

fn pool() -> &'static Mutex<HashMap<String, Slot>> {
    static POOL: std::sync::OnceLock<Mutex<HashMap<String, Slot>>> = std::sync::OnceLock::new();
    POOL.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn slot_for(origin: &str) -> Slot {
    let mut m = pool().lock().await;
    m.entry(origin.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(None)))
        .clone()
}

/// 连接死亡特征（op 失败时据此决定是否重建连接并重试一次）。
fn looks_like_dead_conn(err: &str) -> bool {
    let e = err.to_ascii_lowercase();
    [
        "eof",
        "closed",
        "broken",
        "reset",
        "disconnect",
        "not connected",
        "pipe",
    ]
    .iter()
    .any(|k| e.contains(k))
}

/// SFTP 操作闭包返回的 future 类型别名（借 `&SftpSession`,故带 HRTB 生命周期）。
type SftpFut<'a, T> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, String>> + Send + 'a>>;

/// 在池化 SFTP 连接上跑一次操作。空槽→connect_sftp 建;op 失败且像连接死亡→重建重试一次
/// （文件不存在等业务错误不触发重连,原样返回）。per-origin 锁串行化该 host 的操作。
/// `op` 用 `Box::pin(async move {…})` 包裹(future 借用 `&SftpSession`,需 HRTB)。
pub async fn with_sftp<T, F>(cfg: &RemoteConfig, op: F) -> Result<T, String>
where
    F: for<'a> Fn(&'a russh_sftp::client::SftpSession) -> SftpFut<'a, T>,
{
    let slot = slot_for(&cfg.origin_label()).await;
    let mut guard = slot.lock().await;
    if guard.is_none() {
        *guard = Some(connect_sftp(cfg).await?);
    }
    let first = op(&guard.as_ref().unwrap().sftp).await;
    match first {
        Ok(v) => Ok(v),
        Err(e) if looks_like_dead_conn(&e) => {
            // 连接疑似已死 → 重建一次再试（网络抖动/远端 sshd 回收空闲 SFTP）。
            *guard = Some(connect_sftp(cfg).await?);
            op(&guard.as_ref().unwrap().sftp).await
        }
        Err(e) => Err(e),
    }
}

/// 丢弃某 origin 的池连接（设置卡「断开」/配置改动时调；下次操作重建）。
pub async fn drop_pooled(origin: &str) {
    if let Some(slot) = pool().lock().await.get(origin) {
        *slot.lock().await = None;
    }
}

// === 命令：浏览（只读）===

/// realpath('.')——浏览起点（远端 home 绝对路径）。
#[tauri::command]
pub async fn sftp_realpath(cfg: RemoteConfig, path: String) -> Result<String, String> {
    with_sftp(&cfg, move |s| {
        let path = path.clone();
        Box::pin(async move {
            s.canonicalize(path)
                .await
                .map_err(|e| format!("realpath 失败: {e}"))
        })
    })
    .await
}

/// 列目录:目录在前 + 名称小写排序(aterm 契约)。
#[tauri::command]
pub async fn sftp_list_dir(cfg: RemoteConfig, path: String) -> Result<Vec<SftpEntry>, String> {
    let dir = path.trim_end_matches('/').to_string();
    let mut out = with_sftp(&cfg, move |s| {
        let path = path.clone();
        let dir = dir.clone();
        Box::pin(async move {
            let rd = s
                .read_dir(path)
                .await
                .map_err(|e| format!("读目录失败: {e}"))?;
            let mut v: Vec<SftpEntry> = Vec::new();
            for entry in rd {
                let name = entry.file_name();
                if name == "." || name == ".." {
                    continue;
                }
                let meta = entry.metadata();
                let ft = entry.file_type();
                v.push(SftpEntry {
                    path: format!("{dir}/{name}"),
                    is_dir: meta.is_dir(),
                    is_symlink: ft.is_symlink(),
                    size: meta.len(),
                    lossy_name: is_lossy_name(&name),
                    name,
                });
            }
            Ok(v)
        })
    })
    .await?;
    // 目录在前,再按名称小写排序（稳定、可读）。
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

/// stat 单个路径。
#[tauri::command]
pub async fn sftp_stat(cfg: RemoteConfig, path: String) -> Result<SftpStat, String> {
    with_sftp(&cfg, move |s| {
        let path = path.clone();
        Box::pin(async move {
            let m = s
                .metadata(path.clone())
                .await
                .map_err(|e| format!("stat 失败: {e}"))?;
            Ok(SftpStat {
                path,
                is_dir: m.is_dir(),
                size: m.len(),
            })
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_path_guard() {
        assert!(is_protected_claude_data_path(
            "/home/pi/.claude/projects/-x/abc.jsonl"
        ));
        assert!(is_protected_claude_data_path(
            "/home/u/.claude/sessions/123.json"
        ));
        // 普通用户文件不受守卫
        assert!(!is_protected_claude_data_path("/home/pi/proj/main.rs"));
        assert!(!is_protected_claude_data_path(
            "/home/pi/.claude/settings.json"
        )); // 非 sessions/ 下
        assert!(!is_protected_claude_data_path(
            "/home/pi/notclaude/projects/x.jsonl"
        )); // 非 /.claude/projects/
            // 反斜杠归一
        assert!(is_protected_claude_data_path(
            "C:\\Users\\me\\.claude\\projects\\p\\s.jsonl"
        ));
    }

    #[test]
    fn lossy_name_detection() {
        assert!(is_lossy_name("bad\u{FFFD}name"));
        assert!(!is_lossy_name("good_name.txt"));
    }

    #[test]
    fn dead_conn_classification() {
        assert!(looks_like_dead_conn("channel closed by peer"));
        assert!(looks_like_dead_conn("Broken pipe (os error 32)"));
        assert!(looks_like_dead_conn("connection reset"));
        assert!(!looks_like_dead_conn("No such file or directory"));
        assert!(!looks_like_dead_conn("permission denied"));
    }

    #[test]
    fn list_dir_sort_dirs_first_then_lowercase() {
        // 直接测排序契约（不需真连接）。
        let mut v = vec![
            SftpEntry {
                name: "Zebra".into(),
                path: "/Zebra".into(),
                is_dir: false,
                is_symlink: false,
                size: 0,
                lossy_name: false,
            },
            SftpEntry {
                name: "apple".into(),
                path: "/apple".into(),
                is_dir: false,
                is_symlink: false,
                size: 0,
                lossy_name: false,
            },
            SftpEntry {
                name: "src".into(),
                path: "/src".into(),
                is_dir: true,
                is_symlink: false,
                size: 0,
                lossy_name: false,
            },
        ];
        v.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        assert_eq!(
            v.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            vec!["src", "apple", "Zebra"]
        );
    }
}
