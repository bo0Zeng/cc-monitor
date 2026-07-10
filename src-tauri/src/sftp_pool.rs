//! Batch14-F47：SFTP 文件面板后端——per-host 常驻连接池 + 文件操作命令。
//!
//! ## 与既有 SFTP 写的关系（INVARIANT §1）
//! §1「monitor 零侵入 Claude 数据源」管的是 **monitor 作为监视器**不改坏 Claude 的
//! jsonl/pidfile。本模块是**用户亲自驱动**的通用文件传输面板,与数据源只读契约**正交**
//! （2026-07-10 用户拍板:SFTP 属独立文件传输功能,不算 monitor 写)。防误伤守卫见
//! [`is_protected_claude_data_path`]:SFTP 写命令拒碰 Claude 数据源文件(往正被 Claude
//! 打开的 jsonl 写会损坏会话)——这是防手滑,不是合规。
//!
//! ## 连接分离 + 已知取舍
//! SFTP 面板连接走**独立 utility 池**,与 daemon 数据源流连接(`ssh_source` 长连接)
//! 分离,不共用——面板操作永不影响会话流。池按 origin 键、取用时校活性、死则重建。
//! - **per-origin 锁串行化**:每 host 单连接,传输整程持 slot 锁 → 同 host 不能边传边浏览、
//!   不能并发两个传输(刻意 v1 取舍;F48 若要并发浏览+传输需给池加多连接,属 F47 范围外)。
//! - **非 UTF-8 文件名**:后端**不拦**对 lossy 名(含 U+FFFD)的写(russh-sftp 已有损解码,
//!   无法寻址真字节)——靠 F48 UI 灰置这些项;`lossy_name` 字段供前端判定。
//! - **空闲回收**:池连接空闲不主动回收(一台机一条 SFTP,YAGNI);死连按需重建,
//!   配置改动经 `drop_pooled` 手动丢弃。

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
/// **`origin` 须传 `cfg.origin_label()`**（池按此键建槽,传 host 会静默 no-op）。
/// D 审计 R3:先克隆出 Arc 释放 `pool()` 全局锁,再锁 slot——否则若该 slot 正被长传输
/// 持有,会握着全局锁死等,卡住所有 origin 的新操作(每个命令都要 pool().lock())。
pub async fn drop_pooled(origin: &str) {
    let slot = pool().lock().await.get(origin).cloned();
    if let Some(slot) = slot {
        *slot.lock().await = None;
    }
}

/// D 审计 R1:传输失败若像连接死亡,把该 origin 的池槽置 None,下次传输干净重连
/// （**不**重启当前这次——部分传输不静默从头来）。修「死连留在槽里→后续传输持续失败」。
async fn evict_if_dead(origin: &str, err: &str) {
    if looks_like_dead_conn(err) {
        drop_pooled(origin).await;
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
    sort_entries(&mut out);
    Ok(out)
}

/// 目录在前,再按名称小写排序（aterm 契约;抽出供单测共用生产比较器）。
fn sort_entries(v: &mut [SftpEntry]) {
    v.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
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

// === 传输(download/upload):chunked + 进度 + 取消 ===

use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// SFTP 写分块 ≤32KB（SFTP draft 建议;严格 server 拒超大包）。
const CHUNK: usize = 32 * 1024;
/// 进度上报节流:每 ≥256KB 报一次（避免刷爆 Channel),外加起止各一次。
const PROGRESS_EVERY: u64 = 256 * 1024;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    pub transferred: u64,
    /// 总字节;下载时=远端 size,上传时=本地 size。未知(极少)为 0。
    pub total: u64,
}

/// 取消令牌注册表:transfer_id → flag。**transfer_id 必须全局唯一(前端用 uuid)**——
/// 并发复用同 id 会互相覆盖 flag(D 审计 R2)。用 std::sync::Mutex(map ops 无 await,
/// 可在 [`CancelGuard::drop`] 里同步清理,修 future 被 abort 时的泄漏 = D 审计 S4)。
fn cancels() -> &'static std::sync::Mutex<HashMap<String, Arc<AtomicBool>>> {
    static C: std::sync::OnceLock<std::sync::Mutex<HashMap<String, Arc<AtomicBool>>>> =
        std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// 注册取消令牌,返回 (flag, guard)。guard 一 drop(命令 future 正常完成 **或被 abort**)
/// 就按 `Arc::ptr_eq` 从表里摘除本次的 flag——ptr_eq 保证不误删并发同 id 的他人 flag。
struct CancelGuard {
    id: String,
    flag: Arc<AtomicBool>,
}
impl Drop for CancelGuard {
    fn drop(&mut self) {
        if let Ok(mut m) = cancels().lock() {
            if m.get(&self.id).is_some_and(|f| Arc::ptr_eq(f, &self.flag)) {
                m.remove(&self.id);
            }
        }
    }
}

fn register_cancel(id: &str) -> (Arc<AtomicBool>, CancelGuard) {
    let flag = Arc::new(AtomicBool::new(false));
    if let Ok(mut m) = cancels().lock() {
        m.insert(id.to_string(), flag.clone());
    }
    (
        flag.clone(),
        CancelGuard {
            id: id.to_string(),
            flag,
        },
    )
}

/// 翻转某传输的取消标志（前端「取消」按钮调）。id 未注册(尚未开始/已结束)→ no-op。
#[tauri::command]
pub async fn sftp_cancel_transfer(transfer_id: String) {
    if let Ok(m) = cancels().lock() {
        if let Some(f) = m.get(&transfer_id) {
            f.store(true, Ordering::SeqCst);
        }
    }
}

fn report(ch: &tauri::ipc::Channel<TransferProgress>, transferred: u64, total: u64) {
    let _ = ch.send(TransferProgress { transferred, total });
}

/// 下载远端文件到本地。chunked 读、进度上报、可取消。**不走 with_sftp 重试**——部分传输
/// 不该静默从头重启;失败/取消返回 Err。写本地 `<local>.part` 再 rename(半成品不留原名)。
/// source 是远端(读,不涉写守卫);target 是本地磁盘。
#[tauri::command]
pub async fn sftp_download(
    cfg: RemoteConfig,
    remote_path: String,
    local_path: String,
    transfer_id: String,
    on_progress: tauri::ipc::Channel<TransferProgress>,
) -> Result<(), String> {
    let (cancel, _guard) = register_cancel(&transfer_id); // _guard 摘除注册项(含 abort)
    let r = download_inner(&cfg, &remote_path, &local_path, &cancel, &on_progress).await;
    if let Err(e) = &r {
        evict_if_dead(&cfg.origin_label(), e).await; // R1:死连不留在槽里毒化后续
    }
    r
}

async fn download_inner(
    cfg: &RemoteConfig,
    remote_path: &str,
    local_path: &str,
    cancel: &Arc<AtomicBool>,
    on_progress: &tauri::ipc::Channel<TransferProgress>,
) -> Result<(), String> {
    let slot = slot_for(&cfg.origin_label()).await;
    let mut guard = slot.lock().await;
    if guard.is_none() {
        *guard = Some(connect_sftp(cfg).await?);
    }
    let sftp = &guard.as_ref().unwrap().sftp;

    let total = sftp
        .metadata(remote_path.to_string())
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    let mut rf = sftp
        .open_with_flags(
            remote_path.to_string(),
            russh_sftp::protocol::OpenFlags::READ,
        )
        .await
        .map_err(|e| format!("打开远端 {remote_path} 失败: {e}"))?;

    let tmp = format!("{local_path}.part");
    let mut lf = tokio::fs::File::create(&tmp)
        .await
        .map_err(|e| format!("创建本地 {tmp} 失败: {e}"))?;

    // 传输核心包进一层:任一步失败(含取消)统一清理 `.part`(D 审计 S1,与取消路径对齐)。
    let core = async {
        let mut buf = vec![0u8; CHUNK];
        let mut done: u64 = 0;
        let mut last_report: u64 = 0;
        report(on_progress, 0, total);
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err("已取消".to_string());
            }
            let n = rf
                .read(&mut buf)
                .await
                .map_err(|e| format!("读远端失败: {e}"))?;
            if n == 0 {
                break;
            }
            lf.write_all(&buf[..n])
                .await
                .map_err(|e| format!("写本地失败: {e}"))?;
            done += n as u64;
            if done - last_report >= PROGRESS_EVERY {
                last_report = done;
                report(on_progress, done, total);
            }
        }
        lf.flush()
            .await
            .map_err(|e| format!("flush 本地失败: {e}"))?;
        Ok(done)
    }
    .await;
    drop(lf);
    let done = match core {
        Ok(d) => d,
        Err(e) => {
            let _ = tokio::fs::remove_file(&tmp).await; // 清半成品 .part
            return Err(e);
        }
    };
    tokio::fs::rename(&tmp, local_path).await.map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("落地 {local_path} 失败: {e}")
    })?;
    report(on_progress, done, total);
    Ok(())
}

/// 上传本地文件到远端。chunked 写、进度、可取消。**写守卫**:拒 Claude 数据源路径。
/// 写远端 `<remote>.tmp` → 删旧 → rename(近似原子,复用 upload_atomic 的 flush/shutdown 纪律)。
/// 覆盖不静默:前端先 stat 确认存在并二次确认,后端直接原子替换。
#[tauri::command]
pub async fn sftp_upload(
    cfg: RemoteConfig,
    local_path: String,
    remote_path: String,
    transfer_id: String,
    on_progress: tauri::ipc::Channel<TransferProgress>,
) -> Result<(), String> {
    guard_write(&remote_path)?;
    let (cancel, _guard) = register_cancel(&transfer_id); // _guard 摘除注册项(含 abort)
    let r = upload_inner(&cfg, &local_path, &remote_path, &cancel, &on_progress).await;
    if let Err(e) = &r {
        evict_if_dead(&cfg.origin_label(), e).await; // R1:死连不留在槽里毒化后续
    }
    r
}

async fn upload_inner(
    cfg: &RemoteConfig,
    local_path: &str,
    remote_path: &str,
    cancel: &Arc<AtomicBool>,
    on_progress: &tauri::ipc::Channel<TransferProgress>,
) -> Result<(), String> {
    let total = tokio::fs::metadata(local_path)
        .await
        .map(|m| m.len())
        .map_err(|e| format!("读本地 {local_path} 失败: {e}"))?;
    let mut lf = tokio::fs::File::open(local_path)
        .await
        .map_err(|e| format!("打开本地 {local_path} 失败: {e}"))?;

    let slot = slot_for(&cfg.origin_label()).await;
    let mut guard = slot.lock().await;
    if guard.is_none() {
        *guard = Some(connect_sftp(cfg).await?);
    }
    let sftp = &guard.as_ref().unwrap().sftp;

    let tmp = format!("{remote_path}.tmp");
    let mut rf = sftp
        .open_with_flags(
            tmp.clone(),
            russh_sftp::protocol::OpenFlags::CREATE
                | russh_sftp::protocol::OpenFlags::TRUNCATE
                | russh_sftp::protocol::OpenFlags::WRITE,
        )
        .await
        .map_err(|e| format!("创建远端 {tmp} 失败: {e}"))?;

    // 传输核心包一层:任一步失败(含取消)统一 shutdown+删远端 `.tmp`(S1,与取消路径对齐)。
    let core = async {
        let mut buf = vec![0u8; CHUNK];
        let mut done: u64 = 0;
        let mut last_report: u64 = 0;
        report(on_progress, 0, total);
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err("已取消".to_string());
            }
            let n = lf
                .read(&mut buf)
                .await
                .map_err(|e| format!("读本地失败: {e}"))?;
            if n == 0 {
                break;
            }
            rf.write_all(&buf[..n])
                .await
                .map_err(|e| format!("写远端失败: {e}"))?;
            done += n as u64;
            if done - last_report >= PROGRESS_EVERY {
                last_report = done;
                report(on_progress, done, total);
            }
        }
        // 见 upload_atomic:flush 始终 drain 写队列 + 传播错误,shutdown 关闭。
        rf.flush()
            .await
            .map_err(|e| format!("flush 远端失败（写未确认）: {e}"))?;
        Ok(done)
    }
    .await;
    let _ = rf.shutdown().await;
    drop(rf);
    let done = match core {
        Ok(d) => d,
        Err(e) => {
            let _ = sftp.remove_file(tmp.clone()).await; // 清远端半成品 .tmp
            return Err(e);
        }
    };

    // 原文件在此之前完好无损（失败绝不销毁远端原文件）;此后才删旧+rename(近似原子)。
    if sftp
        .try_exists(remote_path.to_string())
        .await
        .unwrap_or(false)
    {
        sftp.remove_file(remote_path.to_string())
            .await
            .map_err(|e| format!("删旧 {remote_path} 失败: {e}"))?;
    }
    sftp.rename(tmp.clone(), remote_path.to_string())
        .await
        .map_err(|e| format!("rename 到 {remote_path} 失败: {e}"))?;
    report(on_progress, done, total);
    Ok(())
}

// === 小文件编辑(F49):read_text_for_edit / write_text ===

/// F49 编辑上限。aterm 契约:超上限**拒编而非截断**(截断标记当编辑源会写坏文件)。
const MAX_EDIT_BYTES: usize = 256 * 1024;

/// 字节 → 可编辑文本;不可编辑(>256KB / 含 NUL 疑二进制 / 非 UTF-8)→ None。
/// 纯函数,护栏核心(数据安全红线),便于单测。
pub fn decode_editable(bytes: &[u8]) -> Option<String> {
    if bytes.len() > MAX_EDIT_BYTES {
        return None; // 拒编,不截断
    }
    if bytes.contains(&0) {
        return None; // 含 NUL → 疑二进制
    }
    String::from_utf8(bytes.to_vec()).ok() // 非 UTF-8 → None
}

/// 读远端小文本供编辑;None = 不可编辑(前端灰置/提示)。
#[tauri::command]
pub async fn sftp_read_text_for_edit(
    cfg: RemoteConfig,
    path: String,
) -> Result<Option<String>, String> {
    with_sftp(&cfg, move |s| {
        let path = path.clone();
        Box::pin(async move {
            // 护栏前置:先 stat 大小,超限即拒读(不把 GB 级文件整体缓冲入内存,防 OOM)。
            // decode_editable 仍是最终护栏(NUL/非 UTF-8;并冗余复核大小,防 stat 与 read 间竞态)。
            if let Ok(Some(size)) = s.metadata(path.clone()).await.map(|m| m.size) {
                if size > MAX_EDIT_BYTES as u64 {
                    return Ok(None);
                }
            }
            let bytes = s
                .read(path.clone())
                .await
                .map_err(|e| format!("读文件失败: {e}"))?;
            Ok(decode_editable(&bytes))
        })
    })
    .await
}

/// 写回编辑后的文本。过写守卫;保留原文件权限(stat 取 mode,缺省 0o644);
/// `upload_atomic` 原子写(.tmp→删旧→rename);失败传播 Err(前端保留编辑框内容)。
#[tauri::command]
pub async fn sftp_write_text(
    cfg: RemoteConfig,
    path: String,
    content: String,
) -> Result<(), String> {
    guard_write(&path)?;
    with_sftp(&cfg, move |s| {
        let path = path.clone();
        let content = content.clone();
        Box::pin(async move {
            // 保留原权限:stat 取 mode(u32),缺省 0o644(新文件)。
            let mode = s
                .metadata(path.clone())
                .await
                .ok()
                .and_then(|m| m.permissions)
                .map(|p| p & 0o7777)
                .unwrap_or(0o644);
            crate::sftp::upload_atomic(s, &path, content.as_bytes(), mode).await
        })
    })
    .await
}

// === 写命令:mkdir / rename / delete（走 with_sftp,过写守卫）===

/// 拒 Claude 数据源路径的写守卫(返回 Err 便于 `?`)。
fn guard_write(path: &str) -> Result<(), String> {
    if is_protected_claude_data_path(path) {
        return Err(format!(
            "拒绝写 Claude 数据源文件({path})——管理会话文件请用历史浏览器"
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn sftp_mkdir(cfg: RemoteConfig, path: String) -> Result<(), String> {
    guard_write(&path)?;
    with_sftp(&cfg, move |s| {
        let path = path.clone();
        Box::pin(async move {
            s.create_dir(path)
                .await
                .map_err(|e| format!("新建目录失败: {e}"))
        })
    })
    .await
}

#[tauri::command]
pub async fn sftp_rename(cfg: RemoteConfig, from: String, to: String) -> Result<(), String> {
    // from(源)与 to(目标)都过守卫:既不许把 Claude 文件改走,也不许改成 Claude 数据源名。
    guard_write(&from)?;
    guard_write(&to)?;
    with_sftp(&cfg, move |s| {
        let from = from.clone();
        let to = to.clone();
        Box::pin(async move {
            s.rename(from, to)
                .await
                .map_err(|e| format!("重命名失败: {e}"))
        })
    })
    .await
}

/// 删除:`is_dir` 区分 rmdir/rm（rmdir 只删空目录,非空由 server 报错上层提示）。
#[tauri::command]
pub async fn sftp_delete(cfg: RemoteConfig, path: String, is_dir: bool) -> Result<(), String> {
    guard_write(&path)?;
    with_sftp(&cfg, move |s| {
        let path = path.clone();
        Box::pin(async move {
            if is_dir {
                s.remove_dir(path)
                    .await
                    .map_err(|e| format!("删除目录失败（非空?）: {e}"))
            } else {
                s.remove_file(path)
                    .await
                    .map_err(|e| format!("删除文件失败: {e}"))
            }
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_write_rejects_claude_data_allows_normal() {
        assert!(guard_write("/home/pi/.claude/projects/-x/s.jsonl").is_err());
        assert!(guard_write("/home/pi/.claude/sessions/1.json").is_err());
        assert!(guard_write("/home/pi/proj/main.rs").is_ok());
        assert!(guard_write("/home/pi/.claude/settings.json").is_ok()); // 非受保护
    }

    // F49：编辑护栏(数据安全红线)——拒编优于截断/乱码。
    #[test]
    fn decode_editable_guards() {
        assert_eq!(
            decode_editable(b"hello\nworld"),
            Some("hello\nworld".into())
        );
        assert_eq!(
            decode_editable("中文 UTF-8".as_bytes()).as_deref(),
            Some("中文 UTF-8")
        );
        assert_eq!(decode_editable(&[]), Some(String::new())); // 空文件可编辑
        assert_eq!(decode_editable(b"a\0b"), None); // 含 NUL → 疑二进制,拒编
        assert_eq!(decode_editable(&[0xff, 0xfe]), None); // 非 UTF-8,拒编
                                                          // >256KB → 拒编(不截断)
        assert_eq!(decode_editable(&vec![b'x'; MAX_EDIT_BYTES + 1]), None);
        assert!(decode_editable(&vec![b'x'; MAX_EDIT_BYTES]).is_some()); // 恰好上限可编辑
    }

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
        sort_entries(&mut v); // 用生产比较器,改它测试即跟着变(不再假信心)
        assert_eq!(
            v.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            vec!["src", "apple", "Zebra"]
        );
    }

    #[test]
    fn cancel_guard_ptr_eq_no_cross_delete() {
        // D 审计 R2/S4:同 id 两次注册,第一个 guard drop 用 ptr_eq 不误删第二个的 flag。
        let (_f1, g1) = register_cancel("dup-id");
        let (f2, _g2) = register_cancel("dup-id"); // 覆盖 map 里的 flag = f2
        drop(g1); // g1 的 flag != map 现存(f2)→ ptr_eq 不成立→不删
        assert!(
            cancels().lock().unwrap().contains_key("dup-id"),
            "g1 drop 不该误删 f2 的注册项"
        );
        // f2 仍可被取消(标志翻转可达)
        f2.store(true, std::sync::atomic::Ordering::SeqCst);
        drop(_g2);
        assert!(
            !cancels().lock().unwrap().contains_key("dup-id"),
            "g2 drop 摘除自己的项"
        );
    }
}
