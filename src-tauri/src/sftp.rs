//! SS-D：统一 SFTP 会话层（issue #29 自动部署 F08；后续 F11 用户数据写 / F10 profile 写复用）。
//!
//! 复用 `ssh_source::connect_session` 的全套 host-key 指纹校验 + publickey/agent 鉴权，
//! 在一条已鉴权的 russh 连接上 `request_subsystem("sftp")` 起 SFTP 子系统（russh-sftp，
//! transport-agnostic，吃 channel 的 AsyncRead+AsyncWrite 流）。
//!
//! ## 只读铁律豁免（INVARIANT §1 / 账本 SS-G）
//! cc-monitor 对远端的写入**仅限**两类，各自独立 realpath 白名单、绝不混用：
//! - **F08**：自部署 daemon 二进制到 `~/.cc-monitor/bin/`（非用户数据、幂等、版本门控）。
//! - **F11（后续）**：用户**主动**删除 / 改 metadata 到 `~/.claude/`。
//!
//! 本模块当前只实现 F08 的部署写。
//!
//! ## 原子写
//! russh-sftp 无 `posix-rename@openssh.com` 扩展，标准 SFTP `rename` 不覆盖已存在目标。
//! 故 [`upload_atomic`] 用「写 `<path>.tmp` → 删旧 `<path>` → rename」近似原子（单写者、
//! 低频部署场景足够；删与 rename 之间的窗口极短且无并发读者）。

use russh::client;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, OpenFlags};
use tokio::io::AsyncWriteExt;

use crate::ssh_source::{connect_session, ClientHandler, RemoteConfig};

/// 一条 SFTP 连接：持有底层 russh `Handle`（**必须与 SFTP 会话同生命周期**——Handle 一 drop
/// 整条 SSH 连接就断）+ SFTP 会话本身。
pub struct SftpConn {
    /// 保活：底层 SSH 连接句柄。下划线 = 仅持有不直接用，但绝不能提前 drop。
    _session: client::Handle<ClientHandler>,
    pub sftp: SftpSession,
}

/// 打开到远端的 SFTP 会话（复用 connect_session 全套指纹/鉴权）。
pub async fn connect_sftp(cfg: &RemoteConfig) -> Result<SftpConn, String> {
    let (session, _fp) = connect_session(cfg, None).await?;
    let channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("打开 SFTP channel 失败: {e}"))?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| format!("请求 sftp 子系统失败（远端 sshd 未开 sftp?）: {e}"))?;
    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| format!("初始化 sftp 会话失败: {e}"))?;
    Ok(SftpConn {
        _session: session,
        sftp,
    })
}

/// 原子上传 `bytes` 到 `remote_path`，权限 `mode`（八进制如 0o700）。
///
/// 写 `<remote_path>.tmp` → 删旧 → rename（见模块文档「原子写」）。`mode` 在创建时经
/// FileAttributes 设置，rename 后再 `set_metadata` 兜底（部分 server 创建时 attrs 不生效）。
pub async fn upload_atomic(
    sftp: &SftpSession,
    remote_path: &str,
    bytes: &[u8],
    mode: u32,
) -> Result<(), String> {
    let tmp = format!("{remote_path}.tmp");
    let attrs = FileAttributes {
        permissions: Some(mode),
        ..Default::default()
    };
    let mut file = sftp
        .open_with_flags_and_attributes(
            tmp.clone(),
            OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
            attrs,
        )
        .await
        .map_err(|e| format!("创建 {tmp} 失败: {e}"))?;
    file.write_all(bytes)
        .await
        .map_err(|e| format!("写 {tmp} 失败: {e}"))?;
    file.sync_all().await.ok();
    file.shutdown().await.ok();
    drop(file);

    // rename 不覆盖 → 先删旧目标（存在才删）。
    if sftp.try_exists(remote_path.to_string()).await.unwrap_or(false) {
        sftp.remove_file(remote_path.to_string())
            .await
            .map_err(|e| format!("删除旧文件 {remote_path} 失败: {e}"))?;
    }
    sftp.rename(tmp.clone(), remote_path.to_string())
        .await
        .map_err(|e| format!("rename {tmp} → {remote_path} 失败: {e}"))?;
    // 兜底设权限（best-effort）。
    let _ = sftp
        .set_metadata(
            remote_path.to_string(),
            FileAttributes {
                permissions: Some(mode),
                ..Default::default()
            },
        )
        .await;
    Ok(())
}

/// 读远端文件，不存在 / 读失败 → None。
async fn read_optional(sftp: &SftpSession, path: &str) -> Option<Vec<u8>> {
    sftp.read(path.to_string()).await.ok()
}

/// mkdir -p：逐级创建 `dir`（绝对或相对），已存在则跳过，创建失败容忍（并发/权限留给上传报错）。
async fn ensure_dir_all(sftp: &SftpSession, dir: &str) {
    let mut cur = String::new();
    for comp in dir.split('/').filter(|c| !c.is_empty()) {
        cur.push('/');
        cur.push_str(comp);
        if !sftp.try_exists(cur.clone()).await.unwrap_or(false) {
            let _ = sftp.create_dir(cur.clone()).await;
        }
    }
}

/// 内嵌的 daemon 二进制（F08b 由 `include_bytes!` 填充）。`build_id` 与
/// `ssh_source::EXPECTED_DAEMON_BUILD_ID` 同源（SS-B）。
pub struct DaemonBinary {
    pub build_id: &'static str,
    pub bytes: &'static [u8],
}

/// 部署决策（纯函数，可单测）。
#[derive(Debug, PartialEq, Eq)]
pub enum DeployAction {
    /// 远端版本与期望一致 → 无需部署。
    Skip,
    /// 需要部署，附人读原因。
    Deploy(String),
}

/// 比对远端版本标记与期望 build_id，决定是否（重）部署。
pub fn deploy_decision(remote_build_id: Option<&str>, expected: &str) -> DeployAction {
    match remote_build_id {
        None => DeployAction::Deploy("远端无 daemon / 无版本标记".to_string()),
        Some(r) if r.trim() != expected => {
            DeployAction::Deploy(format!("版本不符（远端 {} ≠ 期望 {expected}）", r.trim()))
        }
        Some(_) => DeployAction::Skip,
    }
}

/// 远端路径的父目录（远端恒为 POSIX `/` 分隔，不用 std::path）。
fn remote_parent(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) => "/",
        Some(i) => &path[..i],
        None => ".",
    }
}

/// 版本标记文件路径：daemon 二进制同目录下 `.build_id`。
fn marker_path(daemon_path: &str) -> String {
    let dir = remote_parent(daemon_path);
    if dir == "/" {
        "/.build_id".to_string()
    } else {
        format!("{dir}/.build_id")
    }
}

/// 连接前确保远端 daemon 已部署到 `cfg.daemon_path`（issue #29）。
///
/// `binary = None`（F08b 嵌入二进制就位前）→ 优雅 no-op（debug log + Ok）。
/// `Some` → 开 SFTP、读版本标记、`deploy_decision`、需要则 mkdir -p + 原子上传 + 写标记。
///
/// **best-effort**：调用方（ssh_source::run）对 Err 仅 warn 不阻断——手动部署的 daemon 仍可连。
///
/// **F08b 前置要求（审计 S-2）**：`cfg.daemon_path` 必须是**绝对路径**。SFTP 无 shell，
/// 不展开 `~`——而 daemon 的 shell exec 路径（connect_and_exec）**会**展开 `~`。若 daemon_path
/// 用了 `~`，部署会落到字面 `./~/...`、而 exec 找的是真 home，两边错位。F08b 激活上传前需
/// 在此 canonicalize 或校验 daemon_path 以绝对路径起头。
pub async fn ensure_daemon_deployed(
    cfg: &RemoteConfig,
    binary: Option<&DaemonBinary>,
) -> Result<(), String> {
    let Some(bin) = binary else {
        tracing::debug!("ensure_daemon_deployed: 无内嵌 daemon 二进制（F08b 待嵌入），跳过自动部署");
        return Ok(());
    };
    let conn = connect_sftp(cfg).await?;
    let sftp = &conn.sftp;

    let marker = marker_path(&cfg.daemon_path);
    let remote_id =
        read_optional(sftp, &marker).await.map(|b| String::from_utf8_lossy(&b).trim().to_string());

    match deploy_decision(remote_id.as_deref(), bin.build_id) {
        DeployAction::Skip => {
            tracing::info!("远端 [{}] daemon 已是 {}，跳过部署", cfg.origin_label(), bin.build_id);
        }
        DeployAction::Deploy(reason) => {
            tracing::info!(
                "远端 [{}] 自动部署 daemon（{reason}）→ {}",
                cfg.origin_label(),
                cfg.daemon_path
            );
            ensure_dir_all(sftp, remote_parent(&cfg.daemon_path)).await;
            upload_atomic(sftp, &cfg.daemon_path, bin.bytes, 0o700).await?;
            upload_atomic(sftp, &marker, bin.build_id.as_bytes(), 0o600).await?;
            tracing::info!("远端 [{}] daemon 部署完成：{}", cfg.origin_label(), bin.build_id);
        }
    }
    Ok(())
}

/// 当前内嵌的 daemon 二进制（按 host_arch 选）。F08b 由 `include_bytes!` 填充 aarch64/x86_64
/// musl 二进制；F08a 阶段返回 None（→ ensure_daemon_deployed 优雅跳过，沿用手动部署）。
pub fn daemon_binary() -> Option<&'static DaemonBinary> {
    None
}

// ============================================================================
// F11：远端用户数据写（删除远端历史 jsonl）。SS-G item 3 的唯一 SFTP 用户数据写。
// ============================================================================

/// 远端历史 jsonl 删除路径的安全守卫（纯函数，可单测）。
///
/// 仅允许删除**远端 claude_dir 下符合会话 jsonl 结构的文件**。会话 jsonl 的真实结构恒为
/// `<claude_dir>/projects/<encoded_cwd 单层目录>/<sid>.jsonl`，故要求：
/// - 不含 `..`（防上跳）；
/// - 最后一个 `/projects/` 之后**正好是 `<一层目录>/<name>.jsonl`**（split 后恰 2 段、
///   首段非空非 `.`、末段以 `.jsonl` 结尾且不只是 `.jsonl`）。
///
/// 这比裸 `contains("/projects/")` 强：挡住 `/tmp/projects/x.jsonl`（projects 下直接放
/// jsonl）、`/a/projects/b/c/x.jsonl`（层级不符）这类伪造路径；且**不硬编码 `.claude`**，
/// 兼容 `CLAUDE_CONFIG_DIR` 自定义目录（审计 S-1：`/.claude/projects/` 会误伤自定义目录）。
///
/// 残留（审计登记，后续加固）：完全锚定需远端 daemon 上报的 `claude_dir`（一次性删除连接
/// 无 hello）。但威胁仅「**已被攻陷的 daemon** 喂伪造路径」——而被攻陷 daemon 本就能在远端
/// 任意删文件，monitor 删一个 `projects/*.jsonl` 不增加其能力（非提权）；叠加用户**二次确认**，
/// 残留风险为纵深防御层面。
pub fn is_safe_remote_jsonl(path: &str) -> bool {
    if path.contains("..") || !path.ends_with(".jsonl") {
        return false;
    }
    let Some(idx) = path.rfind("/projects/") else {
        return false;
    };
    let rest = &path[idx + "/projects/".len()..];
    let parts: Vec<&str> = rest.split('/').collect();
    parts.len() == 2
        && !parts[0].is_empty()
        && parts[0] != "."
        && parts[1].len() > ".jsonl".len()
        && parts[1].ends_with(".jsonl")
}

/// 删除远端文件（issue 未拆，F11）：**仅**用于用户主动删除远端历史 jsonl。
///
/// 双重守卫：① 入参先过 [`is_safe_remote_jsonl`]；② SFTP `canonicalize`（realpath，解 symlink）
/// 后**再**校验 canonical 仍含 `/projects/` 且以 `.jsonl` 结尾——挡住 projects/ 内指向外部的
/// symlink 逃逸。只读铁律豁免（SS-G）：仅此一处对远端 `~/.claude/` 的写，且用户显式触发。
pub async fn remove_remote_file(cfg: &RemoteConfig, remote_path: &str) -> Result<(), String> {
    if !is_safe_remote_jsonl(remote_path) {
        return Err(format!("拒绝删除非法远端路径（须为 projects/ 下 .jsonl）: {remote_path}"));
    }
    let conn = connect_sftp(cfg).await?;
    let sftp = &conn.sftp;
    // realpath 解析 symlink 后二次校验，防 projects/ 内 symlink 指向外部文件。
    let canon = sftp
        .canonicalize(remote_path.to_string())
        .await
        .map_err(|e| format!("解析远端路径失败: {e}"))?;
    if !is_safe_remote_jsonl(&canon) {
        return Err(format!("拒绝删除：canonical 路径越出 projects/ 或非 jsonl: {canon}"));
    }
    sftp.remove_file(canon.clone())
        .await
        .map_err(|e| format!("删除远端文件失败: {e}"))?;
    tracing::info!("远端 [{}] 已删除历史会话: {canon}", cfg.origin_label());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deploy_decision_truth_table() {
        // 无标记 → 部署
        assert!(matches!(
            deploy_decision(None, "p1b-overflow"),
            DeployAction::Deploy(_)
        ));
        // 版本不符 → 部署
        assert!(matches!(
            deploy_decision(Some("p1a-history"), "p1b-overflow"),
            DeployAction::Deploy(_)
        ));
        // 一致（含尾随空白）→ 跳过
        assert_eq!(
            deploy_decision(Some("p1b-overflow"), "p1b-overflow"),
            DeployAction::Skip
        );
        assert_eq!(
            deploy_decision(Some("p1b-overflow\n"), "p1b-overflow"),
            DeployAction::Skip,
            "标记文件可能带尾随换行，trim 后比对"
        );
    }

    #[test]
    fn is_safe_remote_jsonl_guard() {
        // 合法：projects/<单层目录>/<sid>.jsonl
        assert!(is_safe_remote_jsonl(
            "/home/pi/.claude/projects/proj/abc-123.jsonl"
        ));
        // 兼容 CLAUDE_CONFIG_DIR 自定义目录（不硬编码 .claude）
        assert!(is_safe_remote_jsonl(
            "/opt/claude-data/projects/-home-pi-x/sid.jsonl"
        ));
        // 非 .jsonl → 拒
        assert!(!is_safe_remote_jsonl("/home/pi/.claude/projects/proj/note.txt"));
        assert!(!is_safe_remote_jsonl("/home/pi/.claude/projects/proj/abc.json"));
        // 不在 projects/ → 拒
        assert!(!is_safe_remote_jsonl("/home/pi/.ssh/id_ed25519.jsonl"));
        assert!(!is_safe_remote_jsonl("/etc/passwd.jsonl"));
        // 含 .. 上跳 → 拒
        assert!(!is_safe_remote_jsonl(
            "/home/pi/.claude/projects/../../../etc/x.jsonl"
        ));
        // 审计 S-1：projects 下直接放 jsonl（无中间目录层）→ 拒
        assert!(!is_safe_remote_jsonl("/tmp/projects/x.jsonl"));
        // 层级过深（≠ <dir>/<sid>.jsonl）→ 拒
        assert!(!is_safe_remote_jsonl("/a/projects/b/c/x.jsonl"));
        // 文件名只是 ".jsonl" → 拒
        assert!(!is_safe_remote_jsonl("/x/projects/dir/.jsonl"));
        // 空中间目录段 → 拒
        assert!(!is_safe_remote_jsonl("/x/projects//abc.jsonl"));
    }

    #[test]
    fn remote_parent_and_marker() {
        assert_eq!(
            remote_parent("/home/pi/.cc-monitor/bin/cc-monitor-remote"),
            "/home/pi/.cc-monitor/bin"
        );
        assert_eq!(remote_parent("/x"), "/");
        assert_eq!(remote_parent("rel/path"), "rel");
        assert_eq!(remote_parent("noslash"), ".");
        assert_eq!(
            marker_path("/home/pi/.cc-monitor/bin/cc-monitor-remote"),
            "/home/pi/.cc-monitor/bin/.build_id"
        );
        assert_eq!(marker_path("/x"), "/.build_id");
    }
}
