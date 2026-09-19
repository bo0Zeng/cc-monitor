//! SS-D：统一 SFTP 会话层（issue #29 自动部署 F08；F11 用户数据写 / F10 profile 写已复用）。
//!
//! 复用 `ssh_source::connect_session` 的全套 host-key 指纹校验 + publickey/agent 鉴权，
//! 在一条已鉴权的 russh 连接上 `request_subsystem("sftp")` 起 SFTP 子系统（russh-sftp，
//! transport-agnostic，吃 channel 的 AsyncRead+AsyncWrite 流）。
//!
//! ## 只读铁律豁免（INVARIANT §1 / 账本 SS-G）—— 穷举登记见 `src/doc/INVARIANTS.md §1`
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
    /// 🔴 `K-R70`：**这份字节自报的身份**（`build.rs` 从二进制里扫 `CC_MONITOR_BUILD_STAMP`
    /// 得来，不是从旁边那个 `.build_id` 文本文件抄的）。
    ///
    /// 〔墓碑 —— 本结构此前还有一格 `id_from_manifest: bool`，逐字注释是
    ///  「build_id 是否来自 .build_id 清单（true=字节真实身份可信）」。那句话把**标签**
    ///  说成了**真实身份**：清单是 `release.yml` 从源码常量抠出来写的，三个载体恒等
    ///  ⇒ 一格证据都不提供（`K-R68` · `DECISIONS.md#R26` 裁定零）。
    ///  今天身份**只有一条来路**（字节），于是那个见证布尔没有了对立面，删掉；
    ///  它守的那件事换成了 [`bytes_carry_build_stamp`] 在部署路上**无条件**跑一遍。〕
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
///
/// ⚠ **这个函数只回答「版本对不对」一件事**，它的入参里根本没有落点那个文件
/// ——「那个文件在不在」由 [`TargetBinary`] 单独取样、在 [`deploy_decision_at`] 里
/// 与本判定合并。daemon 那条路**只许走 `deploy_decision_at`**（见它的头注）；
/// 本函数留给 `acct_iso_deploy` 那条按目录取标记的路，那里标记与内容同一次上传、
/// 且落点是目录不是单个文件。
pub fn deploy_decision(remote_build_id: Option<&str>, expected: &str) -> DeployAction {
    match remote_build_id {
        None => DeployAction::Deploy("远端无 daemon / 无版本标记".to_string()),
        Some(r) if r.trim() != expected => {
            DeployAction::Deploy(format!("版本不符（远端 {} ≠ 期望 {expected}）", r.trim()))
        }
        Some(_) => DeployAction::Skip,
    }
}

/// 部署落点那个文件**本身**的取样结论（`deploy_decision_at` 的第二个输入）。
///
/// 与 [`interpret_profile_read`] 同一条纪律：**「问不出来」不许读成上面任何一个确定答案**
/// ——把无权限/传输失败当成「不在」会变成每次连接都重传（把版本门控拆了），
/// 当成「在」则退回本枚举要治的那个静默。**所以它不是 `bool`。**
/// ⚠ 成员就在下面，别在散文里复述一份基数 —— 那份字面量会在加成员那天变成假话。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TargetBinary {
    /// stat 说它在，且有字节。
    Present,
    /// stat **明确说**它不在。
    Missing,
    /// stat 说它在，但是 **0 字节** —— 不是假想形态：本模块 `upload_atomic` 里
    /// 「绝不 set_metadata」那条注释记的就是真机 e2e 实测把 daemon 截成 0 字节、
    /// 不可 exec 的那次事故。`try_exists` 会把它算成「在」。
    Empty,
    /// 问不出来（无权限 / 传输失败 / 服务器不给属性）—— 不许读成上面任何一个。
    Unknown,
}

/// 版本这一侧的事实用一句话说出来（给 `Deploy` 的人读原因用）。
/// 单独抽出来是因为**两侧的事实要各自有各自的话**：落点没文件时，
/// 版本可能是对的、不符的、或压根没标记，三种都要说得出来。
fn marker_phrase(remote_build_id: Option<&str>, expected: &str) -> String {
    match remote_build_id {
        None => "且无版本标记".to_string(),
        Some(r) if r.trim() != expected => {
            format!("版本标记也不符（远端 {} ≠ 期望 {expected}）", r.trim())
        }
        Some(_) => format!("而版本标记说它已是 {expected}"),
    }
}

/// 部署决策 —— **两个各自独立的事实合起来判**：`.build_id` 说的「版本对不对」
/// 与落点那个文件的「在不在」。
///
/// ## 为什么不能只看 `.build_id`（K-W4 `§0c`）
///
/// `.build_id` 是 **目录级** 的（[`marker_path`] 把它放在二进制的同目录，
/// 路径里不带二进制名），而 [`deploy_decision`] **只读它、从不 stat 二进制本身**。
/// 于是那一个读数今天同时被当成两件事用：「版本对不对」**和**「那个文件在不在」。
/// 已部署且 build 未变的机器上，二进制被删 / 被截成 0 字节 / `daemonPath` 被改到
/// 同目录另一个文件名，标记照旧匹配 ⇒ 判 `Skip` ⇒ 新的字节**永远不会上传**，
/// 而 exec 走的是那个不存在的路径。
///
/// ⚠ **那时用户看到什么，逐字**（`ssh_source.rs` 的连接面）：
/// 「SSH 连上了，但 daemon 在超时内未回 hello（未部署 / 路径错 / 启动失败？）。」
/// —— 一个**三选一的猜测**。★ 病灶正在这里：**手里握着一条 SFTP 会话、能一问就知道
/// 那个文件在不在的这一层，什么都没说**；而要去猜的是**够不着那个事实**的那一层。
/// 本函数买的就是让前一层把它知道的那半句说出来。
///
/// ⚠ **本条没实测过的部分**：上面那句是从源码摘的逐字串（住址在 `ssh_source.rs` 里
/// `未回 hello` 那一处），**不是**真机跑出来的截图 —— 本轮不碰真远端。
///
/// 本模块此前只断掉了这条链的**上传那一段**（`upload_atomic_verified` 的头注：
/// 「标记写在校验之后，就断了这条链」）—— 那管的是「我们自己传坏了」，
/// **管不到部署成功之后那个文件再出事**。这里补的是后半段。
///
/// ## 边界：不是「每次都重传」
/// 只有落点**明确**没文件 / 是 0 字节才越过版本门控。`Present` 与 `Unknown`
/// 一律交回 [`deploy_decision`]，Batch8/9 那套 stale 防御一个字节没动。
pub fn deploy_decision_at(
    remote_build_id: Option<&str>,
    expected: &str,
    target: TargetBinary,
) -> DeployAction {
    match target {
        TargetBinary::Missing => DeployAction::Deploy(format!(
            "落点没有 daemon 二进制（{}）",
            marker_phrase(remote_build_id, expected)
        )),
        TargetBinary::Empty => DeployAction::Deploy(format!(
            "落点的 daemon 二进制是 0 字节（{}）",
            marker_phrase(remote_build_id, expected)
        )),
        // 「在」与「问不出来」都退回版本门控 —— 后者刻意保守：宁可与今天同答，
        // 也不拿一次 stat 失败换一次全量重传。
        TargetBinary::Present | TargetBinary::Unknown => deploy_decision(remote_build_id, expected),
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
///
/// ⚠ **目录级** —— 路径里不带二进制名。所以它认不出「同目录里换了个文件名」，
/// 那半个事实由 [`probe_target_binary`] 单独取样（K-W4 `§0c`）。
fn marker_path(daemon_path: &str) -> String {
    let dir = remote_parent(daemon_path);
    if dir == "/" {
        "/.build_id".to_string()
    } else {
        format!("{dir}/.build_id")
    }
}

/// [`probe_target_binary`] 那两次取样的**解释**（纯函数，可单测 —— K-W4b）。
///
/// 与 [`interpret_profile_read`] 同一形状：**吃两次调用各自的结果，不吃会话**。
/// 拆出来的理由是一个具体缺陷，不是行数：解释这一半原先焊在 async 体里，
/// 四个状态的映射规则因此一条判据都没有 —— 把那个体换成恒答 `Present`，
/// 全量 cargo **0 红**（09-06 沙箱实测，`tests/evidence/K-W4b-readings.md`），
/// 而部署决策当场退回「只看 `.build_id`」的老病。
///
/// 入参就是两次调用**降解之后**的结果（与 [`read_profile_text`] 传给
/// [`interpret_profile_read`] 的那几个入参同一路数）：
/// - `metadata_size`：`None` = `metadata` 那次调用失败；`Some(inner)` = 成功，
///   `inner` 是服务器给的 size —— ⚠ `Some(None)` 是**服务器没给 size**，不是 0 字节。
/// - `exists`：`metadata` 失败时补问 `try_exists` 的结果（`None` = 它也答不出来）。
///   `metadata` 成功那一路根本不问它（不为常见路径多加一次往返），那时它恒为 `None`
///   而本函数在那一路也不看它。
///
/// 四态各自的含义住 [`TargetBinary`] 的成员注释，映射规则住下面这个 `match`
/// —— 两处都不在散文里复述第二份。
pub(crate) fn interpret_target_probe(
    metadata_size: Option<Option<u64>>,
    exists: Option<bool>,
) -> TargetBinary {
    match metadata_size {
        Some(Some(0)) => TargetBinary::Empty,
        // 服务器不给 size（`Some(None)`）≠ 0 字节：存在是确定的，别把「没说」读成「空」。
        Some(_) => TargetBinary::Present,
        None => match exists {
            Some(false) => TargetBinary::Missing,
            Some(true) => TargetBinary::Present,
            None => TargetBinary::Unknown,
        },
    }
}

/// [`deploy_decision_at`] 的异步取样：**只问落点那个文件在不在 / 有没有字节**。
///
/// 不 `read` 它 —— 那是 2.3 MB 的二进制，为判存在把它拉回来是白花带宽；
/// `metadata` 一次往返就够。取样与判定分开（纯函数可单测）是本模块既有的形状，
/// 见 [`read_profile_text`] / [`interpret_profile_read`]；本函数只取样，
/// 四态怎么映射住 [`interpret_target_probe`]。
///
/// `metadata` 失败才补问 `try_exists`：要区分「明确不在」与「问不出来」，
/// 而这两者在 `metadata` 的 `Err` 里长得一模一样。
async fn probe_target_binary(sftp: &SftpSession, path: &str) -> TargetBinary {
    let metadata_size = sftp
        .metadata(path.to_string())
        .await
        .ok()
        .map(|attrs| attrs.size);
    let exists = match metadata_size {
        // `metadata` 成功就够判了，不多问一次。
        Some(_) => None,
        None => sftp.try_exists(path.to_string()).await.ok(),
    };
    interpret_target_probe(metadata_size, exists)
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
    // 🔴 `K-R70`：**把这几 MB 字节推到别人机器上之前，先让它自己说一遍它是谁。**
    //
    // 〔墓碑 —— 原来这里逐字写着：「首选 .build_id 清单（bin.build_id 即字节真实身份）……
    //  无清单（旧产物）时才用 bytes_contain 启发式兜底——注意它可能误拒正品」，
    //  条件是 `!bin.id_from_manifest && !bytes_contain(bin.bytes, bin.build_id.as_bytes())`。
    //  两处病：① 括号里那句「清单即字节真实身份」是假的（清单从源码常量抄，见 `K-R68`）；
    //  ② 有清单时这道闸**整个跳过** ⇒ 真正会出事的那一形（有人手工塞了别的字节、
    //  清单照旧）恰恰不检查。〕
    //
    // 今天判据**无条件**跑，而且不再是启发式：戳是一段 `#[used] static [u8; N]`，
    // 连续、拆不成立即数（daemon 侧 `CC_MONITOR_BUILD_STAMP`）。
    if !bytes_carry_build_stamp(bin.bytes, bin.build_id) {
        tracing::warn!(
            "内嵌 daemon 的字节里问不出 `{}` 这个身份戳——按身份未知跳过自动部署\
             （这份字节不是这套源码编出来的，或它太旧、还没有身份戳；重跑 zigbuild 重铺）",
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
    // K-W4 §0c：标记是目录级的，光凭它判 Skip 会在「标记还在、二进制没了」时静默跳过。
    let target = probe_target_binary(sftp, &cfg.daemon_path).await;

    match deploy_decision_at(remote_id.as_deref(), bin.build_id, target) {
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

/// 🔴 `K-R70`：**这份字节自己说得出它是 `build_id` 吗** —— 不看它旁边任何文件。
///
/// 找的是 daemon 那侧那段 `#[used] static CC_MONITOR_BUILD_STAMP`：
/// `<开>` ＋ `BUILD_ID` ＋ `<关>`，两个界标的**唯一住址**在
/// `src/backend/main.rs`（`BUILD_STAMP_OPEN` / `BUILD_STAMP_CLOSE`），
/// 由 `build.rs` 抠出来经 `DAEMON_STAMP_OPEN` / `DAEMON_STAMP_CLOSE` 交到这里
/// ⇒ 本文件里**不许出现那两个字面量**
/// （`the_embedded_identity_comes_from_the_bytes_not_from_a_label` 在数它）。
///
/// # 它买到的与买不到的
///
/// ✅ 买到：「这份字节是不是 `build_id` 那一次构建的产物」——**戳与字节同生共死**，
///    改一份旁文件、换一张清单都动不了它。
/// ⚠ 买不到：**防篡改**。谁都能往一段字节里塞一个假戳。它防的是漂移与手滑
///    （拿错文件 / 铺了旧产物 / 只 bump 源码没重编），不防恶意 —— 那要签名，不是戳。
pub fn bytes_carry_build_stamp(bytes: &[u8], build_id: &str) -> bool {
    if build_id.is_empty() {
        return false;
    }
    let stamp = format!(
        "{}{build_id}{}",
        env!("DAEMON_STAMP_OPEN"),
        env!("DAEMON_STAMP_CLOSE")
    );
    bytes_contain(bytes, stamp.as_bytes())
}

/// 按远端 arch 选内嵌的 daemon 二进制（F08b）。build.rs 把交叉编译的 musl 二进制复制进
/// OUT_DIR 并置 `embedded_daemons` cfg 时，这里 `include_bytes!` 内嵌并按 arch 返回；二进制
/// 未就位（无 cfg）→ 返回 None（ensure_daemon_deployed 优雅跳过，沿用手动部署）。
/// `build_id` 取编译期 env —— 🔴 `K-R70` 起那个 env 由 `build.rs` **从二进制字节里扫出来**。
pub fn daemon_binary(arch: &str) -> Option<&'static DaemonBinary> {
    #[cfg(embedded_daemons)]
    {
        // 🔴 `K-R70`：`DAEMON_EMBEDDED_ID_<ARCH>` = `build.rs` 从**这份字节**里扫出的身份戳。
        //
        // 〔墓碑 —— 原来这里有一个 `pick()`：清单为空就退回 `env!("DAEMON_BUILD_ID")`（源码 id）。
        //  那是「问不出来就拿源码的答案顶上」——把一个失败面换成一个假答案（`brief` 里
        //  已删的那条按需拉取路的禁词表逐字点名的第三条）。今天它不需要了：
        //  `build.rs` 在**任一 arch 的字节里扫不出身份时当场 panic**，扫得出才置
        //  `embedded_daemons` cfg ⇒ 走到这里的路径上，这两个 env 结构上不可能是空串。
        //  「结构上不可能」不许当成不检查的理由 ⇒ 下面 `deploy_embedded_daemon` 出门前
        //  仍无条件跑一遍 `bytes_carry_build_stamp`，本文件的判据也钉住这两处取值口。〕
        //
        // 身份与期望（`EXPECTED_DAEMON_BUILD_ID` = 源码）**仍然分离**：陈旧内嵌 =
        // 身份 p1f ≠ 期望 p1g → 部署照做（远端至少拿到 p1f）但 confirmed=p1f
        // → 降级不传新 flag，比「拒部署」更平滑。
        static X86: DaemonBinary = DaemonBinary {
            build_id: env!("DAEMON_EMBEDDED_ID_X86_64"),
            bytes: include_bytes!(concat!(env!("OUT_DIR"), "/daemon-x86_64")),
        };
        static ARM: DaemonBinary = DaemonBinary {
            build_id: env!("DAEMON_EMBEDDED_ID_AARCH64"),
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
    // K-W4 §0c：手动「安装 daemon」按钮此前也只看标记 —— 落点文件被删/截断时，
    // 它会对着一个不存在的文件回「已是最新，无需重装」。同一条病，同一处修法。
    let target = probe_target_binary(sftp, &path).await;
    match deploy_decision_at(remote_id.as_deref(), bin.build_id, target) {
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
///
/// ⚠ `K-R62` 起是 `pub(crate)`：**本机 POSIX 那条路装的是同一个块**
/// （`profile_installer::plan_install` 的 `PosixRc` 臂走 [`merge_profile_block`]）。
/// 在那边抄一对同样的字符串就是第二个住址 —— 而「同一件事有两个住址」正是
/// `KR62D1` 那条「不许变成第四套」要挡的东西。名字里的 `remote` 是历史，
/// 今天它的意思是「**POSIX rc 里那一对围栏**」，本机远端共用。
pub(crate) const CCM_PROFILE_BEGIN: &str = "# === cc-monitor remote ccm BEGIN ===";
pub(crate) const CCM_PROFILE_END: &str = "# === cc-monitor remote ccm END ===";

/// 远端 ↗ 拉前用的 `ccm` wrapper（**后端拥有**，install 写它而非前端传入——见审计 S-1：
/// 写进 ~/.bashrc 的是被 shell **执行**的代码，绝不能让前端注入任意 bash）。
///
/// **必须与前端 `remote-section.ts::CCM_WRAPPER_SNIPPET`（面板展示/手动复制用）逐字一致。**
/// **单一来源**：`src/shared/ccm-aliases.sh`——前端 `remote-section.ts` 经 `?raw` import
/// 同一文件（修复历史漂移：Batch7 重构时只改了前端展示版，装进远端的还是老版）。
///
/// **F02 起本块只剩「别名层」**；`K-R48` 第二拍起它指向的那个 `ccm` 是 [`ccm_entry_shim`]
/// （三行入口，转给后端本体），不再是一份 bash 实现。
/// 理由：shell 函数**优先于 PATH**，装成函数则与用户已有同名函数硬冲突且必然被遮蔽（实测）；
/// 且远端是 zsh/fish 时 `.bashrc` 根本不被 source，函数形态拿不到（审计 D2）。
/// ⚠ `K-R49` 起它是 `pub(crate)`：`account_aliases::collision_note` 要问
/// 「`cc` / `cct` 这几个名字是不是已经被自带的别名块占了」，
/// 而那个答案**只有这份文件说了算** —— 在那边抄一份名字清单就是第二个住址。
/// 〔`K-R58` 09-11：`cch` 从这份文件里删了 ⇒ 它**不再**被当作「已被占用」，
/// 用户可以自己定义一个 `cch`。**多一格自由，不是回归。**〕
pub(crate) const CCM_WRAPPER_SNIPPET: &str = include_str!("../../shared/ccm-aliases.sh");

/// 自带别名块里**今天定义了哪几个名字** —— 现算，不写死（`13b`：闭集只许有一个住址，
/// 那个住址就是 `src/shared/ccm-aliases.sh` 自己）。
///
/// `account_aliases` 的撞名判据与本文件的文档对账判据都拿它当人群，
/// 于是「删/加一个别名」这件事**不需要同时去改两份名单**（改漏一份正是 `KR58D1`
/// 的失效方向）。
///
/// 🔴 〔`K-R62` 09-11〕**它从 `#[cfg(test)]` 转正了**，因为多了一个生产使用者：
/// `profile_installer::render_manual_cleanup_hint` 要回答「你 rc 里那几行裸的
/// `cc()` / `cct()`，会不会把我们装的那一块遮蔽掉」—— 那个答案**只有这份文件说了算**，
/// 在提示文案里抄一份名字清单就是第二个住址。转正**没有放宽任何东西**：
/// 它仍然现算自 [`CCM_WRAPPER_SNIPPET`]，一个字节的名单都没写死。
///
/// ⚠ **它认的形状写死在这里**：`<名>() {`（`()` 与 `{` 之间允许空白）。
/// 注释行里那两条示例（`#   zcc()  { … }`）靠「名字只许 `[A-Za-z0-9_]`」被剔掉 ——
/// 换一种写法（`function cc {`）它会**漏**，而漏出来的形状是「人群变空」，
/// 调用处一律先断 `!is_empty()`，不让它静默变成空真。
pub(crate) fn builtin_alias_names() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = CCM_WRAPPER_SNIPPET
        .lines()
        .filter_map(|l| {
            let (name, rest) = l.split_once("()")?;
            if !rest.trim_start().starts_with('{') {
                return None;
            }
            let name = name.trim();
            (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
                .then_some(name)
        })
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// 🔴 **`K-R48` 第二拍（09-11）：`CCM_CLI_SCRIPT` 没了，这里是它的墓碑。**
///
/// 原来这一行是 `pub(crate) const CCM_CLI_SCRIPT: &str = include_str!("../../shared/ccm");`
/// —— 把那个 1592 行的 bash 启动器整份编进产物，再 SFTP 推到远端 `~/.local/bin/ccm`。
/// 〔用@09-11 `K33`〕逐字：「后端**只有一个**…**不要有什么 bash 脚本**，**不要有什么单独的 ccm**。
/// **所有命令只许有一处**，其他都是**根据传参来调用**」⇒ 那个文件删了。
///
/// **`KR48D1` 盯的就是这一行**：那句 `include_str!` 在 = 脚本仍是产品的一部分。今天 0。
///
/// 远端那份 `~/.local/bin/ccm` 换成 [`ccm_entry_shim`] —— **三行、零实现**，
/// 只把 argv 原样转给已经部署好的后端（`intercept` 的第二条入口 `<bin> ccm <argv…>`）。
fn _kr48d1_tombstone() {}

/// 远端 `~/.local/bin/ccm` 的内容：**一个入口，不是一份实现**。
///
/// 🔴 **`K-R69` 09-12：这个生成器搬到了 `backend/control/local_backend.rs`。**
/// 理由是它有了**第二个读者** —— 本机也要一条 `ccm` 入口，而「本机那条与远端那条同源」
/// 这句话只有在两边取自**同一处**时才是结构性的（各写一份就只是巧合，而巧合会漂）。
/// ⇒ 本文件不再自己拼那三行，改成调它；`ccm` 这个词的唯一住址是
/// [`crate::backend::control::local_backend::CCM_ENTRY_WORD`]。
/// 头注（不许有第二个 `case` / 为什么不是软链）逐字跟着搬过去了，别在这里再写一份。
use crate::backend::control::local_backend::ccm_entry_shim;

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
    // 🔴 `K-R48` 第二拍：推的不再是那个 1592 行的 bash 启动器，是 [`ccm_entry_shim`]
    //    —— 三行、零实现，只把 argv 转给**已经部署好的后端**（`ensure_daemon_deployed`
    //    把它推到 `cfg.daemon_path`，默认约定 `~/.cc-monitor/bin/cc-monitor-remote`）。
    // ⚠ **入口与后端本体的部署是两条路，这里刻意不合并**：本函数是「装 shell 便捷层」，
    //   后端本体由连接流程自己保证；合并就等于在这条路上再造一次部署逻辑（第二处实现）。
    let shim = ccm_entry_shim(&cfg.daemon_path);
    upload_atomic(sftp, CCM_CLI_REMOTE_PATH, shim.as_bytes(), 0o755)
        .await
        .map_err(|e| format!("部署 ccm 入口到远端 ~/{CCM_CLI_REMOTE_PATH} 失败: {e}"))?;
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
        crate::verified_write::verify_readback(&shim, &cli_back)
    {
        return Err(format!(
            "ccm 入口写后校验失败：{detail} 未改动 {profile}；\
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
#[path = "../../../tests/bridge/sftp_tests.rs"]
mod tests;
