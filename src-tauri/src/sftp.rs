//! SS-D：统一 SFTP 会话层（issue #29 自动部署 F08；F11 用户数据写 / F10 profile 写已复用）。
//!
//! 复用 `ssh_source::connect_session` 的全套 host-key 指纹校验 + publickey/agent 鉴权，
//! 在一条已鉴权的 russh 连接上 `request_subsystem("sftp")` 起 SFTP 子系统（russh-sftp，
//! transport-agnostic，吃 channel 的 AsyncRead+AsyncWrite 流）。
//!
//! ## 只读铁律豁免（INVARIANT §1 / 账本 SS-G）—— 穷举登记见 `doc/INVARIANTS.md §1`
//! cc-monitor 对远端的写入均**用户显式触发**，各自独立路径守卫、绝不混用：
//! - **F08**：自部署 daemon 二进制到 `~/.cc-monitor/bin/`（非用户数据、幂等、版本门控）。
//! - **F11**：用户**主动**删除远端会话 jsonl（`remove_remote_file`，`is_safe_remote_jsonl` + `canonicalize`）。
//! - **F89a**：用户**显式**增/改/删远端**项目** `.mcp.json`（`mcp::write_remote_mcp_server` 等，字符串守卫
//!   `is_safe_remote_mcp_json`：绝对 + 尾 `/.mcp.json` + 无 `..` + 非裸；经本模块 `upload_atomic` 原子写）。
//!   **SS-14**：写面**只** `.mcp.json`，非 Claude 会话数据。
//! - **F10**：cc(m) 助手装/卸——**本模块 `install_remote_ccm_helper`/`uninstall_remote_ccm_helper` 写远端 `~/.bashrc`**
//!   （BEGIN/END 块 + 备份 + 写后校验回滚）；本机 profile 写在 `profile_installer`。（batch20 审计修：原「非远端」措辞误——本模块确写远端 `~/.bashrc`。）
//! - **F50**：`pubkey::push_public_key` 经 SSH-exec 追加公钥到远端 `~/.ssh/authorized_keys`（不在本模块，登记于此备查）。
//!
//! `upload_atomic`（F89a 审计后加固）：tmp 用 **EXCLUDE** 创建（防 symlink 预置 clobber）+ 旧目标先备份到
//! `.bak` 再 rename（失败可恢复、成功即清），不留垃圾。
//!
//! ## 原子写
//! russh-sftp 无 `posix-rename@openssh.com` 扩展，标准 SFTP `rename` 不覆盖已存在目标。
//! 故 [`upload_atomic`] 用「写 `<path>.tmp` → 旧目标 **rename 成 `.bak`** → rename tmp→目标
//! → 清 `.bak`」近似原子（单写者、低频部署场景足够）。
//!
//! **是备份不是删除**：F89a 审计改的就是这一点 —— 「先删旧」一旦后续 rename 失败就**丢原件**，
//! 而先备份则最坏情况下原内容仍在 `.bak` 里。（本节此前仍写着「删旧」，2026-07-31 随 DN-7 一并订正。）

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
    let (session, _fp) = connect_session(cfg, None, None).await?;
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
/// 流程：写 `<remote_path>.tmp`（**EXCLUDE** 创建，防 symlink 预置 clobber）
/// → 旧目标 **rename 成 `.bak`（不是删掉）** → rename tmp→目标 → 成功后清 `.bak`。
/// 中途失败时旧内容仍在 `.bak` 里可恢复。
///
/// `mode` **只在 open-create 的 attrs 里设一次**。
///
/// ⚠ **rename 之后绝不 `set_metadata` 兜底 chmod** —— 真机 e2e 实证：OpenSSH sftp-server 上
/// setstat（即便只设 permissions、`size=None`）会把刚 rename 好的文件**截断成 0 字节**，
/// daemon 因此不可 exec → 连接 EOF → marker 变空 → 无限重部署。理由详见函数末尾那段注释。
///
/// （2026-07-31 修：本注释此前写的是「删旧 → rename」+「rename 后 set_metadata 兜底」，
/// **两句都与函数体相反**，而且照它实现正好复活上面那个把 daemon 变砖的 bug。
/// 由 aterm 侧交叉核对时发现〔DN-7〕。）
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
    // 安全（F89a 审计·重要）：先删可能残留/被预置的 tmp（remove_file 删链本身、不写穿 target），
    // 再用 **EXCLUDE**（SSH_FXF_EXCL）创建——若删后被抢先重放 symlink，EXCLUDE 令 open 失败而非跟随，
    // 杜绝「tmp 是 symlink → CREATE|TRUNCATE 跟随截断、越写到 `.mcp.json` 之外的用户文件」的 clobber 逃逸。
    let _ = sftp.remove_file(tmp.clone()).await; // best-effort 清残留/预置（不存在则忽略）
    let mut file = sftp
        .open_with_flags_and_attributes(
            tmp.clone(),
            OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
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

    // 数据安全（F89a 审计·重要）：russh-sftp `rename` 不覆盖 → 旧目标**先 rename 成 `.bak`（不 delete）**，
    // 再 rename tmp→目标；tmp→目标失败时旧内容仍在 `.bak`（可恢复），不像「先删旧」失败即丢原件。
    // 成功后即删 `.bak`（不留垃圾——大文件如 daemon 二进制不堆备份）。
    let bak = if sftp
        .try_exists(remote_path.to_string())
        .await
        .unwrap_or(false)
    {
        let b = format!("{remote_path}.bak");
        let _ = sftp.remove_file(b.clone()).await; // 清旧 bak（rename 不覆盖）
        sftp.rename(remote_path.to_string(), b.clone())
            .await
            .map_err(|e| format!("备份旧文件 {remote_path} → {b} 失败: {e}"))?;
        Some(b)
    } else {
        None
    };
    sftp.rename(tmp.clone(), remote_path.to_string())
        .await
        .map_err(|e| format!("rename {tmp} → {remote_path} 失败: {e}"))?;
    if let Some(b) = bak {
        let _ = sftp.remove_file(b).await; // 成功替换 → 清备份
    }
    // **绝不**在这里 `set_metadata(permissions)` 兜底 chmod —— 真机 e2e 诊断确证：在 OpenSSH
    // sftp-server 上 setstat（即便只设 permissions、size=None）会把刚 rename 好的文件**截断成
    // 0 字节**（tmp 写后 size 正确、rename 直后 size 正确，唯独 set_metadata 之后变 0）。daemon
    // 因此变 0 字节不可 exec → 连接 EOF，marker 变空 → 无限重部署。权限已在 open-create 的 attrs
    // 里设好（OpenSSH 按 SSH_FXP_OPEN attrs 建文件：0o700 可执行 / 0o600）、rename 保留权限，无需
    // 也不能再 set_metadata。
    Ok(())
}

/// 读远端文件，不存在 / 读失败 → None。
/// 判定一次远端上传的读回结果。**纯函数，可测**——远端往返塞不进单测，
/// 但"读回的字节该不该判通过"这条判据可以，而它正是此前完全缺失的那一环。
///
/// 按字节而不是按字符串：`deploy_remote_daemon` 上传的是**可执行二进制**，
/// `String::from_utf8` 会失败。这也是没直接复用 `verified_write::verify_readback`
/// （它是 `&str`）的原因——判据同源（逐字节相同才算通过），载体不同。
pub fn verify_uploaded_bytes(
    path: &str,
    expected: &[u8],
    actual: Option<&[u8]>,
) -> Result<(), String> {
    let Some(actual) = actual else {
        return Err(format!(
            "上传后读不回 {path}——无法确认写对了。已中止，未写入版本标记（下次会重新部署）。"
        ));
    };
    if actual == expected {
        return Ok(());
    }
    if actual.len() == expected.len() {
        let at = expected
            .iter()
            .zip(actual)
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        return Err(format!(
            "上传后校验失败：{path} 长度相同（{} 字节）但内容不同，首个差异在第 {at} 字节。\
             这类损坏（传输截断后补齐 / 编码变形）只比长度是查不出来的。\
             已中止，未写入版本标记（下次会重新部署）。",
            expected.len()
        ));
    }
    Err(format!(
        "上传后校验失败：{path} 长度不匹配（期望 {} 字节，读回 {} 字节）。\
         已中止，未写入版本标记（下次会重新部署）。",
        expected.len(),
        actual.len()
    ))
}

/// 上传 + **读回逐字节比对**。
///
/// ## 为什么这个函数此前不存在（T04 审计①）
///
/// `deploy_remote_daemon` 与 `deploy_remote_acct_iso` 的**全部** `upload_atomic`
/// ——1 个 daemon 可执行二进制 + 6 个远端脚本（含 0755 的 `cc-acct-iso` / `lib.sh` /
/// install.sh）——写完**直接写版本标记**，中间没有任何读回。`upload_atomic` 自己
/// 只做 flush/shutdown/rename，不读回（实测 `grep -c` = 0）。
///
/// 而 T04 第二步我论证「备份→写→读回比对→回滚这个范式已共享（5 处），所以不用抽」
/// ——**那 5 处全在 profile/CLI 那条线上，压根没覆盖这两条 deploy 路**。
/// 我那套"五套机制"框架恰好把这个洞盖住了：把"范式已共享"当成了"范式已覆盖"。
///
/// 后果具体：传输损坏的 daemon 二进制照样被写上正确的 `.build_id` 标记 →
/// 下次 `deploy_decision` 判「已是最新，跳过」→ **坏二进制永久驻留**，
/// 而用户看到的是部署成功。标记写在校验之后，就断了这条链。
pub(crate) async fn upload_atomic_verified(
    sftp: &SftpSession,
    remote_path: &str,
    bytes: &[u8],
    mode: u32,
) -> Result<(), String> {
    upload_atomic(sftp, remote_path, bytes, mode).await?;
    let back = read_optional(sftp, remote_path).await;
    verify_uploaded_bytes(remote_path, bytes, back.as_deref())
}

pub(crate) async fn read_optional(sftp: &SftpSession, path: &str) -> Option<Vec<u8>> {
    sftp.read(path.to_string()).await.ok()
}

/// **远端 profile 的读取结论**（Phase G 审阅修复）：把 `read_optional` 的 `Option<Vec<u8>>`
/// 拆成三态，取代原先的 `read_optional(..).map(from_utf8_lossy).unwrap_or_default()`。
///
/// 那一行有两个各自独立的数据丢失口，而**本机侧同一操作两个口都堵着**
/// （`profile_installer::install_to_profile`：`read_to_string` 遇非 UTF-8 直接 `Err`；
/// `on_disk_size > 0 && raw.is_empty()` 直接 `Err`，后者是 v1.7.9 事故的修法）：
///
/// 1. **`unwrap_or_default()` 把「读不出来」当成「文件是空的」**。于是 install 走
///    `if !existing.is_empty()` 时**跳过备份**、把用户整份 `.bashrc` 换成只含 ccm 块的
///    `merged`，无任何可恢复副本；uninstall 则 `stripped == existing` 成立 →
///    对着一份读不出来的文件回「没有 ccm 块，无需卸载」——正是
///    `strip_profile_block` 头注亲自定义为 bug 的形态（"它主动告诉用户没问题"），
///    上一轮只修到纯函数一层，根因在这个读取行。
/// 2. **`from_utf8_lossy` 在有损字符串空间里做读-改-写**。非 UTF-8 字节（GBK 注释、
///    latin-1 人名、误粘的 `\xa0`）变 U+FFFD → **备份写的是已经有损的那份**，原字节
///    从此不可恢复；而读回校验拿同样有损的两份比对，**逐字节相同、校验通过**，
///    整套「备份 + 读回 + 回滚」为这次损坏出具合格证。`verify_uploaded_bytes` 的头注
///    自己写着"按字节而不是按字符串"，那条纪律只落到了 daemon 二进制那条路。
///
/// 修法与本机侧对齐成 **fail-safe**：说不清就 `Err` 中止、不动原文件。
/// `Ok(None)` = 文件真的不存在（`try_exists` 明确说 false）；`Ok(Some(s))` = 读到了且是
/// 合法 UTF-8；`Err` = 读不出来 / 非 UTF-8 / 有字节数却读到空。
pub(crate) fn interpret_profile_read(
    what: &str,
    bytes: Option<&[u8]>,
    exists: Option<bool>,
    size: Option<u64>,
) -> Result<Option<String>, String> {
    let Some(bytes) = bytes else {
        // read 失败。只有 `try_exists` **明确说不存在**才当新建；"问不出来"归到 Err，
        // 因为把无权限/被占用当成空文件正是上面第 1 条的病灶。
        return if exists == Some(false) {
            Ok(None)
        } else {
            Err(format!(
                "读不出 {what}（文件可能存在但无权限 / 被占用 / 传输失败）。已取消，未改动任何文件。"
            ))
        };
    };
    if bytes.is_empty() {
        if let Some(n) = size.filter(|n| *n > 0) {
            return Err(format!(
                "{what} 在远端有 {n} 字节，但读回来是空的。已取消，未改动任何文件——\
                 继续走会用「空内容 + ccm 块」覆盖掉那 {n} 字节。"
            ));
        }
        return Ok(Some(String::new()));
    }
    String::from_utf8(bytes.to_vec()).map(Some).map_err(|e| {
        format!(
            "{what} 不是合法 UTF-8（前 {} 字节合法，之后不是）。ccm 块的合并/删除是按文本做的，\
             按有损文本写回会把这些字节永久换成 U+FFFD，连备份一起坏掉。已取消，未改动任何文件。",
            e.utf8_error().valid_up_to()
        )
    })
}

/// **回滚措辞必须与实际发生的事一致**（Phase G 审阅修复）：install 的两个失败分支都写
/// `if !existing.is_empty() { 回滚 }`，但错误文案是无条件的「已尝试回滚原文件」——
/// `existing` 为空（首次安装 / 原文件是空文件）时那个 `if` 一条也不执行，用户却被告知
/// 回滚过了，而一份校验不通过的 profile 正留在远端等着下次开终端时执行。
/// 这与两条阻塞是同一类病：**机制声称做了它没做的事**。
///
/// 本函数只负责措辞。真正的"首次安装失败就删掉新建的文件"是行为新增（要在远端 `remove`），
/// 已登记为未收项，不在验收轮里做。
pub(crate) fn rollback_note(existing_was_empty: bool) -> &'static str {
    if existing_was_empty {
        "原文件此前不存在或为空，没有可回滚的内容；刚写入的内容仍在远端，请手动清理后重试。"
    } else {
        "已尝试回滚原文件。"
    }
}

/// [`interpret_profile_read`] 的异步取样：read 成功就直接判，**只在需要时**才补问
/// `try_exists`（区分"真不存在"与"读不出来"）/ `metadata`（区分"真空文件"与"有字节读到空"），
/// 不为常见路径多加往返。
async fn read_profile_text(
    sftp: &SftpSession,
    path: &str,
    what: &str,
) -> Result<Option<String>, String> {
    let bytes = read_optional(sftp, path).await;
    let (exists, size) = match &bytes {
        Some(b) if b.is_empty() => (
            None,
            sftp.metadata(path.to_string())
                .await
                .ok()
                .and_then(|m| m.size),
        ),
        Some(_) => (None, None),
        None => (sftp.try_exists(path.to_string()).await.ok(), None),
    };
    interpret_profile_read(what, bytes.as_deref(), exists, size)
}

/// mkdir -p：逐级创建 `dir`（绝对或相对），已存在则跳过，创建失败容忍（并发/权限留给上传报错）。
pub(crate) async fn ensure_dir_all(sftp: &SftpSession, dir: &str) {
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
    /// Batch9：build_id 是否来自 .build_id 清单（true=字节真实身份可信；
    /// false=源码回退，需 bytes_contain 启发式兜底且可能误拒——见 daemon_binary doc）。
    pub id_from_manifest: bool,
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
/// 返回值（Batch7-F24）：`Ok(Some(build_id))` = 已**确认**远端 daemon 版本
/// （Deploy 成功或 Skip-版本相符）；`Ok(None)` = 无法确认（`~` 路径 / arch 探测失败 /
/// 无内嵌二进制等 no-op 路径——手动部署的 daemon，版本未知）。调用方据此决定
/// 是否传新版才认识的流模式参数（如 `--with-bg`）——未确认一律降级不传，
/// 避免旧 daemon 把未知参数当一次性查询处理后退出（无 hello 死循环）。
pub async fn ensure_daemon_deployed(cfg: &RemoteConfig) -> Result<Option<String>, String> {
    // S-2（审计）：SFTP 无 shell 不展开 `~`，而 daemon exec 路径会展开——daemon_path 含 `~`
    // 会两边错位。含 `~` 直接跳过自动部署（用户应填绝对路径），手动部署的 daemon 仍可连。
    if cfg.daemon_path.contains('~') {
        tracing::debug!(
            "daemon_path 含 ~（SFTP 不展开），跳过自动部署：{}",
            cfg.daemon_path
        );
        return Ok(None);
    }
    // 探测 arch 选内嵌二进制；探测失败 / 无该 arch 内嵌 → 优雅 no-op（沿用手动部署）。
    let arch = match probe_remote_arch(cfg).await {
        Ok(a) => a,
        Err(e) => {
            tracing::debug!("远端 arch 探测失败，跳过自动部署: {e}");
            return Ok(None);
        }
    };
    let Some(bin) = daemon_binary(&arch) else {
        tracing::debug!("无 {arch} 的内嵌 daemon 二进制（F08b 未嵌入该 arch?），跳过自动部署");
        return Ok(None);
    };
    // Batch8 stale 防御（Batch9 修订）：首选 .build_id 清单（bin.build_id 即
    // 字节真实身份，与源码期望的比对在 ssh_source 的 confirmed 判定处自然完成）。
    // 无清单（旧产物）时才用 bytes_contain 启发式兜底——注意它可能误拒正品
    // （Batch9 E2E 实证：编译器可把 BUILD_ID 优化成立即数、字节不连续），故仅
    // 在"启发式也找不到源码 id"且**无清单**时拒。
    if !bin.id_from_manifest && !bytes_contain(bin.bytes, bin.build_id.as_bytes()) {
        // 无清单（旧产物）且字节内搜不到源码 id → 无法确认字节身份
        tracing::warn!(
            "内嵌 daemon 无 .build_id 清单且字节内搜不到 {}——按身份未知跳过自动部署             （请在 embedded-daemons/ 旁写 <bin>.build_id 清单，或重跑 zigbuild）",
            bin.build_id
        );
        return Ok(None);
    }
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
            upload_atomic_verified(sftp, &cfg.daemon_path, bin.bytes, 0o700).await?;
            upload_atomic(sftp, &marker, bin.build_id.as_bytes(), 0o600).await?;
            tracing::info!(
                "远端 [{}] daemon 部署完成：{}",
                cfg.origin_label(),
                bin.build_id
            );
        }
    }
    Ok(Some(bin.build_id.to_string()))
}

/// 朴素子串搜索（8MB × 16B 一次性毫秒级；不为此引 memchr 依赖）。
fn bytes_contain(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// 按远端 arch 选内嵌的 daemon 二进制（F08b）。build.rs 把交叉编译的 musl 二进制复制进
/// OUT_DIR 并置 `embedded_daemons` cfg 时，这里 `include_bytes!` 内嵌并按 arch 返回；二进制
/// 未就位（无 cfg）→ 返回 None（ensure_daemon_deployed 优雅跳过，沿用手动部署）。
/// `build_id` 取编译期 env（来自 daemon 源码，SS-B 单源）。
pub fn daemon_binary(arch: &str) -> Option<&'static DaemonBinary> {
    #[cfg(embedded_daemons)]
    {
        // Batch9：build_id = 字节的**真实身份**——优先 .build_id 清单（构建时
        // 与二进制一并写入）；清单缺失（旧产物）→ 退回源码 id + 运行时
        // bytes_contain 启发式兜底（见 ensure_daemon_deployed）。
        // 身份与期望（EXPECTED_DAEMON_BUILD_ID = 源码）分离后：陈旧内嵌 =
        // 身份 p1f ≠ 期望 p1g → 部署照做（远端至少拿到 p1f）但 confirmed=p1f
        // → 降级不传新 flag——比"拒部署"更平滑且永不误拒正品。
        const fn pick(manifest: &'static str) -> &'static str {
            if manifest.is_empty() {
                env!("DAEMON_BUILD_ID")
            } else {
                manifest
            }
        }
        static X86: DaemonBinary = DaemonBinary {
            build_id: pick(env!("DAEMON_EMBEDDED_ID_X86_64")),
            id_from_manifest: !env!("DAEMON_EMBEDDED_ID_X86_64").is_empty(),
            bytes: include_bytes!(concat!(env!("OUT_DIR"), "/daemon-x86_64")),
        };
        static ARM: DaemonBinary = DaemonBinary {
            build_id: pick(env!("DAEMON_EMBEDDED_ID_AARCH64")),
            id_from_manifest: !env!("DAEMON_EMBEDDED_ID_AARCH64").is_empty(),
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

/// 远端受管路径的安全谓词。**T04 审计⑤：两个消费者、5 个条件里 4 个逐字相同，
/// 只差"必须含哪个标记词"** —— 正好达到我为 `fenced_block::find_pair` 立的 ≥2 门槛，
/// 所以按同一把尺子抽出来（`acct_iso_deploy::is_safe_remote_acct_iso_dir` 是第 2 个消费者）。
///
/// 判据：非空 · 绝对路径 · 不含 `..` · 不是根 · 含 `markers` 里任一标记词。
/// 最后一条是**防误删的关键**：它把"这是 cc-monitor 管的目录"变成路径本身的性质，
/// 而不是靠调用方记得。
pub(crate) fn is_safe_remote_managed_path(path: &str, markers: &[&str]) -> bool {
    let p = path.trim();
    !p.is_empty()
        && p.starts_with('/')
        && !p.contains("..")
        && p != "/"
        && markers.iter().any(|m| p.contains(m))
}

/// 远端 daemon 路径安全守卫（卸载用，纯函数可单测）：绝对、无 `..`、非根、且含 `cc-monitor`
/// （约定 `~/.cc-monitor/bin/cc-monitor-remote`）—— 杜绝把卸载误用成删任意远端文件。
fn is_safe_remote_daemon_path(path: &str) -> bool {
    is_safe_remote_managed_path(path, &["cc-monitor"])
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
            upload_atomic_verified(sftp, &path, bin.bytes, 0o700).await?;
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
/// **单一来源**：`shared/ccm-aliases.sh`——前端 `remote-section.ts` 经 `?raw` import
/// 同一文件（修复历史漂移：Batch7 重构时只改了前端展示版，装进远端的还是老版）。
///
/// **F02 起本块只剩「别名层」**：唯一实现搬进 [`CCM_CLI_SCRIPT`]（部署为可执行文件）。
/// 理由：shell 函数**优先于 PATH**，装成函数则与用户已有同名函数硬冲突且必然被遮蔽（实测）；
/// 且远端是 zsh/fish 时 `.bashrc` 根本不被 source，函数形态拿不到（审计 D2）。
const CCM_WRAPPER_SNIPPET: &str = include_str!("../../shared/ccm-aliases.sh");

/// F02：统一启动 CLI 本体，部署为远端 `~/.local/bin/ccm`（0755 可执行文件）。
/// 它独占 L1 容器 / L2 环境 / L5 身份的实现——**环境必须在最终 exec 的那个 shell 里设**，
/// 否则会像旧 `cct` 那样被 tmux 的进程边界吃掉（`update-environment` 默认列表不含
/// `CLAUDE_CONFIG_DIR`，实测有对照组：`e2e/ccm-acceptance.sh`）。
pub(crate) const CCM_CLI_SCRIPT: &str = include_str!("../../shared/ccm");

/// CLI 在远端的落点（SFTP 相对路径 = home 相对）。
const CCM_CLI_REMOTE_PATH: &str = ".local/bin/ccm";

/// 纯函数：把 `snippet` 合进 profile 内容的 BEGIN/END 块（可单测）。
/// - 已有**配对**块（BEGIN 后能找到 END）→ **整块替换**（幂等：`merge(merge(x))==merge(x)`）。
/// - 无 BEGIN → **追加**（块外内容原样保留）。
/// - **有 BEGIN 但其后无 END（损坏/截断/上次安装中断）→ `Err` 中止**（审计 B1：绝不用独立
///   `find` 误配前面的 END 而吞掉用户内容；宁可报错让用户手修，也不破坏文件）。
pub fn merge_profile_block(existing: &str, snippet: &str, what: &str) -> Result<String, String> {
    // **T04 第二步：配对判定改走 `fenced_block::find_pair`，与本机 profile 共用同一条规则。**
    //
    // **更正我原话「判定本身是对的…判定没变」——被实测证伪，9 个边界里 3 个变了**
    // （T04 审计②，它把旧 byte-find 实现逐字复制成 `old_merge` 并列对拍）：
    //   1. **行内 marker**（用户 profile 里有 `echo "…BEGIN…"` / `echo "…END…"`）：
    //      旧实现会**切断那个 echo 行、并把第二个 echo 行整行吃掉** —— 远端侧一个
    //      **我未申报就修掉了的数据丢失**。新实现按行 `trim_start().starts_with` 判，改成追加。
    //   2. **BEGIN 与 END 同一行**：旧能正确替换该行 → 新直接 Err（`find_pair` 认到 BEGIN
    //      就 `continue`，同行的 END 被跳过）。**这是退化**，虽符合"宁可报错"但当时未文档化未测试。
    //   3. **缩进 marker**：旧"保留 BEGIN 行缩进、丢 END 缩进"（不自洽）→ 新统一归一到列 0。
    // 三条现在都有测试锁死（见 `remote_merge_boundary_semantics_after_migration`）。
    //
    // 原实现是自己 `find(BEGIN)` 再在其后 `find(END)`——
    // 但本机侧漏了同一道保护，于是两侧对"围栏损坏"处置不一致、本机那边会**吃掉用户内容**。
    // 现在两侧同一个函数，判定不可能再漂移。
    let block = format!(
        "{CCM_PROFILE_BEGIN}\n{}\n{CCM_PROFILE_END}\n",
        snippet.trim()
    );
    match crate::fenced_block::find_pair(existing, CCM_PROFILE_BEGIN, CCM_PROFILE_END, what)? {
        Some((begin_line, end_line)) => {
            // 行下标 → 字节切片：`split_inclusive('\n')` 与 `.lines()` 索引一致
            let lines: Vec<&str> = existing.split_inclusive('\n').collect();
            let before: String = lines[..begin_line].concat();
            let after: String = if end_line + 1 < lines.len() {
                lines[(end_line + 1)..].concat()
            } else {
                String::new()
            };
            Ok(format!("{before}{block}{after}"))
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
pub fn strip_profile_block(existing: &str, what: &str) -> Result<String, String> {
    // **T04 审计阻塞：这里原先没迁移，于是「卸」那半边被我从"两侧一致"改成了"两侧不一致"。**
    // 原实现在悬空 BEGIN 时 `return existing.to_string()` → 调用方判 `stripped == existing`
    // → 打印「远端 {profile} 里没有 ccm 块，无需卸载」。**那正是我在同一个 commit 里
    // 定义为 bug 的形态**，而且比本机那边更糟：它主动告诉用户"没问题"。
    //
    // 更要紧的是这是我**新造的漂移**：`af21ffb~1` 时两侧卸载都"原样返回"（一致），
    // `af21ffb` 之后本机 Err、远端静默 no-op（不一致）。我 commit 里那句
    // 「两侧不可能再漂移」**只对 install 半边成立，对 uninstall 半边方向相反**。
    // 现在两侧的装与卸四条路全走 `find_pair`。
    let Some((begin_line, end_line)) =
        crate::fenced_block::find_pair(existing, CCM_PROFILE_BEGIN, CCM_PROFILE_END, what)?
    else {
        return Ok(existing.to_string());
    };
    let lines: Vec<&str> = existing.split_inclusive('\n').collect();
    let before: String = lines[..begin_line].concat();
    let after: String = if end_line + 1 < lines.len() {
        lines[(end_line + 1)..].concat()
    } else {
        String::new()
    };
    Ok(format!("{before}{after}"))
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

    let what = format!("远端 ~/{profile}");
    // fail-safe 读取（见 `interpret_profile_read`）：读不出来 → `?` 中止，**不再**回
    // 「没有 ccm 块，无需卸载」。
    let Some(existing) = read_profile_text(sftp, &profile, &what).await? else {
        return Ok(format!("远端 {profile} 不存在，没有 ccm 块可卸载。"));
    };
    let stripped = strip_profile_block(&existing, &what)?;
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

    // T01：判定走统一的 `verify_readback`（与本机侧同一套语义与措辞）。
    // **「读不回来」与「内容不符」要分开报**：原先 `unwrap_or_default()` 把读失败变成空串，
    // 于是 SSH 抖一下会被报成"内容与期望不符"，把用户往错误方向引。
    let verify = read_optional(sftp, &profile)
        .await
        .map(|b| String::from_utf8_lossy(&b).into_owned());
    let Some(verify) = verify else {
        let _ = upload_atomic(sftp, &profile, existing.as_bytes(), 0o644).await;
        return Err(format!(
            "写后读不回 {profile}（无法确认写对了），已尝试回滚原文件。"
        ));
    };
    if let crate::verified_write::WriteVerdict::Mismatch { detail } =
        crate::verified_write::verify_readback(&stripped, &verify)
    {
        let _ = upload_atomic(sftp, &profile, existing.as_bytes(), 0o644).await;
        return Err(format!("写后校验失败：{detail} 已尝试回滚原文件。"));
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

    // ① 先部署 CLI 本体（0755 可执行文件）。**先于写 profile**——别名块引用 `ccm`，
    //    若先写块再部署失败，用户会拿到一堆指向不存在命令的别名。
    //    逐级建目录（相对 home；`ensure_dir_all` 走绝对路径，这里用相对，故手动逐级）。
    //    已存在时 create_dir 失败——容忍，真正的失败由下面的 upload 报出来。
    {
        let mut cur = String::new();
        for comp in CCM_CLI_REMOTE_PATH
            .split('/')
            .filter(|c| !c.is_empty())
            .take(
                CCM_CLI_REMOTE_PATH
                    .split('/')
                    .filter(|c| !c.is_empty())
                    .count()
                    - 1,
            )
        {
            if !cur.is_empty() {
                cur.push('/');
            }
            cur.push_str(comp);
            let _ = sftp.create_dir(cur.clone()).await;
        }
    }
    upload_atomic(sftp, CCM_CLI_REMOTE_PATH, CCM_CLI_SCRIPT.as_bytes(), 0o755)
        .await
        .map_err(|e| format!("部署 ccm CLI 到远端 ~/{CCM_CLI_REMOTE_PATH} 失败: {e}"))?;
    // 读回精确比对（兼防传输损坏）——CLI 是可执行文件，写坏比 profile 写坏更危险。
    let cli_back = read_optional(sftp, CCM_CLI_REMOTE_PATH)
        .await
        .map(|b| String::from_utf8_lossy(&b).into_owned());
    let Some(cli_back) = cli_back else {
        return Err(format!(
            "写后读不回 ~/{CCM_CLI_REMOTE_PATH}（无法确认写对了）。未改动 {profile}。"
        ));
    };
    // **登记未改**（T01 §5 P5）：这一处**不回滚**，而另两处回滚。原因是部署前没有取旧 CLI 的
    // 备份，想回滚得先多一次读往返。留着不动是因为：加备份是这条部署路径上的行为变更，
    // 而它被 12 条 print-parity + 15 条 acceptance 真机断言盯着，风险/收益不划算。
    // 但要如实说清后果——见下方错误措辞：**损坏的 CLI 会留在远端**。
    if let crate::verified_write::WriteVerdict::Mismatch { detail } =
        crate::verified_write::verify_readback(CCM_CLI_SCRIPT, &cli_back)
    {
        return Err(format!(
            "ccm CLI 写后校验失败：{detail} 未改动 {profile}；\
             但 ~/{CCM_CLI_REMOTE_PATH} 已被写入且内容不对，请手动删除或重新部署。"
        ));
    }

    // ② 再把别名块合进 profile。
    let what = format!("远端 ~/{profile}");
    // fail-safe 读取（见 `interpret_profile_read`）：读不出来 → `?` 中止。**这一处最要紧**——
    // 原先读失败会变成 `existing = ""`，于是下方 `if !existing.is_empty()` 跳过备份、
    // `merged`（= 只有 ccm 块）整份覆盖用户 `.bashrc`，无任何可恢复副本。
    let existing = read_profile_text(sftp, &profile, &what)
        .await?
        .unwrap_or_default();
    // 损坏块（BEGIN 无 END）→ merge 返回 Err，直接中止，不动原文件。
    let merged = merge_profile_block(&existing, CCM_WRAPPER_SNIPPET, &what)?;
    if merged == existing {
        return Ok(format!(
            "ccm CLI 已部署到远端 ~/{CCM_CLI_REMOTE_PATH}；{profile} 的别名块已是最新，无需改动。"
        ));
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

    // 读回比对：不等于期望内容（写坏 / 传输损坏）→ 回滚原文件。判定同上走 `verify_readback`。
    let verify = read_optional(sftp, &profile)
        .await
        .map(|b| String::from_utf8_lossy(&b).into_owned());
    let Some(verify) = verify else {
        if !existing.is_empty() {
            let _ = upload_atomic(sftp, &profile, existing.as_bytes(), 0o644).await;
        }
        return Err(format!(
            "写后读不回 {profile}（无法确认写对了）。{}",
            rollback_note(existing.is_empty())
        ));
    };
    if let crate::verified_write::WriteVerdict::Mismatch { detail } =
        crate::verified_write::verify_readback(&merged, &verify)
    {
        if !existing.is_empty() {
            let _ = upload_atomic(sftp, &profile, existing.as_bytes(), 0o644).await;
        }
        return Err(format!(
            "写后校验失败：{detail} {}",
            rollback_note(existing.is_empty())
        ));
    }

    tracing::info!(
        "远端 [{}] 已装 ccm CLI + 别名块到 {profile}",
        cfg.origin_label()
    );
    Ok(format!(
        "已部署 ccm CLI 到 ~/{CCM_CLI_REMOTE_PATH}，别名块已写入 {profile}{backup_note}。\
         重连远端 ssh 终端后可用：`ccm`（起会话）/ `ccm --tmux`（tmux 里起）/ \
         `ccm --account <名>`（指定账号）。`ccm --help` 看全部修饰。"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 单一来源漂移守卫①：写进远端 profile 的**别名块**。
    /// F02 起本块只剩组合层别名——实现搬进 `CCM_CLI_SCRIPT`（见守卫②）。
    #[test]
    fn ccm_aliases_snippet_has_required_elements() {
        for needle in [
            ".local/bin", // CLI 落点必须进 PATH，否则别名全指向不存在的命令
            "cc()",       // 智能选目录
            "cct()",      // tmux 版
            "ccm --tmux", // 别名只做组合，不自己建容器
            "declare -f", // 防覆盖用户已有同名函数
        ] {
            assert!(
                CCM_WRAPPER_SNIPPET.contains(needle),
                "别名块缺关键要素: {needle}"
            );
        }
        // 别名块**不得**再含实现（那是 CLI 的事；混回来就又变成两套实现）。
        for forbidden in ["__ccm_rbind()", "exec claude", "tmux new-session"] {
            assert!(
                !CCM_WRAPPER_SNIPPET.contains(forbidden),
                "别名块不该含实现细节 {forbidden}——实现属于 ~/.local/bin/ccm"
            );
        }
    }

    /// 单一来源漂移守卫②：部署为远端 `~/.local/bin/ccm` 的 **CLI 本体**。
    ///
    /// 这些不是"要素清单"而是**血的教训清单**，每条对应一个真实踩过的坑：
    ///  - `=%s:` / `=$` ：tmux `-t` 必须精确匹配（INVARIANTS §31a）。裸目标会杀错/打错兄弟会话；
    ///    `=名` 无尾冒号则在 send-keys/capture-pane/set-option 上 rc=1 完全失效。
    ///  - `exec` ：不能省——身份 poller 读 `sessions/$PID.json`，不 exec 则 PID 对不上。
    ///  - `@ccm_sid` / `@ccm_agent` ：身份随行，cc-monitor 靠它精确认会话。
    ///  - `@ccm_sid_expect` ：F04——通道A（建时/exec 时立即声明"打算跑这个 sid"）写这个 key，
    ///    与通道B（poller 独立读会话文件确认后才写的 `@ccm_sid`）分离。破坏性动作只认 `@ccm_sid`，
    ///    不被"声明了但从未真正跑起来"的会话骗过（旧审计 D6 的坑）。
    ///
    ///    **R09 复核订正（2026-07-28）——这条分离的作用域是「`shared/ccm` 内部」，不是全仓。**
    ///    此前多处写作"通道B 是 `@ccm_sid` 唯一写者"，那句话不准确：全仓有**两个**写者。
    ///    另一个是 `src/session-backend.ts::TMUX_BACKEND.createRunAttach`——**兜底渲染器**
    ///    自己拼 tmux 命令时，在 create 分支**直写裸 `@ccm_sid`**。
    ///
    ///    那不是漏改，是 F04 Phase B 方案A 的明确取舍：兜底路径**没有 poller**，
    ///    因而没有"意图→事实"的提升机制；若那里改写 `@ccm_sid_expect`，这个 key 将
    ///    **永远不会被提升**，于是 Gate 2 的 `@ccm_sid` 半支永久判不出它 → 该会话变得不可 kill
    ///    （向后兼容回归，正是 §5.1 第 3 条要防的）。所以两侧**故意写不同的 key**。
    ///
    ///    两个方向相反的断言各自被钉住，别把任一侧"改成一致"：
    ///      · 本函数下方的 needle 扫描：`shared/ccm` **必须**写 `@ccm_sid_expect`；
    ///      · `src/session-backend.test.ts`（"#72 + F03.4甲′"那条黄金串）：兜底渲染器
    ///        **必须**写裸 `@ccm_sid`。已实测：把兜底侧改成 `_expect` 会让后者转红。
    ///    **成功标准④ 不受此例外影响**——终端起会话那条路径全程在 `shared/ccm` 内
    ///    （写 expect、poller 提升），与兜底渲染器无交集。
    ///  - `CLAUDE_CONFIG_DIR` ：账号注入必须在**最终 exec 的那个 shell 里**设。
    ///  - `--print` / `--ccm-probe` ：F03 的渲染等价断言 + 安装自检/降级判据依赖它们。
    #[test]
    fn ccm_cli_has_required_elements() {
        // U1a（2026-08-01）：三张表 + `-t` 扫描口径搬进 `crate::ccm_cli_contract`。
        // **判据一条没改、没加、没减** —— 搬出去只是为了让 U9 迁到 `control/` 时
        // 改的是「喂哪份脚本文本」，而不是把这些断言重写一遍（账本 S11：迁移是强度
        // 悄悄下降的经典时机）。强度读数的基线对拍在那个模块的
        // `ccm_cli_strength_is_at_or_above_baseline`。
        use crate::ccm_cli_contract as contract;

        for needle in contract::REQUIRED_NEEDLES {
            assert!(
                CCM_CLI_SCRIPT.contains(needle),
                "ccm CLI 缺关键要素: {needle}"
            );
        }
        for needle in contract::CHANNEL_A_LITERALS {
            assert!(
                CCM_CLI_SCRIPT.contains(needle),
                "通道A（意图声明）必须写 @ccm_sid_expect（而非裸 @ccm_sid），缺: {needle}"
            );
        }
        // 钉死逃生口。**除「逐字存在」还要断言只被赋值一次**（T01 审计 S3，已独立复现：
        // 在它后面再加一行 `t="$tmux_name"`，旧的 contains 版本照样通过而 `$t` 已成裸值）。
        contract::pin_t_def(CCM_CLI_SCRIPT)
            .expect("$t 的定义被改动或被二次赋值 —— 它是 tmux 序列里所有 -t 的来源");

        // tmux 目标精确形态（INVARIANTS §31a）：**结构性扫描**——扫出 CLI 里每一个 `-t ` 的
        // 目标 token，逐个断言含 `=` 且以 `:`（或 `:` + 引号）收尾。
        //
        // 刻意不用固定 needle：D 审计实测过，固定 needle 版本是**空转的**——把 CLI 里的
        // `=名:` 全改回裸目标，`cargo test` 依旧全绿（正向 needle 恰好都还命中，反向 needle
        // 引用的是 CLI 里根本不存在的代码）。而这正是 F01 修掉的「杀错/打错兄弟会话」生产事故。
        // 结构性扫描对**新增**的 `-t` 也自动生效，这是固定 needle 永远做不到的。
        //
        // ⚠ **阈值余量已经没了**（U1a 实测订正）：本注释此前写「真实脚本 checked=11 ……
        // 往下留 1 的余量以免正常增删命令时误红」，而实测 checked = **10** == 阈值。
        // 追溯到 `666cc14`（无名 `--tmux` 改为无条件新建会话）：删两处 `display-message -p -t`、
        // 加一处 `has-session -t`，净 −1，是正当的行为变更。**不下调阈值** —— 下调等于把
        // 「少一处 tmux 命令」重新变成无声的。读数本身由 `ccm_cli_contract::BASELINE` 单独盯着。
        contract::scan_t_targets(CCM_CLI_SCRIPT)
            .require(
                contract::MIN_CHECKED_T_TARGETS,
                "CLI 的 tmux 目标（INVARIANTS §31a）",
            )
            .expect("结构性扫描不通过");
    }

    #[test]
    fn merge_profile_block_append_replace_idempotent() {
        let snippet = "ccm() { :; }";
        // 空 existing → 仅块。
        let m1 = merge_profile_block("", snippet, "远端 ~/.bashrc").unwrap();
        assert!(m1.contains(CCM_PROFILE_BEGIN));
        assert!(m1.contains("ccm() { :; }"));
        assert!(m1.contains(CCM_PROFILE_END));

        // 无块 → 追加，原内容保留在前。
        let existing = "export PATH=/x\nalias ll='ls -l'\n";
        let m2 = merge_profile_block(existing, snippet, "远端 ~/.bashrc").unwrap();
        assert!(m2.starts_with(existing), "块外内容保留在前");
        assert!(m2.contains(CCM_PROFILE_BEGIN));

        // 幂等：同 snippet 再 merge 不变。
        assert_eq!(
            merge_profile_block(&m2, snippet, "远端 ~/.bashrc").unwrap(),
            m2,
            "merge∘merge == merge"
        );

        // 重装（换 snippet 内容）→ 整块替换，只有一个块，块外内容仍保留。
        let m3 = merge_profile_block(&m2, "ccm() { echo new; }", "远端 ~/.bashrc").unwrap();
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
        let m = merge_profile_block(&existing, "ccm() { echo new; }", "远端 ~/.bashrc").unwrap();
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
            merge_profile_block(&corrupt, "ccm() { :; }", "远端 ~/.bashrc").is_err(),
            "孤立 BEGIN（其后无 END）必须中止而非吞内容"
        );
        // 纯孤立 BEGIN（截断的安装）→ Err。
        let truncated = format!("user_x\n{CCM_PROFILE_BEGIN}\nhalf");
        assert!(merge_profile_block(&truncated, "ccm() { :; }", "远端 ~/.bashrc").is_err());
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
        let out = strip_profile_block(&s, "远端 ~/.bashrc").unwrap();
        assert_eq!(out, "head\ntail\n");
        assert!(!out.contains(CCM_PROFILE_BEGIN));
        // 幂等：再 strip 不变
        assert_eq!(strip_profile_block(&out, "远端 ~/.bashrc").unwrap(), out);
    }

    #[test]
    fn strip_noop_when_no_block() {
        let s = "just user content\nno block here\n";
        assert_eq!(strip_profile_block(s, "远端 ~/.bashrc").unwrap(), s);
    }

    /// **T04 审计⑤**：抽出来的谓词要对两个消费者都成立，且**标记词是必需条件**
    /// ——那是防误删的关键（把"这是 cc-monitor 管的目录"变成路径本身的性质）。
    #[test]
    fn safe_managed_path_requires_a_marker() {
        // 四条通用条件
        for bad in ["", "  ", "relative/x", "/a/../b/cc-monitor", "/"] {
            assert!(
                !is_safe_remote_managed_path(bad, &["cc-monitor"]),
                "{bad:?} 不该通过"
            );
        }
        // **没有标记词一律不通过**——哪怕是个完全正常的绝对路径
        assert!(!is_safe_remote_managed_path(
            "/home/u/.local/bin/x",
            &["cc-monitor"]
        ));
        assert!(is_safe_remote_managed_path(
            "/home/u/.cc-monitor/d",
            &["cc-monitor"]
        ));
        // 多标记词：任一命中即可（acct-iso 就是两个）
        let m = &["cc-acct-iso", ".cc-monitor"];
        assert!(is_safe_remote_managed_path("/opt/cc-acct-iso", m));
        assert!(is_safe_remote_managed_path("/home/u/.cc-monitor/ai", m));
        assert!(!is_safe_remote_managed_path("/opt/other", m));
    }

    /// **T04 审计②：迁移后远端这三个边界的语义确实变了，逐条锁死。**
    /// 我原话"判定没变"已被实测证伪——写在这里免得下次又当成"没变"。
    #[test]
    fn remote_merge_boundary_semantics_after_migration() {
        let snip = "ccm() { :; }";
        // ① 行内 marker 不再命中 → 追加，且**用户那两行 echo 一个字节都不动**
        //    （旧实现会切断第一行、吃掉第二行——远端一个未申报就修掉的数据丢失）
        let inline =
            format!("a\necho \"{CCM_PROFILE_BEGIN}\"\necho \"{CCM_PROFILE_END}\"\nuser code\n");
        let got = merge_profile_block(&inline, snip, "远端 ~/.bashrc").unwrap();
        assert!(got.starts_with(&inline), "块外内容必须逐字保留：{got}");
        assert!(got.contains(snip));
        // ② BEGIN 与 END 同一行 → 现在 Err（**退化，如实记**：旧实现能替换该行）
        let same_line = format!("a\n{CCM_PROFILE_BEGIN} {CCM_PROFILE_END}\nb\n");
        let e = merge_profile_block(&same_line, snip, "远端 ~/.bashrc").unwrap_err();
        assert!(e.contains("找不到配对的 END"), "{e}");
        // ③ 缩进 marker → 归一到列 0（旧实现保留 BEGIN 缩进、丢 END 缩进，不自洽）
        let indented = format!("a\n  {CCM_PROFILE_BEGIN}\nold\n\t{CCM_PROFILE_END}\nb\n");
        let got = merge_profile_block(&indented, snip, "远端 ~/.bashrc").unwrap();
        assert!(
            got.contains(&format!("\n{CCM_PROFILE_BEGIN}\n")),
            "缩进应归一到列 0：{got}"
        );
        assert!(
            got.starts_with("a\n") && got.ends_with("b\n"),
            "块外保留：{got}"
        );
    }

    // ===== T04 审计① 上传读回判据（此前这条路完全没有读回）=====

    #[test]
    fn upload_verify_catches_same_length_corruption() {
        let want = b"#!/bin/sh\nexec ccm \"$@\"\n";
        // 等长但一字节不同——**只比长度是查不出来的**，而此前连长度都没比
        let mut bad = want.to_vec();
        let k = bad.len() / 2;
        bad[k] ^= 0x01;
        let e = verify_uploaded_bytes("/r/x", want, Some(&bad)).unwrap_err();
        assert!(e.contains("长度相同"), "{e}");
        assert!(e.contains("首个差异在第"), "要指出位置：{e}");
        // **关键**：措辞必须说清标记没写，否则用户不知道下次会重试
        assert!(e.contains("未写入版本标记"), "{e}");
    }

    #[test]
    fn upload_verify_catches_truncation_and_unreadable() {
        let want = b"0123456789";
        let e = verify_uploaded_bytes("/r/x", want, Some(b"01234")).unwrap_err();
        assert!(e.contains("长度不匹配"), "{e}");
        assert!(e.contains("期望 10 字节"), "{e}");
        // 读不回来 ≠ 写对了
        let e2 = verify_uploaded_bytes("/r/x", want, None).unwrap_err();
        assert!(e2.contains("读不回"), "{e2}");
        assert!(e2.contains("未写入版本标记"), "{e2}");
    }

    #[test]
    fn upload_verify_passes_on_exact_bytes() {
        // 二进制（含 NUL 与非 UTF-8）也要过——daemon 是可执行文件，String 路线走不通
        let bin = &[0x7f, b'E', b'L', b'F', 0x00, 0xff, 0xfe];
        assert!(verify_uploaded_bytes("/r/d", bin, Some(bin)).is_ok());
        assert!(verify_uploaded_bytes("/r/d", b"", Some(b"")).is_ok());
    }

    /// Phase G 阻塞①：**「读不出来」绝不能变成「文件是空的」**。
    ///
    /// 旧代码是 `read_optional(..).map(from_utf8_lossy).unwrap_or_default()`，
    /// 读失败 → `existing = ""` → install 跳过备份 + 整份覆盖用户 `.bashrc`；
    /// uninstall 回「没有 ccm 块，无需卸载」。
    #[test]
    fn read_failure_is_not_an_empty_file() {
        // 读失败 + 明确不存在 → 当新建（这条是**反向自检**：不能一律 Err，否则首次安装就废了）
        assert_eq!(
            interpret_profile_read("远端 ~/.bashrc", None, Some(false), None),
            Ok(None)
        );
        // 读失败 + 文件确实在 → 必须 Err
        let e = interpret_profile_read("远端 ~/.bashrc", None, Some(true), None).unwrap_err();
        assert!(e.contains("读不出"), "{e}");
        assert!(e.contains("未改动任何文件"), "{e}");
        // 读失败 + 连"在不在"都问不出来 → 也必须 Err（不许乐观当新建）
        let e2 = interpret_profile_read("远端 ~/.bashrc", None, None, None).unwrap_err();
        assert!(e2.contains("读不出"), "{e2}");
    }

    /// Phase G 阻塞②：**非 UTF-8 的 profile 必须拒绝，不许有损重写**。
    ///
    /// 有损路线的恶性在于它**自带合格证**：备份写的是已经变成 U+FFFD 的那份，
    /// 读回校验两边同样有损 → 逐字节相同 → 校验通过。所以这里断言的是"根本不进那条路"。
    #[test]
    fn non_utf8_profile_is_refused_instead_of_lossily_rewritten() {
        // GBK 的「中」= 0xD6 0xD0，单独出现不是合法 UTF-8
        let gbk = b"# \xd6\xd0\xce\xc4\nexport PATH=$PATH\n";
        let e = interpret_profile_read("远端 ~/.bashrc", Some(gbk), None, None).unwrap_err();
        assert!(e.contains("不是合法 UTF-8"), "{e}");
        assert!(e.contains("前 2 字节合法"), "偏移要说清，实得：{e}");
        assert!(e.contains("未改动任何文件"), "{e}");
        // 有损重写会把它变成什么——写在这里，好让人一眼看到丢了什么
        assert_ne!(
            String::from_utf8_lossy(gbk).into_owned().as_bytes(),
            gbk,
            "这条测试的前提没了：这串本来就该是有损的"
        );

        // **反向自检**：合法的多字节 UTF-8（中文注释）必须原样通过、往返零损失
        let utf8 = "# 中文注释\nexport PATH=$PATH\n";
        assert_eq!(
            interpret_profile_read("远端 ~/.bashrc", Some(utf8.as_bytes()), None, None),
            Ok(Some(utf8.to_string()))
        );
    }

    /// Phase G：本机侧 v1.7.9 的那道防线（磁盘有字节却读到空）补到远端侧。
    #[test]
    fn bytes_on_disk_but_read_empty_is_refused() {
        let e = interpret_profile_read("远端 ~/.bashrc", Some(b""), None, Some(120)).unwrap_err();
        assert!(e.contains("有 120 字节"), "{e}");
        assert!(e.contains("未改动任何文件"), "{e}");
        // 反向自检：真的空文件（size 0 / 问不到 size）不能被拦
        assert_eq!(
            interpret_profile_read("远端 ~/.bashrc", Some(b""), None, Some(0)),
            Ok(Some(String::new()))
        );
        assert_eq!(
            interpret_profile_read("远端 ~/.bashrc", Some(b""), None, None),
            Ok(Some(String::new()))
        );
    }

    /// **结构性守卫**：两处 profile 读-改-写的**初始读取**必须走 fail-safe 读取器。
    ///
    /// 范围**只覆盖「喂给文本变换的那一次读取」**，即函数体开头到
    /// `merge_profile_block`/`strip_profile_block` 之间那一段。
    ///
    /// 第一版写成"整个函数体不许出现 `read_optional(sftp, &profile)`"，**当场被自己抓红**：
    /// 同一函数里的**写后读回校验**正当地用它，而且那处用 lossy 也是安全的——写进去的一定是
    /// 合法 UTF-8，传输损坏会变成 U+FFFD → 与期望不符 → Mismatch → 回滚，方向 fail-safe。
    /// 收窄了**两次**才对上，两次都是自己抓自己：
    /// ① 初版扫整个函数体 → 撞上写后读回校验那处正当的 `read_optional(sftp, &profile)`；
    /// ② 收到"变换之前"后仍假红 → `install` 的读取段里还有一处正当的 `read_optional`，
    ///    读的是刚部署的 **ccm CLI 脚本**（`CCM_CLI_REMOTE_PATH`），不是 profile。
    /// 所以禁的必须是**读 profile 那一次**的确切形态，不是"任何 `read_optional`"。
    /// 守卫范围必须等于性质范围；本会话第三次栽在同一形状上，故把订正过程留在注释里。
    #[test]
    fn profile_read_modify_write_goes_through_the_failsafe_reader() {
        fn body<'a>(src: &'a str, sig: &str) -> &'a str {
            let i = src
                .find(sig)
                .unwrap_or_else(|| panic!("找不到 {sig}——守卫失效了"));
            let j = src[i..].find("\n}\n").map(|k| i + k).unwrap_or(src.len());
            &src[i..j]
        }
        let src = include_str!("sftp.rs");
        let mut checked = 0usize;
        for (sig, transform) in [
            (
                "pub async fn uninstall_remote_ccm_helper(",
                "strip_profile_block(",
            ),
            (
                "pub async fn install_remote_ccm_helper(",
                "merge_profile_block(",
            ),
        ] {
            let code = body(src, sig)
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            // 反向自检①：真取到函数体了（不是空串在空转）
            assert!(code.contains("upload_atomic("), "{sig}: 取到的体里没有上传");
            let cut = code
                .find(transform)
                .unwrap_or_else(|| panic!("{sig}: 找不到 {transform}——守卫失效了"));
            let before = &code[..cut];
            // 反向自检②：截出来的前半段非空，且确实是读取段
            assert!(
                before.len() > 40 && before.contains("profile"),
                "{sig}: 截出的读取段不像读取段（{} 字节）",
                before.len()
            );
            assert!(
                before.contains("read_profile_text(sftp, &profile"),
                "{sig}: 喂给 {transform} 的 profile 读取没走 fail-safe 读取器"
            );
            assert!(
                !before.contains("read_optional(sftp, &profile)"),
                "{sig}: 又直接拿 read_optional 读 profile 了——那会把「读不出来」当成空文件，\
                 于是跳过备份 + 整份覆盖 / 谎报无需卸载"
            );
            checked += 1;
        }
        assert_eq!(checked, 2, "期望恰好两个 profile 命令，实得 {checked}");
    }

    /// Phase G：回滚措辞必须与实际发生的事一致（机制不许声称做了它没做的事）。
    #[test]
    fn rollback_note_matches_what_actually_happened() {
        assert!(rollback_note(false).contains("已尝试回滚"));
        let n = rollback_note(true);
        assert!(n.contains("没有可回滚的内容"), "{n}");
        assert!(!n.contains("已尝试回滚"), "空 existing 时不许说回滚过：{n}");
        assert!(n.contains("请手动清理"), "要给出恢复路径：{n}");
    }

    /// **结构性守卫**：两条 deploy 路径的**内容**上传必须走 verified。
    ///
    /// 范围只覆盖 `deploy_remote_daemon` 与 `deploy_remote_acct_iso` 两个函数体
    /// ——**第一版写成"全文件不许有裸 upload_atomic"，当场被自己抓**：
    /// ccm helper 那条路（`&profile, stripped/merged`）**故意**用裸上传，
    /// 因为它下游紧接着自己的读回 + 回滚（`sftp.rs` 那三处 `verify_readback`）。
    /// 守卫范围比性质宽 = 假红 = 会被人关掉。
    ///
    /// 版本标记允许裸上传：它是"校验通过"的凭证，必须最后写。
    #[test]
    fn deploy_paths_use_verified_upload_for_content() {
        fn body<'a>(src: &'a str, sig: &str) -> &'a str {
            let i = src
                .find(sig)
                .unwrap_or_else(|| panic!("找不到 {sig}——守卫失效了"));
            let j = src[i..].find("\n}\n").map(|k| i + k).unwrap_or(src.len());
            &src[i..j]
        }
        let checks = [
            (
                body(
                    include_str!("sftp.rs"),
                    "pub async fn deploy_remote_daemon(",
                ),
                "deploy_remote_daemon",
            ),
            (
                body(
                    include_str!("acct_iso_deploy.rs"),
                    "pub async fn deploy_remote_acct_iso(",
                ),
                "deploy_remote_acct_iso",
            ),
        ];
        let mut verified_total = 0usize;
        for (b, what) in checks {
            let code = b
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            // 反向自检：真取到函数体了
            assert!(
                code.contains("upload_atomic"),
                "{what}: 取到的体里没有上传，守卫在空转"
            );
            verified_total += code.matches("upload_atomic_verified(").count();
            for l in code.lines() {
                if !l.contains("upload_atomic(") {
                    continue;
                }
                assert!(
                    l.contains("marker"),
                    "{what}: 内容上传仍走裸 upload_atomic —— {}",
                    l.trim()
                );
            }
        }
        // 计数自检：2 处 daemon 二进制 + 6 个 acct-iso 脚本
        assert_eq!(
            verified_total, 7,
            "期望 1(daemon 体内) + 6(acct-iso)，实得 {verified_total}"
        );
    }

    #[test]
    fn strip_aborts_on_malformed_begin_without_end() {
        // **这条测试原先把 bug 编码进去了**（T04 审计阻塞）：它断言悬空 BEGIN 时
        // strip 是 no-op，而调用方据此打印「远端 … 没有 ccm 块，无需卸载」——
        // 那正是同一个 commit 里被定义为 bug 的形态，只是发生在「卸」这半边。
        // 现在两侧的装与卸四条路全走 `find_pair`，此处必须 Err 中止。
        let corrupt = format!("a\n{CCM_PROFILE_BEGIN}\nccm() {{ :; }}\nuser code\n");
        let e = strip_profile_block(&corrupt, "远端 ~/.bashrc").unwrap_err();
        assert!(e.contains("找不到配对的 END"), "{e}");
        assert!(e.contains("已中止"), "要让用户知道我们没动文件：{e}");
        assert!(e.contains("远端 ~/.bashrc"), "要说清是哪个文件：{e}");
        // 而**没有** BEGIN 时仍是正常的 no-op（别把这条也变成错误）
        assert_eq!(
            strip_profile_block("just user code\n", "远端 ~/.bashrc").unwrap(),
            "just user code\n"
        );
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
