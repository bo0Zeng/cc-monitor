//! SS-D：远端**自部署**的业务那一半（issue #29 自动部署 F08 · 手动安装 / 卸载 · `ccm` 入口）＋ 别名块的规划。
//!
//! 〔SR1b〕执行那一半（SFTP）不在本模块：经本机常驻后端的 `files` 链路（见下「本模块手里已经没有 SFTP 了」）。
//! 〔墓碑 —— 从前这里逐字「复用 `ssh_source::connect_session` 的全套 host-key 指纹校验 + publickey/agent 鉴权，
//!  在一条已鉴权的 russh 连接上开 SFTP 子系统」—— 那条进程内拨号随 SR1b 删了。〕
//!
//! ## 只读铁律豁免（INVARIANT §1 / 账本 SS-G）—— 穷举登记见 `src/doc/INVARIANTS.md §1`
//! cc-monitor 对远端的写入均**用户显式触发**，各自独立路径守卫、绝不混用：
//! - **F08**：自部署后端二进制到 `~/.cc-monitor/bin/`（非用户数据、幂等、版本门控）。
//! - **F11**：用户**主动**删除远端会话 jsonl。〔RW1 · 第四波 09-24〕**已不在本模块**：经远端后端的
//!   `files-delete-session`（只收 sid）删，从前那道 SFTP 直删与它的结构守卫〔散文墓碑〕走了。
//! - **F89a**：用户**显式**增/改/删远端**项目** `.mcp.json`（字符串守卫 `is_safe_remote_mcp_json`：
//!   绝对 + 尾 `/.mcp.json` + 无 `..` + 非裸）。〔RW1 · 第四波 09-24〕**写已不在本模块**：
//!   经远端后端（`mcp::write_project_mcp_server` → `user_files`），不再 SFTP 直写。
//!   **SS-14**：写面**只** `.mcp.json`，非 Claude 会话数据。
//! - **F10**：别名块装/卸——〔AL2 · 第四波 4D〕今天是 `lib.rs` 的 `aliases_block_install` / `aliases_block_remove`（带 `origin`，本机远端同一条）
//!   （〔MC1〕从前这一对叫 `install_remote_ccm_helper`〔散文墓碑〕/ `uninstall_…`，推入口那一半并进了 [`deploy_remote_backend`]）
//!   （BEGIN/END 块 + 备份 + 写后校验回滚）；本机 profile 写在 `profile_installer`。（batch20 审计修：原「非远端」措辞误——本模块确写远端 `~/.bashrc`。）
//!   〔RW1 · 第四波 09-24〕**落盘已不在本模块**：经那台后端读改写（`user_files`）；规划那一半
//!   （`merge_profile_block` / `strip_profile_block`）〔W5-ALIAS〕住 `profile_installer`。
//! - **F50**：`pubkey::push_public_key` 经 SSH-exec 追加公钥到远端 `~/.ssh/authorized_keys`（不在本模块，登记于此备查）。
//!
//! 原子写（EXCL 临时件 → 旧目标先**改名成 `.bak`**（不是删）→ 上位 → 清 `.bak`）与它的来历住后端
//! `dial/sftp.rs::put_atomic`（F89a 审计后加固 · DN-7 订正「删旧」那句 · setstat 截断事故）。
//!
//! # 〔SR1b · 2026-09-24〕**本模块手里已经没有 SFTP 了**
//!
//! 用户 V89「SFTP 进本机常驻后端，只写暂存区」：SFTP 客户端住本机常驻后端（`src/backend/dial/sftp.rs`，
//! 与其它 SSH 同一条连接），**远端写只许两处**（`~/.cc-monitor/staging/` · `~/.cc-monitor/bin/`）。本模块留下的是
//! **部署的业务判定**（版本门控 · 落点四态 · 读回判定 · 入口的序列），执行经 [`crate::dial_host::RemoteFs`]
//! （本机后端那条 `files` 链路的一问一答）。〔墓碑 —— 从前本模块自己开 SFTP：`connect_sftp`〔散文墓碑〕在一条
//! 进程内拨的 russh 连接上开子系统，`upload_atomic`〔散文墓碑〕在这里跑「EXCL 临时件 → 旧的改名 `.bak` → 上位」。
//! 那段序列与它的两条事故教训（先备份不删旧 · **绝不** rename 之后 setstat 兜底 chmod）逐字搬去了后端
//! `dial/sftp.rs::put_atomic` 的头注。〕
//! ⇒ 落点变了一格：`ccm` 入口从 `~/.local/bin/ccm` 挪到 **`~/.cc-monitor/bin/ccm`**（两个写根之内；也正是
//! `设计/01 §6.7b` 用户 09-18 拍的落点；自带别名块把 `~/.cc-monitor/bin` 加进 PATH）。

use crate::copy_table::copy_text;

use crate::dial_host::{Readback, RemoteFs};
use crate::ssh_source::RemoteConfig;

/// 判定一次远端上传的读回结果。**纯函数，可测**——远端往返塞不进单测，
/// 但"读回的字节该不该判通过"这条判据可以，而它正是此前完全缺失的那一环。
///
/// 按字节而不是按字符串：`deploy_remote_backend` 上传的是**可执行二进制**。
/// 〔SR1b〕读回那一趟住本机后端（它就在远端文件旁边，不必把 MB 级的字节再拉回界面），
/// 它交回的是**比对的事实**（读回长度 · 首个差异的偏移，读不回 ⇒ `None`）；判不判通过、话怎么说仍住这里。
pub fn verify_readback(path: &str, expected_len: u64, readback: Readback) -> Result<(), String> {
    let Some((got_len, first_diff)) = readback else {
        return Err(copy_text(
            "rsSftp.verify.unreadable",
            &[("path", &path.to_string())],
        ));
    };
    if got_len == expected_len {
        let Some(at) = first_diff else {
            return Ok(());
        };
        return Err(copy_text(
            "rsSftp.verify.contentDiffers",
            &[
                ("path", &path.to_string()),
                ("expectedLen", &expected_len.to_string()),
                ("at", &at.to_string()),
            ],
        ));
    }
    Err(copy_text(
        "rsSftp.verify.lengthDiffers",
        &[
            ("path", &path.to_string()),
            ("expectedLen", &expected_len.to_string()),
            ("gotLen", &got_len.to_string()),
        ],
    ))
}

/// 上传 + **读回逐字节比对**。
///
/// ## 为什么这个函数此前不存在（T04 审计①）
///
/// `deploy_remote_backend` 与 `deploy_remote_acct_iso` 的**全部**上传
/// ——1 个后端可执行二进制 + 6 个远端脚本（含 0755 的 `cc-acct-iso` / `lib.sh` /
/// install.sh）——写完**直接写版本标记**，中间没有任何读回。
///
/// 而 T04 第二步我论证「备份→写→读回比对→回滚这个范式已共享（5 处），所以不用抽」
/// ——**那 5 处全在 profile/CLI 那条线上，压根没覆盖这两条 deploy 路**。
/// 我那套"五套机制"框架恰好把这个洞盖住了：把"范式已共享"当成了"范式已覆盖"。
///
/// 后果具体：传输损坏的后端二进制照样被写上正确的 `.build_id` 标记 →
/// 下次 `deploy_decision` 判「已是最新，跳过」→ **坏二进制永久驻留**，
/// 而用户看到的是部署成功。标记写在校验之后，就断了这条链。
/// 〔SR1b〕上传与读回都经本机后端（`RemoteFs::put`，`verify` 那一格）；判定照旧是 [`verify_readback`]。
///
/// 〔DP1 · 第四波〕**读回不对 ⇒ 当场删掉传坏的那一份。** 从前断这条链靠「标记写在校验之后」；后端那条路的旁挂标记
/// 退役之后（身份读字节自己的戳），一份传坏的字节若恰好还带着对的戳，下次会被判「已是这一版」⇒ 坏字节永久驻留。
/// 删掉它，下次就是「落点没有 ⇒ 装」。删不掉也要说出来（那一份还在）。
pub(crate) async fn upload_verified(
    fs: &RemoteFs,
    remote_path: &str,
    bytes: &[u8],
    mode: u32,
) -> Result<(), String> {
    let back = fs.put(remote_path, bytes, mode, true).await?;
    let Err(bad) = verify_readback(remote_path, bytes.len() as u64, back) else {
        return Ok(());
    };
    Err(match fs.remove(remote_path).await {
        Ok(_) => copy_text("rsSftp.upload.badRemoved", &[("bad", &bad.to_string())]),
        Err(e) => copy_text(
            "rsSftp.upload.badKept",
            &[("bad", &bad.to_string()), ("e", &e.to_string())],
        ),
    })
}

/// 版本标记那一类小文件：读回来的字节；读不出 ⇒ `None`（标记不在，下一步就是部署）。
pub(crate) async fn read_marker(fs: &RemoteFs, path: &str) -> Result<Option<Vec<u8>>, String> {
    Ok(fs.read(path, MARKER_READ_MAX).await?.0)
}

/// 写版本标记：**不读回**（它是「校验通过」的凭证，只在内容那一份读回对了之后才写；它自己坏了下次重部署就是了）。
pub(crate) async fn put_marker(
    fs: &RemoteFs,
    path: &str,
    bytes: &[u8],
    mode: u32,
) -> Result<(), String> {
    fs.put(path, bytes, mode, false).await.map(|_| ())
}

/// 标记 / 入口这类小文件一次最多读多少（它们都是几十字节；超了是那台机器上的东西不对）。
const MARKER_READ_MAX: u64 = 64 * 1024;

/// 远端那台要的那一份后端（〔DP1〕字节从 `byte_table` 按那台的 (OS, arch) 取，`include_bytes!` 不在本文件）。
pub struct BackendBinary {
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
    /// 那台机器是哪一格（说给人听：「Linux / x86_64」）。
    pub machine: String,
}

/// 〔DP1 · 第四波〕远端那台要哪一份后端：**先问它是什么机器**（`uname -s -m`，`byte_table::probe_key`），
/// 再查表（`byte_table::choose`）。三种结果分得开：链路没通（`Err`）· 表拒绝了（`Ok(Err(拒绝))`）· 拿到字节。
///
/// 〔墓碑 —— 从前这里只问 `uname -m`、再按 arch 取字节（不认 OS）：Windows 远端若恰好答得出 `uname -m`，
///  会被推一份 Linux 字节（`设计/96 §7.3` 第二条）。〕
async fn remote_backend_binary(
    cfg: &RemoteConfig,
) -> Result<Result<BackendBinary, crate::byte_table::Refusal>, String> {
    use crate::byte_table::{choose, probe_key, Product, Route};
    let key = probe_key(cfg).await?;
    let machine = key.as_ref().map(|k| k.label()).unwrap_or_default();
    Ok(
        choose(Product::Backend, Route::Remote, key).map(|p| BackendBinary {
            // 后端那几槽的身份由 `build.rs` 从字节里扫出（`K-R70`）；空串过不了下面那道 `bytes_carry_build_stamp`。
            build_id: p.build_id.unwrap_or_default(),
            bytes: p.bytes,
            machine,
        }),
    )
}

/// 部署决策（纯函数，可单测）。
#[derive(Debug, PartialEq, Eq)]
pub enum DeployAction {
    /// 远端版本与期望一致 → 无需部署。
    Skip,
    /// 需要部署，附人读原因。
    Deploy(String),
    /// 〔HX2 · 主会话 D-b〕那台上是**另一版、但不比这一版旧**（更新 · 同序不同名 · 序解不出）⇒ **不动它**，
    /// 照旧连上那一份。`theirs` = 那台上那一份自报的身份；`why` = 人读原因（点名两边各是哪一版）。
    /// 只有 [`identity_decision`] 产这一格。
    Keep { theirs: String, why: String },
}

/// 比对远端版本标记与期望 build_id，决定是否（重）部署。
///
/// ⚠ **这个函数只回答「版本对不对」一件事**，入参是一份旁挂的版本标记。
/// 〔DP1 · 第四波〕backend 那条路**不走它**：后端的身份读那份字节自己里的身份戳（[`identity_decision`]，
/// `设计/96 §7.2.1`），旁挂标记在那条路上退役了。本函数只留给 `acct_iso_deploy` 那条按目录取标记的路 ——
/// 那里是一批脚本（没有身份戳可读），标记与内容同一次上传，且落点是目录不是单个文件。
pub fn deploy_decision(remote_build_id: Option<&str>, expected: &str) -> DeployAction {
    match remote_build_id {
        None => DeployAction::Deploy(copy_text("rsSftp.deploy.missing", &[])),
        Some(r) if r.trim() != expected => DeployAction::Deploy(copy_text(
            "rsSftp.deploy.versionMismatch",
            &[
                ("remote", &(r.trim()).to_string()),
                ("expected", &expected.to_string()),
            ],
        )),
        Some(_) => DeployAction::Skip,
    }
}

/// 部署落点那个文件**本身**的取样结论（〔DP1〕[`remote_identity`] 的第一步：没有 / 0 字节就不必再问它是谁）。
///
/// 纪律：**「问不出来」不许读成上面任何一个确定答案**
/// ——把无权限/传输失败当成「不在」会变成每次连接都重传（把版本门控拆了），
/// 当成「在」则退回本枚举要治的那个静默。**所以它不是 `bool`。**
/// ⚠ 成员就在下面，别在散文里复述一份基数 —— 那份字面量会在加成员那天变成假话。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TargetBinary {
    /// stat 说它在，且有字节。
    Present,
    /// stat **明确说**它不在。
    Missing,
    /// stat 说它在，但是 **0 字节** —— 不是假想形态：原子上传那一段（今天住后端 `dial/sftp.rs::put_atomic`）里
    /// 「绝不 set_metadata」那条注释记的就是真机 e2e 实测把后端截成 0 字节、
    /// 不可 exec 的那次事故。`try_exists` 会把它算成「在」。
    Empty,
    /// 问不出来（无权限 / 传输失败 / 服务器不给属性）—— 不许读成上面任何一个。
    Unknown,
}

/// 〔DP1 · 第四波〕**那台机器上落点那一份后端是谁** —— `设计/96 §7.2.4` 那张四态表 ＋ 0 字节那一格。
///
/// 〔墓碑 —— 从前这一问读的是同目录一份旁挂的版本标记文件（目录级，路径里不带二进制名），再与落点那个文件在不在
///  合起来判（`K-W4 §0c`）：标记是**标签不是指纹**，二进制被换成别的东西而标记照旧，就会判「已是这一版」。
///  `96 §7.2.1`：「读它字节里那段身份戳，不跑它」。〕
/// 四态**不许合并**：没装 ⇒ 装；问不出 / 问出多个 / 读不到 ⇒ **显式失败、不覆盖**（「判不了」要作为结论说出来）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RemoteIdentity {
    /// 落点没有那个文件（没装）。
    Missing,
    /// 落点那个文件是 0 字节 —— 里面没有任何可保护的东西（真机截断事故就是这一形），按「没装」装。
    Empty,
    /// 那份字节里恰好一个身份戳。
    Stamp(String),
    /// 有文件、字节里一个身份戳都没有（不是我们编的 / 太旧 / 被改过）。
    NoStamp,
    /// 问出不止一个身份（身份不唯一）。
    Ambiguous(Vec<String>),
    /// 读不到（权限 / 链路 / 那台上没有能用的只读扫描手段）—— **判不了**。
    Unreadable(String),
}

/// 〔E2 · `96 §7.2.2`〕**手上一份字节自报的身份**（本机那一跳：不经 shell、不跑它，直接扫字节）。
/// 与远端那条扫描（[`stamp_scan_cmd`] ＋ [`interpret_stamp_scan`]）同一条规矩：界标之间 `[[:alnum:]_.-]+`，恰好一个才是身份。**纯函数**。
pub(crate) fn identity_of_bytes(bytes: &[u8]) -> RemoteIdentity {
    if bytes.is_empty() {
        return RemoteIdentity::Empty;
    }
    let (open, close) = (
        env!("BACKEND_STAMP_OPEN").as_bytes(),
        env!("BACKEND_STAMP_CLOSE").as_bytes(),
    );
    let ok = |b: &u8| b.is_ascii_alphanumeric() || b"_.-".contains(b);
    let mut ids: Vec<String> = Vec::new();
    let mut i = 0;
    while let Some(k) = bytes[i..].windows(open.len()).position(|w| w == open) {
        let start = i + k + open.len();
        let n = bytes[start..].iter().take_while(|b| ok(b)).count();
        if n > 0 && bytes[start + n..].starts_with(close) {
            ids.push(String::from_utf8_lossy(&bytes[start..start + n]).into_owned());
        }
        i = start;
    }
    ids.sort();
    ids.dedup();
    match ids.len() {
        0 => RemoteIdentity::NoStamp,
        1 => RemoteIdentity::Stamp(ids.remove(0)),
        _ => RemoteIdentity::Ambiguous(ids),
    }
}

/// 在目标机器上扫身份戳的那一条命令（`96 §7.2.1` 档 A：目标机器**自己的**只读工具，一次 exec，常数字节回传）。
///
/// 正则与 `tests/scripts/re-embed.sh::bytes_id` 同一条（界标之间是 `[[:alnum:]_.-]`，这里要**至少一个字符** ——
/// 两个界标在 `.rodata` 里挨着就是空串那一形，`build.rs::bytes_build_id` 也不收它）。
/// 界标**不写字面量**，只从 `build.rs` 交进来的 env 取（闭集唯一住址在后端源码）。**纯函数**。
/// 〔E2〕`word` 是那个文件在远端 shell 里的**写法**（固定落点常量，自带 `"$HOME"`），不再是一条要 quote 的外来路径。
pub(crate) fn stamp_scan_cmd(word: &str) -> String {
    let ere = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if "\\^$.|?*+()[]{}".contains(c) {
                    format!("\\{c}")
                } else {
                    c.to_string()
                }
            })
            .collect()
    };
    let pattern = format!(
        "{}[[:alnum:]_.-]+{}",
        ere(env!("BACKEND_STAMP_OPEN")),
        ere(env!("BACKEND_STAMP_CLOSE"))
    );
    format!(
        "LC_ALL=C grep -aoE {} -- {word}",
        crate::ssh_source::shell_quote(&pattern),
    )
}

/// 那一条扫描的收全结果 → [`RemoteIdentity`]（落点在、不是 0 字节时才问）。**纯函数**。
///
/// `grep` 的退出码：0 = 扫到了 · 1 = 一个都没有 · 其余（2 = 读不了 / 链路断了没给退出码）= 判不了。
pub(crate) fn interpret_stamp_scan(
    exit: Option<u32>,
    stdout: &str,
    stderr: &str,
) -> RemoteIdentity {
    let (open, close) = (env!("BACKEND_STAMP_OPEN"), env!("BACKEND_STAMP_CLOSE"));
    match exit {
        Some(0) => {
            let mut ids: Vec<String> = stdout
                .lines()
                .filter_map(|l| l.trim().strip_prefix(open)?.strip_suffix(close))
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .collect();
            ids.sort();
            ids.dedup();
            match ids.len() {
                0 => RemoteIdentity::NoStamp,
                1 => RemoteIdentity::Stamp(ids.remove(0)),
                _ => RemoteIdentity::Ambiguous(ids),
            }
        }
        Some(1) => RemoteIdentity::NoStamp,
        // 退出码不进这句话（`Some(..)` / `None` 是实现的形状）：没有错误输出时只说「没答完」。
        _ => RemoteIdentity::Unreadable(match stderr.trim() {
            "" => copy_text("rsSftp.stamp.unfinished", &[]),
            said => said.to_string(),
        }),
    }
}

/// 〔HX2 · 主会话 D-b「部署只在『我的比盘上的新』时才换（BUILD_ID 可比序）」〕**`BUILD_ID` 的序键** —— 唯一实现。
///
/// 形状 `p<代号>` ＋ `<一个小写字母>` ＋ `-<名>`（`p1a-history` … `p3m-ssh-zlib`）⇒ 序键 `(代号, 字母)`。
/// 解不出 ⇒ `None`（**不可比**，不是「最旧」也不是「最新」）。下一次 bump 写出解不出的形状由
/// `sftp_tests::hx2_every_build_id_ever_shipped_has_an_order_and_the_history_climbs` 当场红（它读后端源码里的历史表）。
/// **纯函数**。
pub(crate) fn build_order(id: &str) -> Option<(u32, u8)> {
    let rest = id.strip_prefix('p')?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let generation: u32 = rest[..digits].parse().ok()?;
    let tail = rest[digits..].as_bytes();
    match tail {
        [letter, b'-', name @ ..] if letter.is_ascii_lowercase() && !name.is_empty() => {
            Some((generation, *letter))
        }
        _ => None,
    }
}

/// 「手上这一版比那台上的新」—— 两边都解得出序键、且这一版的严格大。解不出任何一边 ⇒ `false`（不可比 ⇒ 不换）。
pub(crate) fn is_newer(mine: &str, theirs: &str) -> bool {
    matches!((build_order(mine), build_order(theirs)), (Some(m), Some(t)) if m > t)
}

/// 要不要（重）部署 —— **对照物是手上那份字节自报的身份**（`96 §7.2.3`），不是源码常量。**纯函数**。
///
/// `Err` = 显式失败、**一个字节都不写**（出路交给用户：机器页「卸载后端」删掉那个文件，就是明确授权覆盖）。
///
/// 〔HX2 · D-b〕「另一版」那一格按 [`build_order`] 拆两格：那台上的**比这一版旧** ⇒ 换；**不比这一版旧** ⇒ [`DeployAction::Keep`]
/// （两个不同版本的 monitor 连同一台远端，从此只会升不会降，不再每次连上互相换掉 —— 审计 E3）。
pub(crate) fn identity_decision(
    id: &RemoteIdentity,
    expected: &str,
    machine: &str,
    path: &str,
) -> Result<DeployAction, String> {
    let hands_off = copy_text("rsSftp.identity.handsOff", &[]);
    match id {
        RemoteIdentity::Missing => Ok(DeployAction::Deploy(copy_text(
            "rsSftp.identity.missing",
            &[],
        ))),
        RemoteIdentity::Empty => Ok(DeployAction::Deploy(copy_text(
            "rsSftp.identity.empty",
            &[],
        ))),
        RemoteIdentity::Stamp(s) if s == expected => Ok(DeployAction::Skip),
        RemoteIdentity::Stamp(s) if is_newer(expected, s) => Ok(DeployAction::Deploy(copy_text(
            "rsSftp.identity.other",
            &[("s", &s.to_string()), ("expected", &expected.to_string())],
        ))),
        RemoteIdentity::Stamp(s) => Ok(DeployAction::Keep {
            theirs: s.clone(),
            why: copy_text(
                "rsSftp.identity.notOlder",
                &[
                    ("machine", &machine.to_string()),
                    ("s", &s.to_string()),
                    ("expected", &expected.to_string()),
                ],
            ),
        }),
        RemoteIdentity::NoStamp => Err(copy_text(
            "rsSftp.identity.unstamped",
            &[
                ("machine", &machine.to_string()),
                ("path", &path.to_string()),
                ("handsOff", &hands_off.to_string()),
            ],
        )),
        RemoteIdentity::Ambiguous(ids) => Err(copy_text(
            "rsSftp.identity.multiple",
            &[
                ("machine", &machine.to_string()),
                ("path", &path.to_string()),
                ("ids", &ids.join(&copy_text("rsSftp.identity.listSep", &[]))),
                ("handsOff", &hands_off.to_string()),
            ],
        )),
        RemoteIdentity::Unreadable(why) => Err(copy_text(
            "rsSftp.identity.undecidable",
            &[
                ("machine", &machine.to_string()),
                ("path", &path.to_string()),
                ("why", &why.to_string()),
            ],
        )),
    }
}

/// 〔DP1〕问那台落点那一份是谁：先 stat（没有 / 0 字节就不必再问），在就扫它字节里的身份戳（一次 exec，不跑它）。
/// `rel` = SFTP 那一侧（家目录相对），`word` = 同一个文件在远端 shell 里的写法（两格都是固定常量，〔E2〕）。
async fn remote_identity(
    cfg: &RemoteConfig,
    fs: &RemoteFs,
    rel: &str,
    word: &str,
) -> Result<RemoteIdentity, String> {
    Ok(match probe_target_binary(fs, rel).await? {
        TargetBinary::Missing => RemoteIdentity::Missing,
        TargetBinary::Empty => RemoteIdentity::Empty,
        TargetBinary::Present | TargetBinary::Unknown => {
            match crate::ssh_source::connect_and_exec_capture(cfg, &stamp_scan_cmd(word), None)
                .await
            {
                Ok(r) => interpret_stamp_scan(r.exit_status, &r.stdout, &r.stderr),
                Err(e) => RemoteIdentity::Unreadable(e),
            }
        }
    })
}

/// 远端路径的父目录（远端恒为 POSIX `/` 分隔，不用 std::path）。
fn remote_parent(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) => "/",
        Some(i) => &path[..i],
        None => ".",
    }
}

/// [`probe_target_binary`] 那两次取样的**解释**（纯函数，可单测 —— K-W4b）。
///
/// 形状：**吃两次调用各自的结果，不吃会话**。
/// 拆出来的理由是一个具体缺陷，不是行数：解释这一半原先焊在 async 体里，
/// 四个状态的映射规则因此一条判据都没有 —— 把那个体换成恒答 `Present`，
/// 全量 cargo **0 红**（09-06 沙箱实测，`tests/evidence/K-W4b-readings.md`），
/// 而部署决策当场退回「只看 `.build_id`」的老病。
///
/// 入参就是两次调用**降解之后**的结果：
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

/// [`remote_identity`] 的第一步取样：**只问落点那个文件在不在 / 有没有字节**。
///
/// 不 `read` 它 —— 那是 2.3 MB 的二进制，为判存在把它拉回来是白花带宽；
/// `metadata` 一次往返就够。取样与判定分开（纯函数可单测）是本模块既有的形状，
/// 本函数只取样，
/// 四态怎么映射住 [`interpret_target_probe`]。
///
/// `metadata` 失败才补问 `try_exists`（〔SR1b〕这一问住后端 `stat`，一趟带回两样）：要区分「明确不在」与
/// 「问不出来」，而这两者在 `metadata` 的 `Err` 里长得一模一样。链路本身坏了（问都没问出去）⇒ `Err`，不是 `Unknown`。
async fn probe_target_binary(fs: &RemoteFs, path: &str) -> Result<TargetBinary, String> {
    let (metadata_size, exists) = fs.stat(path).await?;
    Ok(interpret_target_probe(metadata_size, exists))
}

/// 〔DP1 · 第四波〕自动部署没成的两种说法 —— **类型上与「部署成功」分得开**（`设计/96 §7.1.4` 第 3 条：
/// 「返回类型上不许有『成功』这一支」）。〔墓碑 —— 从前是 `Result<Option<String>, String>`：`Ok(None)` 就是
/// 「没部署也算成功」那一支，路径含 `~` / 问不出 arch / 没这格字节 / 字节问不出身份全落在它上面、只留一行 `debug!`。〕
#[derive(Debug)]
pub enum DeployError {
    /// 取字节那一步拒了（`byte_table::choose`：那台机器不要这份 / 这一版没带）。
    Refused(crate::byte_table::Refusal),
    /// 做了但没做成，或判清了不该做（配置、链路、那台上的东西不肯说自己是谁……）—— 一句说清楚的话。
    Failed(String),
}

impl DeployError {
    /// 对用户说的那一句（`machine` = 机器名）。
    pub fn say(&self, machine: &str) -> String {
        match self {
            DeployError::Refused(r) => r.say(crate::byte_table::Product::Backend, machine),
            DeployError::Failed(why) => why.clone(),
        }
    }
}

impl From<String> for DeployError {
    fn from(why: String) -> Self {
        DeployError::Failed(why)
    }
}

/// 〔E2 · V28 · `设计/01 §6.7b`〕后端的落点：SFTP 那一侧（家目录相对）· shell 里 · 给人看的。**本机与远端同一个**，常量住 `relay_route_core`。
pub(crate) const LANDING_REL: &str = relay_route_core::BACKEND_LANDING_REL;
const LANDING_SHOWN: &str = "~/.cc-monitor/bin/ccm";

/// 〔E2 · E-c〕旧默认 `backendPath` 落下的那份后端字节（`backendPath` 那一格删了之后没人再用它）。
pub(crate) const LEGACY_BACKEND_REL: &str = ".cc-monitor/bin/cc-monitor-backend";
const LEGACY_BACKEND_WORD: &str = "\"$HOME\"/.cc-monitor/bin/cc-monitor-backend";

/// 〔E2〕落点那一份怎么办：先按身份戳判（[`identity_decision`]）；「不说自己是谁」时再看它是不是我们从前放的
/// 三行入口（`ccm_legacy::is_ours`，读它的字节）—— 是 ⇒ 换成后端本体（部署那一步是原子替换）；不是 ⇒ 照旧显式失败、不动。
/// 为什么不先 `files-delete`：部署在连上之前，那台的后端还够不着；旧入口转给旧后端的 `ccm` 模式，连它本身都连不上。
async fn landing_decision(
    cfg: &RemoteConfig,
    fs: &RemoteFs,
    expected: &str,
) -> Result<DeployAction, String> {
    let id = remote_identity(cfg, fs, LANDING_REL, crate::ssh_source::BACKEND_CMD).await?;
    if matches!(id, RemoteIdentity::NoStamp) {
        let old = read_marker(fs, LANDING_REL).await.ok().flatten();
        if old.is_some_and(|b| crate::ccm_legacy::is_ours(&String::from_utf8_lossy(&b))) {
            return Ok(DeployAction::Deploy(copy_text(
                "rsSftp.identity.oldEntry",
                &[],
            )));
        }
    }
    identity_decision(&id, expected, &cfg.origin_label(), LANDING_SHOWN)
}

/// 〔E2 · E-c〕旧落点那份后端字节：身份戳恰一个（是我们编的）⇒ 删；不在 ⇒ 不说话；别的 ⇒ 不动、说一句为什么。
/// 部署时与每次连上（预检）各走一次。回「要对人说的那一句」（空 = 没东西）。
pub(crate) async fn sweep_legacy_backend(cfg: &RemoteConfig, fs: &RemoteFs) -> String {
    let shown = format!("~/{LEGACY_BACKEND_REL}");
    match remote_identity(cfg, fs, LEGACY_BACKEND_REL, LEGACY_BACKEND_WORD).await {
        Ok(RemoteIdentity::Missing) => String::new(),
        Ok(RemoteIdentity::Stamp(_)) => match fs.remove(LEGACY_BACKEND_REL).await {
            Ok(_) => copy_text("rsSftp.legacyBackend.removed", &[("rel", &shown)]),
            Err(e) => copy_text("rsSftp.legacyBackend.failed", &[("rel", &shown), ("e", &e)]),
        },
        Ok(_) => copy_text("rsSftp.legacyBackend.kept", &[("rel", &shown)]),
        Err(e) => copy_text("rsSftp.legacyBackend.failed", &[("rel", &shown), ("e", &e)]),
    }
}

/// 连接前确保远端后端已（自动）部署到固定落点 `~/.cc-monitor/bin/ccm`（issue #29；〔E2〕那个文件就是后端本身）。
///
/// 流程：① 〔DP1〕问远端是什么机器、查表选内嵌二进制（[`remote_backend_binary`]）——表拒绝则 [`DeployError::Refused`]；
/// ② 开 SFTP、读落点那一份的身份（[`landing_decision`]）、需要则 mkdir -p + 原子上传；③ 〔E2〕清旧落点那份字节。
///
/// **不阻断**：调用方（ssh_source::run）拿到 `Err` 仍接着试连已有后端（手动部署的后端照样能连），
/// 但〔DP1〕那句话经远端健康通道（`kind = "deploy"`）发到界面上，不再只是一行日志（`设计/96 §7.1.4` 第 2 条）。
/// 返回值：`Ok(build_id)` = 已**确认**远端后端就是手上这份字节（部署成功或已是这一版）。调用方据此决定
/// 是否传新版才认识的流模式参数（如 `--with-bg`）——`Err` 一律降级不传，
/// 避免旧后端把未知参数当一次性查询处理后退出（无 hello 死循环）。
pub async fn ensure_backend_deployed(cfg: &RemoteConfig) -> Result<String, DeployError> {
    // 〔DP1〕先问那台是什么机器、再查表；表拒绝 ⇒ `Refused`（那句话由 `byte_table::Refusal::say` 说）。
    let bin = match remote_backend_binary(cfg).await {
        Ok(Ok(b)) => b,
        Ok(Err(refusal)) => return Err(DeployError::Refused(refusal)),
        Err(e) => {
            return Err(DeployError::Failed(copy_text(
                "rsSftp.deploy.unameFailed",
                &[
                    ("machine", &(cfg.origin_label()).to_string()),
                    ("e", &e.to_string()),
                ],
            )))
        }
    };
    // 🔴 `K-R70`：**把这几 MB 字节推到别人机器上之前，先让它自己说一遍它是谁。**
    //   戳是一段 `#[used] static [u8; N]`，连续、拆不成立即数（backend 侧 `CC_MONITOR_BUILD_STAMP`）；判据无条件跑。
    if !bytes_carry_build_stamp(bin.bytes, bin.build_id) {
        tracing::warn!(
            "内嵌后端的字节里问不出 `{}` 这个身份戳——按身份未知不推\
             （这份字节不是这套源码编出来的，或它太旧、还没有身份戳；重跑 zigbuild 重铺）",
            bin.build_id
        );
        return Err(DeployError::Failed(copy_text(
            "rsSftp.deploy.noBuildId",
            &[],
        )));
    }
    // 〔SR1b〕经本机常驻后端那条 `files` 链路（写只许 `~/.cc-monitor/bin/` 与暂存区）。
    let fs = RemoteFs::open(cfg).await?;

    // 〔DP1〕那台上那一份是谁：读它字节里的身份戳（不跑它）；判不了 / 它不肯说 ⇒ 显式失败、一个字节都不写。
    let theirs = match landing_decision(cfg, &fs, bin.build_id).await? {
        // 〔HX2 · D-b〕不比这一版旧 ⇒ 一个字节不写、照旧连上那一份；回**那台上的**身份（不是这一版的 ——
        //   否则调用方的乐观路径会拿这一版内嵌的能力常量去发 flag），能力由那一份的 hello 自报。
        DeployAction::Keep { theirs, why } => {
            tracing::info!("远端 [{}] 不部署：{why}", cfg.origin_label());
            Some(theirs)
        }
        DeployAction::Skip => {
            tracing::info!(
                "远端 [{}] backend 已是 {}，跳过部署",
                cfg.origin_label(),
                bin.build_id
            );
            None
        }
        DeployAction::Deploy(reason) => {
            tracing::info!(
                "远端 [{}] 自动部署后端（{reason}）→ {LANDING_SHOWN}",
                cfg.origin_label(),
            );
            fs.mkdirs(remote_parent(LANDING_REL)).await?;
            upload_verified(&fs, LANDING_REL, bin.bytes, 0o700).await?;
            tracing::info!(
                "远端 [{}] backend 部署完成：{}",
                cfg.origin_label(),
                bin.build_id
            );
            None
        }
    };
    // 〔E2 · E-c〕每次连上（预检）都扫一次旧落点；结局只进日志，不挡连接。
    let swept = sweep_legacy_backend(cfg, &fs).await;
    if !swept.is_empty() {
        tracing::info!("远端 [{}] {swept}", cfg.origin_label());
    }
    Ok(theirs.unwrap_or_else(|| bin.build_id.to_string()))
}

/// 朴素子串搜索（8MB × 16B 一次性毫秒级；不为此引 memchr 依赖）。
fn bytes_contain(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// 🔴 `K-R70`：**这份字节自己说得出它是 `build_id` 吗** —— 不看它旁边任何文件。
///
/// 找的是后端那侧那段 `#[used] static CC_MONITOR_BUILD_STAMP`：
/// `<开>` ＋ `BUILD_ID` ＋ `<关>`，两个界标的**唯一住址**在
/// `src/backend/main.rs`（`BUILD_STAMP_OPEN` / `BUILD_STAMP_CLOSE`），
/// 由 `build.rs` 抠出来经 `BACKEND_STAMP_OPEN` / `BACKEND_STAMP_CLOSE` 交到这里
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
        env!("BACKEND_STAMP_OPEN"),
        env!("BACKEND_STAMP_CLOSE")
    );
    bytes_contain(bytes, stamp.as_bytes())
}

// 〔DP1 · 第四波〕这里原来是按 arch 取字节的那个函数：两份 musl 的 `include_bytes!` 与一个只认 arch 的 `match`。
//   槽与它们的 `K-R70` 身份取值口（`BACKEND_EMBEDDED_ID_<ARCH>`）逐字搬进了 `byte_table.rs`（全仓唯一的取字节口）。

// ============================================================================
// F08c：手动安装 / 卸载后端（设置面板两个按钮）。安装逻辑同自动部署、但返回人读结果；
// 卸载删落点那个文件（〔E2〕固定落点 `~/.cc-monitor/bin/ccm`，没有外来路径要守）。
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

/// 手动安装 / 更新远端后端（机器页 ①「部署后端」按钮）。逻辑同自动部署 [`ensure_backend_deployed`]，
/// 但**返回人读结果**，且把自动部署里「优雅跳过」的几种情况（探测不到 arch / 无该 arch 内嵌）显式报错。
/// 〔E2 · V28〕落点就是 `~/.cc-monitor/bin/ccm`（后端本体，没有 shim）⇒ 部署后端就是放 `ccm`，没有第二样要放。
#[tauri::command]
pub async fn deploy_remote_backend(cfg: RemoteConfig) -> Result<String, String> {
    // 〔DP1〕与自动部署同一个取字节口、同一句拒绝的话。
    let bin = match remote_backend_binary(&cfg).await? {
        Ok(b) => b,
        Err(refusal) => {
            return Err(refusal.say(crate::byte_table::Product::Backend, &cfg.origin_label()))
        }
    };
    // 〔SR1b〕经本机常驻后端那条 `files` 链路。
    let fs = RemoteFs::open(&cfg).await?;
    // 〔DP1〕与自动部署同一条判定：读那台上那一份字节自报的身份，不读旁挂标记。
    let backend_msg = match landing_decision(&cfg, &fs, bin.build_id).await? {
        // 〔HX2 · D-b〕手动点也不降级：出路与「它不说自己是谁」那一格同一句（先卸载再部署 = 明确授权覆盖）。
        DeployAction::Keep { why, .. } => copy_text("rsSftp.deploy.keptNotOlder", &[("why", &why)]),
        DeployAction::Skip => copy_text(
            "rsSftp.deploy.upToDate",
            &[
                ("buildId", &bin.build_id.to_string()),
                ("machine", &bin.machine.to_string()),
                ("path", &LANDING_SHOWN.to_string()),
            ],
        ),
        DeployAction::Deploy(reason) => {
            fs.mkdirs(remote_parent(LANDING_REL)).await?;
            upload_verified(&fs, LANDING_REL, bin.bytes, 0o700).await?;
            tracing::info!(
                "远端 [{}] 手动部署后端完成：{}",
                cfg.origin_label(),
                bin.build_id
            );
            copy_text(
                "rsSftp.deploy.done",
                &[
                    ("buildId", &bin.build_id.to_string()),
                    ("machine", &bin.machine.to_string()),
                    ("path", &LANDING_SHOWN.to_string()),
                    ("reason", &reason.to_string()),
                ],
            )
        }
    };
    // 〔E2 · E-c〕旧落点那份后端字节（认出是我们编的才删）。
    let swept = sweep_legacy_backend(&cfg, &fs).await;
    // 〔GP1 · 第四波〕`设计/01 §6.7b` 迁移 ② ③：旧版放在 `~/.local/bin/ccm` 的那一份，认出是我们放的就删
    //   （经那台的后端、带 CAS；那一格在 SFTP 两个写根之外）。没东西 ⇒ 不多说一句；查不成 ⇒ 说出来，不挡部署。
    let door = crate::user_files::BackendDoor::new(crate::origin::Origin(cfg.origin_label()));
    let legacy = match crate::ccm_legacy::sweep(&door).await {
        Ok(s) => s.say(),
        Err(e) => copy_text(
            "rsSftp.ccmLegacy.checkFailed",
            &[
                ("rel", &crate::ccm_legacy::LEGACY_REL.to_string()),
                ("e", &e.to_string()),
            ],
        ),
    };
    Ok(format!("{backend_msg}{swept}{legacy}"))
}

/// 卸载远端后端（设置面板「卸载后端」按钮）：删落点那个文件（〔E2〕它就是 `ccm`，卸后端就是卸 `ccm`）。
/// 只读铁律豁免（SS-G）：用户显式触发的删。注意：若该机器仍启用，自动部署会在下次连接重新装回——提示见返回消息。
#[tauri::command]
pub async fn uninstall_remote_backend(cfg: RemoteConfig) -> Result<String, String> {
    // 〔SR1b〕经本机常驻后端那条 `files` 链路删（写只许 `~/.cc-monitor/bin/` 与暂存区 —— 围栏拒 ⇒ 原话带回）。
    let fs = RemoteFs::open(&cfg).await?;
    let removed = fs.remove(LANDING_REL).await?;
    tracing::info!(
        "远端 [{}] 卸载后端：{LANDING_SHOWN} {}",
        cfg.origin_label(),
        if removed { "已删" } else { "本来就不在" }
    );
    let path = LANDING_SHOWN.to_string();
    if removed {
        Ok(copy_text("rsSftp.uninstall.done", &[("path", &path)]))
    } else {
        Ok(copy_text("rsSftp.uninstall.absent", &[("path", &path)]))
    }
}

// ============================================================================
// F11：远端用户数据写（删除远端历史 jsonl）。
// 〔RW1 · 第四波 · 2026-09-24〕**这一段整个搬走了**：F11 按用户裁「按推荐改」经那台远端的后端删
// （`files-delete-session`，只收 sid —— 〔AR1 · V119〕当时说「会话文件围栏唯一的例外」，FN1 之后写面已无那道围栏；
// 落点由远端后端按 sid 在它自己的记录树里找），
// 从前这里那道结构守卫 `is_safe_remote_jsonl`〔散文墓碑〕与 SFTP 直删 `remove_remote_file`〔散文墓碑〕零调用方 ⇒ 删了。
// 「哪几份才许删」那一问的住址从此是 `src/backend/agents/claudecode/paths.rs::session_file_for_delete`。
// ============================================================================

// 〔W5-ALIAS · 第五波先行〕F10（别名块）这一段搬去了 `profile_installer.rs`（B §2 第 12 条：本文件已经不做 SFTP，
//   别名块的真相 —— 围栏那一对 · 块的内容 · 自带的名字 · 合 / 剥 · 远端装 / 卸两条命令 —— 归别名域）。
//   本文件只剩部署：后端字节 · 身份 · 读回 · `ccm` 入口。

// 〔E2 · V28 · `设计/01 §6.7b`〕这里原先是 `ccm` 入口那一段（三行 shim 的推送口 `put_ccm_entry`〔散文墓碑〕、落点常量
//   `CCM_CLI_REMOTE_PATH`〔散文墓碑〕）：`ccm` 就是后端本身之后，落点 `~/.cc-monitor/bin/ccm` 上放的就是后端字节（[`LANDING_REL`]），
//   没有第二样要放。`K-R48` 那句墓碑（打包 bash 启动器的 `include_str!` 已删，`KR48D1` 盯着）照旧成立。

#[cfg(test)]
#[path = "../../../tests/bridge/sftp_tests.rs"]
mod tests;
