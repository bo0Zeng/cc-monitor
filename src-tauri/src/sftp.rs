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

use std::time::{SystemTime, UNIX_EPOCH};

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
    // russh-sftp 的 `write_all` 只把 WRITE 包入队（write_nowait），ack 只在 `flush`/`shutdown`
    // 的 poll_drain_writes 里 drain。用 `flush()`（**始终** drain，不像 sync_all 在服务器无
    // `fsync@openssh` 时 noop 不 drain）+ **传播错误**，确保数据真正落服务器、失败不静默。
    file.flush()
        .await
        .map_err(|e| format!("flush {tmp} 失败（写未确认）: {e}"))?;
    file.shutdown()
        .await
        .map_err(|e| format!("关闭 {tmp} 失败: {e}"))?;
    drop(file);

    // rename 不覆盖 → 先删旧目标（存在才删）。
    if sftp
        .try_exists(remote_path.to_string())
        .await
        .unwrap_or(false)
    {
        sftp.remove_file(remote_path.to_string())
            .await
            .map_err(|e| format!("删除旧文件 {remote_path} 失败: {e}"))?;
    }
    sftp.rename(tmp.clone(), remote_path.to_string())
        .await
        .map_err(|e| format!("rename {tmp} → {remote_path} 失败: {e}"))?;
    // **绝不**在这里 `set_metadata(permissions)` 兜底 chmod —— 真机 e2e 诊断确证：在 OpenSSH
    // sftp-server 上 setstat（即便只设 permissions、size=None）会把刚 rename 好的文件**截断成
    // 0 字节**（tmp 写后 size 正确、rename 直后 size 正确，唯独 set_metadata 之后变 0）。daemon
    // 因此变 0 字节不可 exec → 连接 EOF，marker 变空 → 无限重部署。权限已在 open-create 的 attrs
    // 里设好（OpenSSH 按 SSH_FXP_OPEN attrs 建文件：0o700 可执行 / 0o600）、rename 保留权限，无需
    // 也不能再 set_metadata。
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

/// 探测远端 CPU 架构（`uname -m`）以选对应的内嵌 daemon 二进制（F08b）。一次性 exec。
async fn probe_remote_arch(cfg: &RemoteConfig) -> Result<String, String> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let stream = crate::ssh_source::connect_and_exec_cmd(cfg, "uname -m").await?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .map_err(|e| format!("读 uname -m 失败: {e}"))?;
    let arch = line.trim().to_string();
    if arch.is_empty() {
        return Err("uname -m 空输出".to_string());
    }
    Ok(arch)
}

/// 连接前确保远端 daemon 已（自动）部署到 `cfg.daemon_path`（issue #29）。
///
/// 流程：① S-2 守卫（daemon_path 含 `~` → 跳过，SFTP 不展开 `~`）；② 探测远端 arch 选内嵌
/// 二进制（[`daemon_binary`]）——无对应 arch 内嵌（F08b 未嵌入该 arch）则**优雅 no-op**；
/// ③ 开 SFTP、读版本标记、[`deploy_decision`]、需要则 mkdir -p + 原子上传 + 写标记。
///
/// **best-effort**：调用方（ssh_source::run）对 Err 仅 warn 不阻断——手动部署的 daemon 仍可连。
pub async fn ensure_daemon_deployed(cfg: &RemoteConfig) -> Result<(), String> {
    // S-2（审计）：SFTP 无 shell 不展开 `~`，而 daemon exec 路径会展开——daemon_path 含 `~`
    // 会两边错位。含 `~` 直接跳过自动部署（用户应填绝对路径），手动部署的 daemon 仍可连。
    if cfg.daemon_path.contains('~') {
        tracing::debug!(
            "daemon_path 含 ~（SFTP 不展开），跳过自动部署：{}",
            cfg.daemon_path
        );
        return Ok(());
    }
    // 探测 arch 选内嵌二进制；探测失败 / 无该 arch 内嵌 → 优雅 no-op（沿用手动部署）。
    let arch = match probe_remote_arch(cfg).await {
        Ok(a) => a,
        Err(e) => {
            tracing::debug!("远端 arch 探测失败，跳过自动部署: {e}");
            return Ok(());
        }
    };
    let Some(bin) = daemon_binary(&arch) else {
        tracing::debug!("无 {arch} 的内嵌 daemon 二进制（F08b 未嵌入该 arch?），跳过自动部署");
        return Ok(());
    };
    let conn = connect_sftp(cfg).await?;
    let sftp = &conn.sftp;

    let marker = marker_path(&cfg.daemon_path);
    let remote_id = read_optional(sftp, &marker)
        .await
        .map(|b| String::from_utf8_lossy(&b).trim().to_string());

    match deploy_decision(remote_id.as_deref(), bin.build_id) {
        DeployAction::Skip => {
            tracing::info!(
                "远端 [{}] daemon 已是 {}，跳过部署",
                cfg.origin_label(),
                bin.build_id
            );
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
            tracing::info!(
                "远端 [{}] daemon 部署完成：{}",
                cfg.origin_label(),
                bin.build_id
            );
        }
    }
    Ok(())
}

/// 按远端 arch 选内嵌的 daemon 二进制（F08b）。build.rs 把交叉编译的 musl 二进制复制进
/// OUT_DIR 并置 `embedded_daemons` cfg 时，这里 `include_bytes!` 内嵌并按 arch 返回；二进制
/// 未就位（无 cfg）→ 返回 None（ensure_daemon_deployed 优雅跳过，沿用手动部署）。
/// `build_id` 取编译期 env（来自 daemon 源码，SS-B 单源）。
pub fn daemon_binary(arch: &str) -> Option<&'static DaemonBinary> {
    #[cfg(embedded_daemons)]
    {
        static X86: DaemonBinary = DaemonBinary {
            build_id: env!("DAEMON_BUILD_ID"),
            bytes: include_bytes!(concat!(env!("OUT_DIR"), "/daemon-x86_64")),
        };
        static ARM: DaemonBinary = DaemonBinary {
            build_id: env!("DAEMON_BUILD_ID"),
            bytes: include_bytes!(concat!(env!("OUT_DIR"), "/daemon-aarch64")),
        };
        match arch {
            "x86_64" | "amd64" => Some(&X86),
            "aarch64" | "arm64" => Some(&ARM),
            _ => None,
        }
    }
    #[cfg(not(embedded_daemons))]
    {
        let _ = arch;
        None
    }
}

// ============================================================================
// F08c：手动安装 / 卸载 daemon（设置面板两个按钮）。安装逻辑同自动部署、但返回人读结果；
// 卸载删 daemon 二进制 + 同目录 .build_id（is_safe_remote_daemon_path 守卫）。
// ============================================================================

/// 远端 daemon 路径安全守卫（卸载用，纯函数可单测）：绝对、无 `..`、非根、且含 `cc-monitor`
/// （约定 `~/.cc-monitor/bin/cc-monitor-remote`）—— 杜绝把卸载误用成删任意远端文件。
pub fn is_safe_remote_daemon_path(path: &str) -> bool {
    let p = path.trim();
    !p.is_empty() && p.starts_with('/') && !p.contains("..") && p != "/" && p.contains("cc-monitor")
}

/// 手动安装 / 更新远端 daemon（设置面板「安装 daemon」按钮）。逻辑同自动部署
/// [`ensure_daemon_deployed`]，但**返回人读结果**，且把自动部署里「优雅跳过」的几种情况
/// （路径含 `~` / 探测不到 arch / 无该 arch 内嵌）显式报错——手动触发时用户要反馈。
#[tauri::command]
pub async fn deploy_remote_daemon(cfg: RemoteConfig) -> Result<String, String> {
    let path = cfg.daemon_path.trim().to_string();
    if path.is_empty() {
        return Err(
            "请先填 daemon 路径（绝对路径，如 /home/<user>/.cc-monitor/bin/cc-monitor-remote）"
                .into(),
        );
    }
    if path.contains('~') {
        return Err("daemon 路径含 ~（SFTP 不展开 ~），请改用绝对路径".into());
    }
    let arch = probe_remote_arch(&cfg)
        .await
        .map_err(|e| format!("探测远端架构失败（uname -m）: {e}"))?;
    let Some(bin) = daemon_binary(&arch) else {
        return Err(format!(
            "本 monitor 构建未内嵌 {arch} 架构的 daemon，无法一键安装。请用内嵌了该架构的发布版，或手动把 daemon 放到 {path}。"
        ));
    };
    let conn = connect_sftp(&cfg).await?;
    let sftp = &conn.sftp;
    let marker = marker_path(&path);
    let remote_id = read_optional(sftp, &marker)
        .await
        .map(|b| String::from_utf8_lossy(&b).trim().to_string());
    match deploy_decision(remote_id.as_deref(), bin.build_id) {
        DeployAction::Skip => Ok(format!(
            "远端已是最新 daemon（{}，{arch}）：{path}，无需重装。",
            bin.build_id
        )),
        DeployAction::Deploy(reason) => {
            ensure_dir_all(sftp, remote_parent(&path)).await;
            upload_atomic(sftp, &path, bin.bytes, 0o700).await?;
            upload_atomic(sftp, &marker, bin.build_id.as_bytes(), 0o600).await?;
            tracing::info!(
                "远端 [{}] 手动部署 daemon 完成：{}",
                cfg.origin_label(),
                bin.build_id
            );
            Ok(format!(
                "已安装 daemon（{}，{arch}）到 {path}（{reason}）。重连远端即可用。",
                bin.build_id
            ))
        }
    }
}

/// 卸载远端 daemon（设置面板「卸载 daemon」按钮）：删 daemon 二进制 + 同目录 `.build_id`。
/// [`is_safe_remote_daemon_path`] 守卫。只读铁律豁免（SS-G）：用户显式触发的删。
/// 注意：若该机器仍启用，自动部署会在下次连接重新装回——提示见返回消息。
#[tauri::command]
pub async fn uninstall_remote_daemon(cfg: RemoteConfig) -> Result<String, String> {
    let path = cfg.daemon_path.trim().to_string();
    if path.contains('~') {
        return Err("daemon 路径含 ~（SFTP 不展开），请改用绝对路径后再卸载".into());
    }
    if !is_safe_remote_daemon_path(&path) {
        return Err(format!(
            "拒绝删除可疑 daemon 路径（须为含 cc-monitor 的绝对路径、无 ..）: {path}"
        ));
    }
    let conn = connect_sftp(&cfg).await?;
    let sftp = &conn.sftp;
    let marker = marker_path(&path);
    let mut removed = Vec::new();
    for f in [path.clone(), marker.clone()] {
        if sftp.remove_file(f.clone()).await.is_ok() {
            removed.push(f);
        }
    }
    tracing::info!(
        "远端 [{}] 卸载 daemon：删除 {removed:?}",
        cfg.origin_label()
    );
    if removed.is_empty() {
        Ok(format!(
            "没有可删的 daemon 文件（{path} 及其 .build_id 都不在，可能已卸载）。"
        ))
    } else {
        Ok(format!(
            "已删除 {} 个文件：{}。注意：若本机器仍勾选「启用」，自动部署会在下次连接时把 daemon 装回——彻底移除请取消该机器启用 / 删除该机器后重启 monitor。",
            removed.len(),
            removed.join("、")
        ))
    }
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
        return Err(format!(
            "拒绝删除非法远端路径（须为 projects/ 下 .jsonl）: {remote_path}"
        ));
    }
    let conn = connect_sftp(cfg).await?;
    let sftp = &conn.sftp;
    // realpath 解析 symlink 后二次校验，防 projects/ 内 symlink 指向外部文件。
    let canon = sftp
        .canonicalize(remote_path.to_string())
        .await
        .map_err(|e| format!("解析远端路径失败: {e}"))?;
    if !is_safe_remote_jsonl(&canon) {
        return Err(format!(
            "拒绝删除：canonical 路径越出 projects/ 或非 jsonl: {canon}"
        ));
    }
    sftp.remove_file(canon.clone())
        .await
        .map_err(|e| format!("删除远端文件失败: {e}"))?;
    tracing::info!("远端 [{}] 已删除历史会话: {canon}", cfg.origin_label());
    Ok(())
}

// ============================================================================
// F10：远端 cc/bash 集成——一键把 ccm wrapper 装进远端 ~/.bashrc（SS-H）。
// 写 ~/.bashrc 不是 Claude 数据（不触 INVARIANT §1），与本地 PowerShell profile 安装同性质。
// ============================================================================

/// 远端 ccm 块的 BEGIN/END 标记（镜像本地 profile_installer 的 `# === cc-monitor BEGIN/END`）。
/// 重装时整块替换、卸载时整块删；用户在块外的内容绝不动。
const CCM_PROFILE_BEGIN: &str = "# === cc-monitor remote ccm BEGIN ===";
const CCM_PROFILE_END: &str = "# === cc-monitor remote ccm END ===";

/// 远端 ↗ 拉前用的 `ccm` wrapper（**后端拥有**，install 写它而非前端传入——见审计 S-1：
/// 写进 ~/.bashrc 的是被 shell **执行**的代码，绝不能让前端注入任意 bash）。
///
/// **必须与前端 `remote-section.ts::CCM_WRAPPER_SNIPPET`（面板展示/手动复制用）逐字一致。**
/// `\033`/`\007` 是 bash `printf` 的八进制转义（ESC/BEL），用 raw string 保留字面反斜杠。
/// 标记 `ccm-rbind-%s` 必须与 `bind.rs` 的 `format!("ccm-rbind-{sid}")` 一致。
const CCM_WRAPPER_SNIPPET: &str = r#"ccm() {
  ( cpid=$BASHPID
    ( prev=""
      while kill -0 "$cpid" 2>/dev/null; do
        sid=$(grep -o '"sessionId":"[^"]*"' ~/.claude/sessions/$cpid.json 2>/dev/null | head -1 | cut -d'"' -f4)
        [ -n "$sid" ] && [ "$sid" != "$prev" ] && { printf '\033]0;ccm-rbind-%s\007' "$sid"; prev="$sid"; }
        sleep 1
      done
    ) &
    exec claude "$@"
  )
}"#;

/// 纯函数：把 `snippet` 合进 profile 内容的 BEGIN/END 块（可单测）。
/// - 已有**配对**块（BEGIN 后能找到 END）→ **整块替换**（幂等：`merge(merge(x))==merge(x)`）。
/// - 无 BEGIN → **追加**（块外内容原样保留）。
/// - **有 BEGIN 但其后无 END（损坏/截断/上次安装中断）→ `Err` 中止**（审计 B1：绝不用独立
///   `find` 误配前面的 END 而吞掉用户内容；宁可报错让用户手修，也不破坏文件）。
pub fn merge_profile_block(existing: &str, snippet: &str) -> Result<String, String> {
    let block = format!(
        "{CCM_PROFILE_BEGIN}\n{}\n{CCM_PROFILE_END}\n",
        snippet.trim()
    );
    match existing.find(CCM_PROFILE_BEGIN) {
        Some(b) => {
            // 关键：找 BEGIN **之后**的 END（独立 find 会误配前面的 END → 吞内容）。
            match existing[b..].find(CCM_PROFILE_END) {
                Some(rel) => {
                    let e = b + rel;
                    let after = existing[e..]
                        .find('\n')
                        .map(|n| e + n + 1)
                        .unwrap_or(existing.len());
                    Ok(format!("{}{}{}", &existing[..b], block, &existing[after..]))
                }
                None => Err(
                    "远端 profile 里有 cc-monitor BEGIN 标记但缺对应的 END（可能被手动改坏 / \
                     上次安装中断）。为避免误删你的内容，已中止——请手动修好该文件后重试。"
                        .to_string(),
                ),
            }
        }
        None => {
            // 无块 → 追加（原内容不以换行结尾则补一个，保证块独占起行）。
            let mut out = existing.to_string();
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&block);
            Ok(out)
        }
    }
}

/// 纯函数：从 profile 内容删掉 cc-monitor 的 BEGIN/END 块（可单测）。
/// - 有**配对**块（BEGIN 后找得到 END）→ 整块删，块前后用户内容原样保留。
/// - 无 BEGIN，或 BEGIN 后无 END（损坏）→ **原样返回**（宁可不删也不破坏文件）。
pub fn strip_profile_block(existing: &str) -> String {
    let Some(b) = existing.find(CCM_PROFILE_BEGIN) else {
        return existing.to_string();
    };
    // 找 BEGIN **之后**的 END（同 merge：独立 find 会误配前面的 END）。
    let Some(rel) = existing[b..].find(CCM_PROFILE_END) else {
        return existing.to_string(); // 损坏块（BEGIN 无 END）→ 不动
    };
    let e = b + rel;
    let after = existing[e..]
        .find('\n')
        .map(|n| e + n + 1)
        .unwrap_or(existing.len());
    format!("{}{}", &existing[..b], &existing[after..])
}

/// 卸载远端 ccm 助手（设置面板「卸载 ccm」按钮）：从 profile 删 BEGIN/END 块。
/// 镜像 [`install_remote_ccm_helper`]：read → `strip_profile_block` → 无变化 no-op；否则
/// **先备份**（timestamped `.ccm-backup-<ms>`）→ 写 → **读回精确比对**，不符则回滚。
#[tauri::command]
pub async fn uninstall_remote_ccm_helper(
    cfg: RemoteConfig,
    profile: String,
) -> Result<String, String> {
    let profile = {
        let p = profile.trim();
        if p.is_empty() {
            ".bashrc".to_string()
        } else {
            p.to_string()
        }
    };
    if profile.contains('/') || profile.contains('\\') || profile.contains("..") {
        return Err("profile 只能是 home 下的文件名（如 .bashrc / .zshrc）".to_string());
    }

    let conn = connect_sftp(&cfg).await?;
    let sftp = &conn.sftp;

    let existing = read_optional(sftp, &profile)
        .await
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let stripped = strip_profile_block(&existing);
    if stripped == existing {
        return Ok(format!("远端 {profile} 里没有 ccm 块，无需卸载。"));
    }

    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let backup = format!("{profile}.ccm-backup-{ms}");
    upload_atomic(sftp, &backup, existing.as_bytes(), 0o600)
        .await
        .map_err(|e| format!("备份远端 {profile} 失败（未改动原文件）: {e}"))?;

    upload_atomic(sftp, &profile, stripped.as_bytes(), 0o644)
        .await
        .map_err(|e| format!("写远端 {profile} 失败: {e}"))?;

    let verify = read_optional(sftp, &profile)
        .await
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    if verify != stripped {
        let _ = upload_atomic(sftp, &profile, existing.as_bytes(), 0o644).await;
        return Err("写后校验失败（读回内容与期望不符），已尝试回滚原文件。".to_string());
    }

    tracing::info!("远端 [{}] 已卸载 ccm 助手（{profile}）", cfg.origin_label());
    Ok(format!(
        "已从远端 {profile} 删除 ccm 块（原文件已备份为 {backup}）。"
    ))
}

/// 一键把 `ccm` wrapper 装进远端 bash profile（F10，SS-H）。
///
/// `profile` 默认 `.bashrc`（SFTP 相对路径解析到 home；拒 `/`、`\`、`..` 防写 home 外）。
/// 写入的 snippet 是**后端拥有**的 [`CCM_WRAPPER_SNIPPET`]（审计 S-1：不接受前端传入可执行
/// bash）。安全范式镜像本地 `profile_installer`：read → `merge_profile_block`（损坏块 → Err
/// 中止，绝不吞内容）→ 相同则 no-op；否则**先备份**（timestamped `.ccm-backup-<ms>`）→ 写 →
/// **读回精确比对**（== merged，比仅查 BEGIN 强，兼防传输损坏）→ 失败**回滚**原文件。
///
/// 注：profile 统一写 `0o644`（.bashrc 惯例）；若用户原本 `chmod 600`，重装会归一到 644。
#[tauri::command]
pub async fn install_remote_ccm_helper(
    cfg: RemoteConfig,
    profile: String,
) -> Result<String, String> {
    let profile = {
        let p = profile.trim();
        if p.is_empty() {
            ".bashrc".to_string()
        } else {
            p.to_string()
        }
    };
    if profile.contains('/') || profile.contains('\\') || profile.contains("..") {
        return Err("profile 只能是 home 下的文件名（如 .bashrc / .zshrc）".to_string());
    }

    let conn = connect_sftp(&cfg).await?;
    let sftp = &conn.sftp;

    let existing = read_optional(sftp, &profile)
        .await
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    // 损坏块（BEGIN 无 END）→ merge 返回 Err，直接中止，不动原文件。
    let merged = merge_profile_block(&existing, CCM_WRAPPER_SNIPPET)?;
    if merged == existing {
        return Ok(format!("远端 {profile} 已是最新（ccm 块已在），无需改动。"));
    }

    // 备份原文件（非空才备份），失败则不动原文件直接返回。
    let mut backup_note = String::new();
    if !existing.is_empty() {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let backup = format!("{profile}.ccm-backup-{ms}");
        upload_atomic(sftp, &backup, existing.as_bytes(), 0o600)
            .await
            .map_err(|e| format!("备份远端 {profile} 失败（未改动原文件）: {e}"))?;
        backup_note = format!("（原文件已备份为 {backup}）");
    }

    upload_atomic(sftp, &profile, merged.as_bytes(), 0o644)
        .await
        .map_err(|e| format!("写远端 {profile} 失败: {e}"))?;

    // 读回**精确比对**：不等于期望内容（写坏 / 传输损坏）→ 回滚原文件。
    let verify = read_optional(sftp, &profile)
        .await
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    if verify != merged {
        if !existing.is_empty() {
            let _ = upload_atomic(sftp, &profile, existing.as_bytes(), 0o644).await;
        }
        return Err("写后校验失败（读回内容与期望不符），已尝试回滚原文件。".to_string());
    }

    tracing::info!("远端 [{}] 已装 ccm 助手到 {profile}", cfg.origin_label());
    Ok(format!(
        "已装到远端 {profile}{backup_note}。重连远端 ssh 终端后，用 `ccm` 代替 `claude` 启动即可让 ↗ 拉前生效。"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_profile_block_append_replace_idempotent() {
        let snippet = "ccm() { :; }";
        // 空 existing → 仅块。
        let m1 = merge_profile_block("", snippet).unwrap();
        assert!(m1.contains(CCM_PROFILE_BEGIN));
        assert!(m1.contains("ccm() { :; }"));
        assert!(m1.contains(CCM_PROFILE_END));

        // 无块 → 追加，原内容保留在前。
        let existing = "export PATH=/x\nalias ll='ls -l'\n";
        let m2 = merge_profile_block(existing, snippet).unwrap();
        assert!(m2.starts_with(existing), "块外内容保留在前");
        assert!(m2.contains(CCM_PROFILE_BEGIN));

        // 幂等：同 snippet 再 merge 不变。
        assert_eq!(
            merge_profile_block(&m2, snippet).unwrap(),
            m2,
            "merge∘merge == merge"
        );

        // 重装（换 snippet 内容）→ 整块替换，只有一个块，块外内容仍保留。
        let m3 = merge_profile_block(&m2, "ccm() { echo new; }").unwrap();
        assert!(m3.starts_with(existing), "重装仍保留块外内容");
        assert!(
            m3.contains("echo new") && !m3.contains("{ :; }"),
            "块被整块替换"
        );
        assert_eq!(m3.matches(CCM_PROFILE_BEGIN).count(), 1, "重装不重复加块");
    }

    /// 审计 B1 回归：块外内容（含块**后**的用户内容）在替换时绝不丢。
    #[test]
    fn merge_profile_block_preserves_content_after_block() {
        let existing =
            format!("head_line\n{CCM_PROFILE_BEGIN}\nold()\n{CCM_PROFILE_END}\ntail_user_line\n");
        let m = merge_profile_block(&existing, "ccm() { echo new; }").unwrap();
        assert!(m.contains("head_line"), "块前内容保留");
        assert!(
            m.contains("tail_user_line"),
            "块后用户内容保留（B1 不能吞掉）"
        );
        assert!(m.contains("echo new") && !m.contains("old()"), "块整块替换");
        assert_eq!(m.matches(CCM_PROFILE_BEGIN).count(), 1);
    }

    /// 审计 B1 核心：BEGIN 存在但其后无 END（损坏/截断）→ Err 中止，**绝不**误配前面的 END
    /// 而吞掉用户内容。
    #[test]
    fn merge_profile_block_aborts_on_orphan_begin() {
        // END 在前、孤立 BEGIN 在后无配对 END：独立 find 会误配 → 旧实现吞内容。新实现报错。
        let corrupt = format!("{CCM_PROFILE_END}\nuser_a\n{CCM_PROFILE_BEGIN}\nuser_b\n");
        assert!(
            merge_profile_block(&corrupt, "ccm() { :; }").is_err(),
            "孤立 BEGIN（其后无 END）必须中止而非吞内容"
        );
        // 纯孤立 BEGIN（截断的安装）→ Err。
        let truncated = format!("user_x\n{CCM_PROFILE_BEGIN}\nhalf");
        assert!(merge_profile_block(&truncated, "ccm() { :; }").is_err());
    }

    /// F08b：仅当交叉编译产物已放进 embedded-daemons/（build.rs 置了 `embedded_daemons` cfg）
    /// 才编译/运行——证实内嵌真生效：按 arch 取到 ELF 二进制 + build_id 非空。CI 无二进制时
    /// 本测试被 cfg 掉，不误报。
    #[cfg(embedded_daemons)]
    #[test]
    fn embedded_daemon_binaries_present_and_valid() {
        for arch in ["x86_64", "aarch64"] {
            let bin = daemon_binary(arch).expect("内嵌二进制应存在");
            assert!(!bin.build_id.is_empty(), "build_id 非空");
            assert_eq!(&bin.bytes[..4], b"\x7fELF", "{arch} 应是 ELF");
            assert!(bin.bytes.len() > 100_000, "{arch} 体积应非平凡");
        }
        assert!(daemon_binary("riscv64").is_none(), "未知 arch → None");
    }

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
        assert!(!is_safe_remote_jsonl(
            "/home/pi/.claude/projects/proj/note.txt"
        ));
        assert!(!is_safe_remote_jsonl(
            "/home/pi/.claude/projects/proj/abc.json"
        ));
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

    #[test]
    fn strip_removes_paired_block_keeps_surrounding() {
        let s = format!("head\n{CCM_PROFILE_BEGIN}\nccm() {{ :; }}\n{CCM_PROFILE_END}\ntail\n");
        let out = strip_profile_block(&s);
        assert_eq!(out, "head\ntail\n");
        assert!(!out.contains(CCM_PROFILE_BEGIN));
        // 幂等：再 strip 不变
        assert_eq!(strip_profile_block(&out), out);
    }

    #[test]
    fn strip_noop_when_no_block() {
        let s = "just user content\nno block here\n";
        assert_eq!(strip_profile_block(s), s);
    }

    #[test]
    fn strip_noop_on_malformed_begin_without_end() {
        // BEGIN 无配对 END（损坏）→ 原样返回，绝不吞内容（同 merge 的 B1 守卫精神）。
        let s = format!("user_a\n{CCM_PROFILE_BEGIN}\nhalf written, no end");
        assert_eq!(strip_profile_block(&s), s);
    }

    #[test]
    fn safe_daemon_path_accepts_convention_rejects_suspicious() {
        assert!(is_safe_remote_daemon_path(
            "/home/pi/.cc-monitor/bin/cc-monitor-remote"
        ));
        assert!(!is_safe_remote_daemon_path("")); // 空
        assert!(!is_safe_remote_daemon_path("relative/cc-monitor")); // 非绝对
        assert!(!is_safe_remote_daemon_path("/")); // 根
        assert!(!is_safe_remote_daemon_path("/etc/passwd")); // 不含 cc-monitor
        assert!(!is_safe_remote_daemon_path(
            "/home/pi/.cc-monitor/../../../etc/x"
        )); // 含 ..
    }
}
