//! Batch14-F47：SFTP 文件面板后端——per-host 常驻连接池 + 文件操作命令。
//!
//! ## 与既有 SFTP 写的关系（INVARIANT §1）
//! §1「monitor 零侵入 Claude 数据源」管的是 **monitor 作为监视器**不改坏 Claude 的
//! jsonl/pidfile。本模块是**用户亲自驱动**的通用文件传输面板,与数据源只读契约**正交**
//! （2026-07-10 用户拍板:SFTP 属独立文件传输功能,不算 monitor 写)。防误伤守卫见
//! [`is_protected_claude_data_path`]:SFTP 写命令拒碰 Claude 数据源文件(往正被 Claude
//! 打开的 jsonl 写会损坏会话)——这是防手滑,不是合规。
//! ★〔devbench F10c〕这条承诺**现在有牙了**：六个写入口都过 `guard_write` 这件事由
//! `remote_write_registry::a_user_chosen_remote_write_passes_the_claude_data_fence` 钉着。
//! 在那之前它零判据 —— 删掉任一处 `guard_write?`，全仓一条不红。
//!
//! ## 连接分离 + 已知取舍
//! SFTP 面板连接走**独立 utility 池**,与后端数据源流连接(`ssh_source` 长连接)
//! 分离,不共用——面板操作永不影响会话流。池按 origin 键、取用时校活性、死则重建。
//! - **per-origin 锁串行化**:每 host 单连接,传输整程持 slot 锁 → 同 host 不能边传边浏览、
//!   不能并发两个传输(刻意 v1 取舍;F48 若要并发浏览+传输需给池加多连接,属 F47 范围外)。
//! - **非 UTF-8 文件名**:后端**不拦**对 lossy 名(含 U+FFFD)的写(russh-sftp 已有损解码,
//!   无法寻址真字节)——靠 F48 UI 灰置这些项;`lossy_name` 字段供前端判定。
//! - **空闲回收**:池连接空闲不主动回收(一台机一条 SFTP,YAGNI);死连按需重建
//!   (`drop_pooled` 现仅由 `evict_if_dead` 死连驱逐调用)。**配置改动未主动丢弃旧池连接**——
//!   旧连接滞留到自然死亡才用新配置(F47 取舍/backlog,未接线,别声称已接)。

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::Mutex;

use crate::sftp::{connect_sftp, SftpConn};
use crate::ssh_source::RemoteConfig;

/// 目录项（前端渲染 + 排序用）。
#[derive(Serialize, Clone, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SftpEntry {
    pub name: String,
    /// 绝对路径（父目录 + name，SFTP 恒用 `/`）。
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    /// **C03：显式收窄成 `number`，不许回落到 `ts-rs` 的默认 `bigint`。**
    /// Tauri 命令 IPC 走 `serde_json::to_string` ⇒ 线上是 JSON 文本，`JSON.parse` 永不产 BigInt
    /// ⇒ `bigint` 与运行时不一致。上限论证：文件大小，f64 安全整数上限 2^53-1 ≈ **8 PB**。
    /// **这是 Phase G 报的那个唯一已确认的静默有损点**（Rust `u64` ↔ 手写 `paths.ts` 的 `size: number`）。
    #[cfg_attr(test, ts(type = "number"))]
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
/// （F11 删除带确认),不走文件面板。**结构判定**(与 sftp::is_safe_remote_jsonl 同风格,batch20 起不靠 `.claude` 字面,闭 CLAUDE_CONFIG_DIR 缺口)。
pub fn is_protected_claude_data_path(path: &str) -> bool {
    let p = path.replace('\\', "/");
    // batch20 审计修：**结构判定**，不靠 `/.claude/` 字面——Claude 数据文件结构为 `<任意>/projects/<proj>/<sid>.jsonl`
    // （projects 下恰 2 段）或 `<任意>/sessions/<x>.json`（sessions 下 1 段）。**闭 `CLAUDE_CONFIG_DIR` 重定位缺口**：
    // 重定位后路径成 `<CFGDIR>/projects/.../*.jsonl`，原字面 `/.claude/` 判定会漏、SFTP 面板可覆写 live jsonl。
    let jsonl_protected = p.rfind("/projects/").is_some_and(|i| {
        let parts: Vec<&str> = p[i + "/projects/".len()..].split('/').collect();
        parts.len() == 2
            && !parts[0].is_empty()
            && parts[1].len() > ".jsonl".len()
            && parts[1].ends_with(".jsonl")
    });
    let json_protected = p.rfind("/sessions/").is_some_and(|i| {
        let rest = &p[i + "/sessions/".len()..];
        !rest.contains('/') && rest.len() > ".json".len() && rest.ends_with(".json")
    });
    jsonl_protected || json_protected
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

/// 丢弃某 origin 的池连接（现仅由 `evict_if_dead` 死连驱逐调用；下次操作重建）。
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    /// C03：同 `SftpEntry.size` —— 传输字节数，2^53-1 ≈ 8 PB 够用；显式收窄不回落 `bigint`。
    #[cfg_attr(test, ts(type = "number"))]
    pub transferred: u64,
    /// 总字节;下载时=远端 size,上传时=本地 size。未知(极少)为 0。
    #[cfg_attr(test, ts(type = "number"))]
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

// === 步 23b：远端内部复制 —— 零流量走 `copy-data`，协商不到就退路 **并出声** ===

/// 一次远端内部复制的裁决。
///
/// - `None` ⇒ 走了 `copy-data`，**一个文件字节都没经过这台机器**；
/// - `Some(说明)` ⇒ **退了路**，那一句就是要摆到用户眼前的话（含实际过网字节数）。
///
/// ★ 刻意**不是 `bool`**：`设计/60 §5` 第二段逐字「不许静默退化成 2× 流量 ——
/// 用户看得见『这一趟走的是慢路』」。一个 `bool` 到了界面上还得由界面去编一句话，
/// 而**慢路为什么慢**（没协商到扩展 / 句柄不是 UTF-8 / 服务端判 `OP_UNSUPPORTED`）
/// 只有这一层答得出。把那句话做成返回值的一部分 ⇒ 静默退化在类型上就做不到。
pub type CopyVerdict = Option<String>;

/// 远端内部复制的核心。**语料由调用方给**（一个裸会话），所以它在判据里
/// 既走得了「服务端支持」这一路、也走得了「服务端不支持」那一路 —— 秤 F3 两个方向。
///
/// # 两条路
///
/// **快路**（`rs.copy_data == true` 且两个句柄都能逐字节回送）：
/// `open(src,READ)` · `open(tmp,CREATE|EXCLUDE|WRITE)` · 一条 `copy-data` · `close` ×2
/// · 换名上位。**客户端侧 `SSH_FXP_READ` / `SSH_FXP_WRITE` / `SSH_FXP_DATA` 恒 0 条。**
///
/// **退路**（协商不到 / 句柄非 UTF-8 / 服务端回 `OP_UNSUPPORTED`）：
/// 逐块 `read` → `write` 中转。字节走「远端 → 你的机器 → 远端」，**就是 2× 流量**，
/// 与 `设计/60 §5` 说的「下载再上传」同一件事、同一个代价。
///
/// ⚠ **与「下载再上传」的实现差异如实记**：这里**不落本机磁盘**（块在内存里转手），
/// 省掉一趟本机读写；过网字节数与落盘版**一模一样**，所以「不许静默 2×」那条
/// 承诺的对象没变。选它的理由：不必再造一处本机临时文件的落点与清理
/// （那会多一处 `write_site_registry` 管辖的本机写面）。
///
/// # 落地纪律照 [`upload_inner`]
///
/// 先写 `<to>.part`（**EXCLUDE** 创建，防 symlink 预置 clobber，同 `upload_atomic`）
/// → 成功后删旧 → rename 上位。**半途失败绝不在正名上留半截文件**。
///
/// # 它不守什么（逐条，别读大）
///
/// - **只复制一个普通文件**。目录递归不在这一层（`copy-data` 自己也只吃文件句柄；
///   本机 `sftp` 客户端现打同样拒：`Cannot copy non-regular file: %s`）。
/// - **退路没有取消点之外的断点续传**：取消/失败即清 `.part`，下次从头。
/// - 服务端报了 `copy-data` 却回**别的**错误状态（权限 / 磁盘满）⇒ **原样报错，不退路**。
///   退路只接「协商不到」那一族 —— 拿退路去盖真实故障，会把「远端满了」伪装成「慢了点」。
pub async fn copy_remote_path(
    rs: &crate::sftp::RawSftp,
    from: &str,
    to: &str,
    cancel: &AtomicBool,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<CopyVerdict, String> {
    use russh_sftp::protocol::{Packet, StatusCode};

    let total = rs
        .raw
        .stat(from.to_string())
        .await
        .ok()
        .and_then(|a| a.attrs.size)
        .unwrap_or(0);
    progress(0, total);

    let tmp = format!("{to}.part");
    let _ = rs.raw.remove(tmp.clone()).await; // best-effort 清残留/预置

    let h_src = rs
        .raw
        .open(
            from.to_string(),
            russh_sftp::protocol::OpenFlags::READ,
            attrs_empty(),
        )
        .await
        .map_err(|e| format!("打开远端源 {from} 失败: {e}"))?
        .handle;
    let h_dst = match rs
        .raw
        .open(
            tmp.clone(),
            russh_sftp::protocol::OpenFlags::CREATE
                | russh_sftp::protocol::OpenFlags::EXCLUDE
                | russh_sftp::protocol::OpenFlags::WRITE,
            attrs_empty(),
        )
        .await
    {
        Ok(h) => h.handle,
        Err(e) => {
            let _ = rs.raw.close(h_src).await;
            return Err(format!("创建远端 {tmp} 失败: {e}"));
        }
    };

    // ── 选路。**三个岔口，每个都留一句给用户的话** ──────────────────────────
    let mut verdict: CopyVerdict = if !rs.copy_data {
        Some("远端的 sftp-server 握手时没报 `copy-data` 扩展（或修订号不是 1）".to_string())
    } else if handle_is_lossy(&h_src) || handle_is_lossy(&h_dst) {
        // russh-sftp 解 SFTP 的 `string` 字段时对非 UTF-8 走 `from_utf8_lossy`
        // （现打核过 3.0.0 那份取 string 的辅助函数）⇒ 句柄里的字节被换成了 U+FFFD，
        // **再发回去就不是同一个句柄**。OpenSSH 的句柄是 4 字节大端的句柄序号，
        // 序号 ≥ 0x80 时末字节就不是合法 UTF-8 ⇒ 同一条会话开到 128 个以上句柄才碰得到。
        // ⚠ 这一条是**库的既有缺陷**（`read`/`write`/`close` 同样受影响），不是本路新增；
        //   但 `copy-data` 是唯一一处**我们自己把句柄再序列化一遍**的地方 ⇒ 这里必须判。
        Some("远端给的 SFTP 句柄含非 UTF-8 字节，库已有损解码 ⇒ 不敢照原样发回去".to_string())
    } else {
        None
    };

    let core = async {
        if verdict.is_none() {
            let body: Vec<u8> = crate::sftp::CopyDataExtension {
                read_from_handle: h_src.clone(),
                read_from_offset: 0,
                read_data_length: 0, // 0 = 一直读到 EOF（现打验过，见 sftp.rs 那张突变表）
                write_to_handle: h_dst.clone(),
                write_to_offset: 0,
            }
            .try_into()?;
            match rs.raw.extended(crate::sftp::COPY_DATA, body).await {
                Ok(Packet::Status(s)) if s.status_code == StatusCode::Ok => return Ok(0u64),
                Ok(Packet::Status(s)) if s.status_code == StatusCode::OpUnsupported => {
                    verdict = Some(
                        "远端报了 `copy-data`，可真发过去它回 `SSH_FX_OP_UNSUPPORTED`".to_string(),
                    );
                }
                Ok(Packet::Status(s)) => {
                    // 真实故障（权限 / 空间 / 路径）—— **不拿退路去盖它**。
                    return Err(format!(
                        "远端 copy-data 失败（{:?}）: {}",
                        s.status_code, s.error_message
                    ));
                }
                Ok(_) => return Err("远端对 copy-data 回了个非 STATUS 包".to_string()),
                Err(e) => return Err(format!("发 copy-data 失败: {e}")),
            }
        }
        // ── 退路：逐块中转。**这里每一块都是真的 2× 流量** ──────────────────
        let mut off: u64 = 0;
        let mut last_report: u64 = 0;
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err("已取消".to_string());
            }
            let chunk = match rs.raw.read(h_src.clone(), off, CHUNK as u32).await {
                Ok(d) => d.data,
                Err(russh_sftp::client::error::Error::Status(s))
                    if s.status_code == StatusCode::Eof =>
                {
                    break
                }
                Err(e) => return Err(format!("读远端源失败: {e}")),
            };
            if chunk.is_empty() {
                break;
            }
            let n = chunk.len() as u64;
            rs.raw
                .write(h_dst.clone(), off, chunk)
                .await
                .map_err(|e| format!("写远端目标失败: {e}"))?;
            off += n;
            if off - last_report >= PROGRESS_EVERY {
                last_report = off;
                progress(off, total);
            }
        }
        Ok(off)
    }
    .await;

    let _ = rs.raw.close(h_src).await;
    let _ = rs.raw.close(h_dst).await;

    let relayed = match core {
        Ok(n) => n,
        Err(e) => {
            let _ = rs.raw.remove(tmp.clone()).await; // 清半成品 .part
            return Err(e);
        }
    };

    // 目标原文件在此之前完好无损；此后才删旧 + 换名（russh-sftp 的 rename 不覆盖）。
    if rs.raw.stat(to.to_string()).await.is_ok() {
        rs.raw
            .remove(to.to_string())
            .await
            .map_err(|e| format!("删旧 {to} 失败: {e}"))?;
    }
    rs.raw
        .rename(tmp.clone(), to.to_string())
        .await
        .map_err(|e| format!("rename {tmp} → {to} 失败: {e}"))?;
    progress(total.max(relayed), total);

    // 退路那句话在这里才**装上读数** —— 「慢」不是形容词，是一个字节数。
    Ok(verdict
        .map(|why| format!("{why} ⇒ 退回中转：{relayed} 字节经过了你这台机器（零流量复制没走上）")))
}

/// 一个空属性块（`SSH_FXP_OPEN` 的 attrs 位图全 0）。
///
/// 抽成函数**不是**为了省字：`russh_sftp::protocol::FileAttributes::empty()` 在
/// `copy_remote_path` 里要写两遍，而那两处必须一模一样（一处带了 permissions
/// 就会在 `EXCLUDE` 创建时改变落地权限）。
fn attrs_empty() -> russh_sftp::protocol::FileAttributes {
    russh_sftp::protocol::FileAttributes::empty()
}

/// 这个 SFTP 句柄是不是已经被库有损解码过了（含 U+FFFD ⇒ 原字节回不去）。
///
/// 与 [`is_lossy_name`] 同一条性质、**刻意不复用那一个**：那个判的是**文件名**
/// （有损只影响显示），这个判的是**句柄**（有损意味着「发回去就是另一个句柄」）。
/// 两个判据的后果完全不同，共用一个名字会让下一个人以为改一处就够。
fn handle_is_lossy(handle: &str) -> bool {
    handle.contains('\u{FFFD}')
}

/// 远端内部复制。**零流量优先，退路必出声。**
///
/// 返回 `null` = 服务端内部复制（`copy-data`），一个文件字节都没过网；
/// 返回一串话 = **退了路**，那串话就是要给用户看的（已含实际过网字节数）。
///
/// `from`（源）与 `to`（目标）**各过一次** `guard_write` —— 照 [`sftp_rename`] 的先例：
/// 既不许把 Claude 的会话文件复制走，也不许复制成一个 Claude 数据源名
/// （往正被 Claude 打开的 jsonl 上盖一份复制品，和覆写它一样会损坏会话）。
/// ★ **池化那一段刻意留在本函数体内、不抽 `copy_inner`**（`download_inner` /
/// `upload_inner` 那两条是抽出去的）。理由是判据的形状，不是风格：
/// `remote_write_registry::the_ipc_entry_points_route_through_a_registered_write_site`
/// 那张路由表是 `(入口所在文件, 入口名, 它该转发到的已登记写点)` **一跳**的 ——
/// 中间多垫一层 `copy_inner`，「按钮 ↔ 真实写点」那条边就表达不出来，
/// 而那条边正是那一条判据存在的全部理由（「两个不同的层，一条边」）。
/// ⇒ 让 `sftp_copy` 的函数体里**直接点名** `copy_remote_path`。
///
/// **不走 `with_sftp` 的重试**：同 `download_inner` / `upload_inner`，
/// 半途失败不静默从头重来（`.part` 已清，重来由用户决定）。
#[tauri::command]
pub async fn sftp_copy(
    cfg: RemoteConfig,
    from: String,
    to: String,
    transfer_id: String,
    on_progress: tauri::ipc::Channel<TransferProgress>,
) -> Result<CopyVerdict, String> {
    guard_write(&from)?;
    guard_write(&to)?;
    let (cancel, _guard) = register_cancel(&transfer_id); // _guard 摘除注册项(含 abort)
    let r = async {
        let slot = slot_for(&cfg.origin_label()).await;
        let mut guard = slot.lock().await;
        if guard.is_none() {
            *guard = Some(connect_sftp(&cfg).await?);
        }
        let rs = guard.as_ref().unwrap().open_raw_sftp().await?;
        let report_to = |done: u64, total: u64| report(&on_progress, done, total);
        copy_remote_path(&rs, &from, &to, &cancel, &report_to).await
    }
    .await;
    if let Err(e) = &r {
        evict_if_dead(&cfg.origin_label(), e).await; // R1:死连不留在槽里毒化后续
    }
    r
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
#[path = "../../../tests/bridge/sftp_pool_tests.rs"]
mod tests;

/// **秤 F3**（`设计/17 §6.9`）：零流量复制的对拍，正反两个方向。
/// 刻意**另立一个模块**而不是塞进上面那份 —— 它自带一个合成 SFTP 服务端与一层
/// 按字节数包的计数流，是一台**台架**，与 `sftp_pool_tests` 那些单点判据不同族。
#[cfg(test)]
#[path = "../../../tests/bridge/sftp_copy_f3_tests.rs"]
mod copy_f3_tests;
