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
//! - **F10**：别名块装/卸——**本模块 [`install_remote_alias_block`]/[`uninstall_remote_alias_block`] 写远端 `~/.bashrc`**
//!   （〔MC1〕从前这一对叫 `install_remote_ccm_helper`〔散文墓碑〕/ `uninstall_…`，推入口那一半并进了 [`deploy_remote_backend`]）
//!   （BEGIN/END 块 + 备份 + 写后校验回滚）；本机 profile 写在 `profile_installer`。（batch20 审计修：原「非远端」措辞误——本模块确写远端 `~/.bashrc`。）
//!   〔RW1 · 第四波 09-24〕**落盘已不在本模块**：两条命令经远端后端读改写（`user_files`），本模块只剩规划那一半
//!   （[`merge_profile_block`] / [`strip_profile_block`]）。
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

use std::time::{SystemTime, UNIX_EPOCH};

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
        return Err(format!(
            "上传后读不回 {path}——无法确认写对了。已中止，未写入版本标记（下次会重新部署）。"
        ));
    };
    if got_len == expected_len {
        let Some(at) = first_diff else {
            return Ok(());
        };
        return Err(format!(
            "上传后校验失败：{path} 长度相同（{expected_len} 字节）但内容不同，首个差异在第 {at} 字节。\
             这类损坏（传输截断后补齐 / 编码变形）只比长度是查不出来的。\
             已中止，未写入版本标记（下次会重新部署）。"
        ));
    }
    Err(format!(
        "上传后校验失败：{path} 长度不匹配（期望 {expected_len} 字节，读回 {got_len} 字节）。\
         已中止，未写入版本标记（下次会重新部署）。"
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
pub(crate) async fn upload_verified(
    fs: &RemoteFs,
    remote_path: &str,
    bytes: &[u8],
    mode: u32,
) -> Result<(), String> {
    let back = fs.put(remote_path, bytes, mode, true).await?;
    verify_readback(remote_path, bytes.len() as u64, back)
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
///    整套「备份 + 读回 + 回滚」为这次损坏出具合格证。[`verify_readback`] 的头注
///    自己写着"按字节而不是按字符串"，那条纪律只落到了后端二进制那条路。
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

// 〔AL1 · 2026-09-24〕这里原来是 `rollback_note`〔散文墓碑〕（「回滚措辞必须与实际发生的事一致」）。
// 它记的那条未收项 ——「首次安装失败就删掉新建的文件是行为新增（要在远端 `remove`），不在验收轮里做」——
// 在 `fenced_block::apply` 里收了：原本不存在的文件写坏了就 `Store::delete_created`，措辞由
// `fenced_block::undo_note` 按**真发生了的事**说，本机远端同一份。

/// [`interpret_profile_read`] 的取样：〔SR1b〕一问（`RemoteFs::read`）带回三样 —— 字节 ·
/// 读不出时补问的「在不在」· 读到空时补问的大小（后端**只在需要时**补问，不为常见路径多加往返）。
async fn read_profile_text(
    fs: &RemoteFs,
    path: &str,
    what: &str,
) -> Result<Option<String>, String> {
    let (bytes, exists, size) = fs.read(path, MARKER_READ_MAX).await?;
    interpret_profile_read(what, bytes.as_deref(), exists, size)
}

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
}

/// 比对远端版本标记与期望 build_id，决定是否（重）部署。
///
/// ⚠ **这个函数只回答「版本对不对」一件事**，它的入参里根本没有落点那个文件
/// ——「那个文件在不在」由 [`TargetBinary`] 单独取样、在 [`deploy_decision_at`] 里
/// 与本判定合并。backend 那条路**只许走 `deploy_decision_at`**（见它的头注）；
/// 本函数留给 `acct_iso_deploy` 那条按目录取标记的路，那里标记与内容同一次上传、
/// 且落点是目录不是单个文件。
pub fn deploy_decision(remote_build_id: Option<&str>, expected: &str) -> DeployAction {
    match remote_build_id {
        None => DeployAction::Deploy("远端无 backend / 无版本标记".to_string()),
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
    /// stat 说它在，但是 **0 字节** —— 不是假想形态：原子上传那一段（今天住后端 `dial/sftp.rs::put_atomic`）里
    /// 「绝不 set_metadata」那条注释记的就是真机 e2e 实测把后端截成 0 字节、
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
/// 已部署且 build 未变的机器上，二进制被删 / 被截成 0 字节 / `backendPath` 被改到
/// 同目录另一个文件名，标记照旧匹配 ⇒ 判 `Skip` ⇒ 新的字节**永远不会上传**，
/// 而 exec 走的是那个不存在的路径。
///
/// ⚠ **那时用户看到什么，逐字**（`ssh_source.rs` 的连接面）：
/// 「SSH 连上了，但后端在超时内未回 hello（未部署 / 路径错 / 启动失败？）。」
/// —— 一个**三选一的猜测**。★ 病灶正在这里：**手里握着一条 SFTP 会话、能一问就知道
/// 那个文件在不在的这一层，什么都没说**；而要去猜的是**够不着那个事实**的那一层。
/// 本函数买的就是让前一层把它知道的那半句说出来。
///
/// ⚠ **本条没实测过的部分**：上面那句是从源码摘的逐字串（住址在 `ssh_source.rs` 里
/// `未回 hello` 那一处），**不是**真机跑出来的截图 —— 本轮不碰真远端。
///
/// 本模块此前只断掉了这条链的**上传那一段**（[`upload_verified`] 的头注：
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
            "落点没有后端二进制（{}）",
            marker_phrase(remote_build_id, expected)
        )),
        TargetBinary::Empty => DeployAction::Deploy(format!(
            "落点的后端二进制是 0 字节（{}）",
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

/// 版本标记文件路径：backend 二进制同目录下 `.build_id`。
///
/// ⚠ **目录级** —— 路径里不带二进制名。所以它认不出「同目录里换了个文件名」，
/// 那半个事实由 [`probe_target_binary`] 单独取样（K-W4 `§0c`）。
fn marker_path(backend_path: &str) -> String {
    let dir = remote_parent(backend_path);
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
/// `metadata` 失败才补问 `try_exists`（〔SR1b〕这一问住后端 `stat`，一趟带回两样）：要区分「明确不在」与
/// 「问不出来」，而这两者在 `metadata` 的 `Err` 里长得一模一样。链路本身坏了（问都没问出去）⇒ `Err`，不是 `Unknown`。
async fn probe_target_binary(fs: &RemoteFs, path: &str) -> Result<TargetBinary, String> {
    let (metadata_size, exists) = fs.stat(path).await?;
    Ok(interpret_target_probe(metadata_size, exists))
}

/// 连接前确保远端后端已（自动）部署到 `cfg.backend_path`（issue #29）。
///
/// 流程：① S-2 守卫（backend_path 含 `~` → 跳过，SFTP 不展开 `~`）；② 〔DP1〕问远端是什么机器、查表选内嵌
/// 二进制（[`remote_backend_binary`]）——表拒绝则说那句拒绝的话；
/// ③ 开 SFTP、读版本标记、[`deploy_decision`]、需要则 mkdir -p + 原子上传 + 写标记。
///
/// **best-effort**：调用方（ssh_source::run）对 Err 仅 warn 不阻断——手动部署的后端仍可连。
/// 返回值（Batch7-F24）：`Ok(Some(build_id))` = 已**确认**远端后端版本
/// （Deploy 成功或 Skip-版本相符）；`Ok(None)` = 无法确认（`~` 路径 / arch 探测失败 /
/// 无内嵌二进制等 no-op 路径——手动部署的后端，版本未知）。调用方据此决定
/// 是否传新版才认识的流模式参数（如 `--with-bg`）——未确认一律降级不传，
/// 避免旧后端把未知参数当一次性查询处理后退出（无 hello 死循环）。
pub async fn ensure_backend_deployed(cfg: &RemoteConfig) -> Result<Option<String>, String> {
    // S-2（审计）：SFTP 无 shell 不展开 `~`，而 backend exec 路径会展开——backend_path 含 `~`
    // 会两边错位。含 `~` 直接跳过自动部署（用户应填绝对路径），手动部署的后端仍可连。
    if cfg.backend_path.contains('~') {
        tracing::debug!(
            "backend_path 含 ~（SFTP 不展开），跳过自动部署：{}",
            cfg.backend_path
        );
        return Ok(None);
    }
    // 〔DP1〕先问那台是什么机器、再查表；表拒绝 ⇒ 那句话（`byte_table::Refusal::say`）。
    let bin = match remote_backend_binary(cfg).await {
        Ok(Ok(b)) => b,
        Ok(Err(refusal)) => {
            tracing::warn!("{}", refusal.say(&cfg.origin_label()));
            return Ok(None);
        }
        Err(e) => {
            tracing::debug!("问远端是什么机器没问成，跳过自动部署: {e}");
            return Ok(None);
        }
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
    // 连续、拆不成立即数（backend 侧 `CC_MONITOR_BUILD_STAMP`）。
    if !bytes_carry_build_stamp(bin.bytes, bin.build_id) {
        tracing::warn!(
            "内嵌后端的字节里问不出 `{}` 这个身份戳——按身份未知跳过自动部署\
             （这份字节不是这套源码编出来的，或它太旧、还没有身份戳；重跑 zigbuild 重铺）",
            bin.build_id
        );
        return Ok(None);
    }
    // 〔SR1b〕经本机常驻后端那条 `files` 链路（写只许 `~/.cc-monitor/bin/` 与暂存区；`backend_path` 不在
    //   `~/.cc-monitor/bin/` 下 ⇒ 后端围栏拒，这里原话往上报 —— 调用方对 Err 只 warn，手动部署的后端照旧能连）。
    let fs = RemoteFs::open(cfg).await?;

    let marker = marker_path(&cfg.backend_path);
    let remote_id = read_marker(&fs, &marker)
        .await?
        .map(|b| String::from_utf8_lossy(&b).trim().to_string());
    // K-W4 §0c：标记是目录级的，光凭它判 Skip 会在「标记还在、二进制没了」时静默跳过。
    let target = probe_target_binary(&fs, &cfg.backend_path).await?;

    match deploy_decision_at(remote_id.as_deref(), bin.build_id, target) {
        DeployAction::Skip => {
            tracing::info!(
                "远端 [{}] backend 已是 {}，跳过部署",
                cfg.origin_label(),
                bin.build_id
            );
        }
        DeployAction::Deploy(reason) => {
            tracing::info!(
                "远端 [{}] 自动部署后端（{reason}）→ {}",
                cfg.origin_label(),
                cfg.backend_path
            );
            fs.mkdirs(remote_parent(&cfg.backend_path)).await?;
            upload_verified(&fs, &cfg.backend_path, bin.bytes, 0o700).await?;
            put_marker(&fs, &marker, bin.build_id.as_bytes(), 0o600).await?;
            tracing::info!(
                "远端 [{}] backend 部署完成：{}",
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
// 卸载删后端二进制 + 同目录 .build_id（is_safe_remote_backend_path 守卫）。
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

/// 远端后端路径安全守卫（卸载用，纯函数可单测）：绝对、无 `..`、非根、且含 `cc-monitor`
/// （约定 `~/.cc-monitor/bin/cc-monitor-backend`）—— 杜绝把卸载误用成删任意远端文件。
fn is_safe_remote_backend_path(path: &str) -> bool {
    is_safe_remote_managed_path(path, &["cc-monitor"])
}

/// 手动安装 / 更新远端后端（机器页 ①「部署后端」按钮）。逻辑同自动部署
/// [`ensure_backend_deployed`]，但**返回人读结果**，且把自动部署里「优雅跳过」的几种情况
/// （路径含 `~` / 探测不到 arch / 无该 arch 内嵌）显式报错——手动触发时用户要反馈。
#[tauri::command]
pub async fn deploy_remote_backend(cfg: RemoteConfig) -> Result<String, String> {
    let path = cfg.backend_path.trim().to_string();
    if path.is_empty() {
        return Err(
            "请先填后端路径（绝对路径，如 /home/<user>/.cc-monitor/bin/cc-monitor-backend）".into(),
        );
    }
    if path.contains('~') {
        return Err("backend 路径含 ~（SFTP 不展开 ~），请改用绝对路径".into());
    }
    // 〔DP1〕与自动部署同一个取字节口、同一句拒绝的话。
    let bin = match remote_backend_binary(&cfg).await? {
        Ok(b) => b,
        Err(refusal) => return Err(refusal.say(&cfg.origin_label())),
    };
    // 〔SR1b〕经本机常驻后端那条 `files` 链路；`path` 不在 `~/.cc-monitor/bin/` 下 ⇒ 后端围栏拒、原话带回。
    let fs = RemoteFs::open(&cfg).await?;
    let marker = marker_path(&path);
    let remote_id = read_marker(&fs, &marker)
        .await?
        .map(|b| String::from_utf8_lossy(&b).trim().to_string());
    // K-W4 §0c：手动「安装后端」按钮此前也只看标记 —— 落点文件被删/截断时，
    // 它会对着一个不存在的文件回「已是最新，无需重装」。同一条病，同一处修法。
    let target = probe_target_binary(&fs, &path).await?;
    let backend_msg = match deploy_decision_at(remote_id.as_deref(), bin.build_id, target) {
        DeployAction::Skip => format!(
            "远端已是最新后端（{}，{}）：{path}，无需重装。",
            bin.build_id, bin.machine
        ),
        DeployAction::Deploy(reason) => {
            fs.mkdirs(remote_parent(&path)).await?;
            upload_verified(&fs, &path, bin.bytes, 0o700).await?;
            put_marker(&fs, &marker, bin.build_id.as_bytes(), 0o600).await?;
            tracing::info!(
                "远端 [{}] 手动部署后端完成：{}",
                cfg.origin_label(),
                bin.build_id
            );
            format!(
                "已安装后端（{}，{}）到 {path}（{reason}）。重连远端即可用。",
                bin.build_id, bin.machine
            )
        }
    };
    // 〔MC1 · 2026-09-24〕`设计/71 §13.3` ①：**部署后端只有一个动作** —— 后端本体 ＋ `ccm` 入口
    //   一起放（从前入口住「装 ccm 启动器」那颗按钮里，要点两次）。后端先、入口后：入口转发给后端，
    //   后端没就位时放入口等于给一条当场报错的命令。**只在这颗按钮上放**，连接时的自动部署
    //   （[`ensure_backend_deployed`]）不碰入口 —— 那是用户点了才发生的事。
    //   〔SR1b〕入口落在 `~/.cc-monitor/bin/ccm`（两个写根之内；`设计/01 §6.7b` 的落点）。
    let entry = match put_ccm_entry(&fs, &path).await? {
        crate::fenced_block::Applied::Unchanged => "终端里的 ccm 入口已就位。",
        crate::fenced_block::Applied::Written { .. } => {
            "终端里的 ccm 入口已放好（~/.cc-monitor/bin/ccm；装了别名块的终端里直接能用）。"
        }
    };
    Ok(format!("{backend_msg}{entry}"))
}

/// 卸载远端后端（设置面板「卸载后端」按钮）：删后端二进制 + 同目录 `.build_id`。
/// [`is_safe_remote_backend_path`] 守卫。只读铁律豁免（SS-G）：用户显式触发的删。
/// 注意：若该机器仍启用，自动部署会在下次连接重新装回——提示见返回消息。
#[tauri::command]
pub async fn uninstall_remote_backend(cfg: RemoteConfig) -> Result<String, String> {
    let path = cfg.backend_path.trim().to_string();
    if path.contains('~') {
        return Err("backend 路径含 ~（SFTP 不展开），请改用绝对路径后再卸载".into());
    }
    if !is_safe_remote_backend_path(&path) {
        return Err(format!(
            "拒绝删除可疑后端路径（须为含 cc-monitor 的绝对路径、无 ..）: {path}"
        ));
    }
    // 〔SR1b〕经本机常驻后端那条 `files` 链路删（写只许 `~/.cc-monitor/bin/` 与暂存区 —— 围栏拒 ⇒ 原话带回）。
    let fs = RemoteFs::open(&cfg).await?;
    let marker = marker_path(&path);
    let mut removed = Vec::new();
    for f in [path.clone(), marker.clone()] {
        if fs.remove(&f).await? {
            removed.push(f);
        }
    }
    tracing::info!("远端 [{}] 卸载后端：删除 {removed:?}", cfg.origin_label());
    if removed.is_empty() {
        Ok(format!(
            "没有可删的后端文件（{path} 及其 .build_id 都不在，可能已卸载）。"
        ))
    } else {
        Ok(format!(
            "已删除 {} 个文件：{}。注意：若本机器仍勾选「启用」，自动部署会在下次连接时把后端装回——彻底移除请取消该机器启用 / 删除该机器后重启 monitor。",
            removed.len(),
            removed.join("、")
        ))
    }
}

// ============================================================================
// F11：远端用户数据写（删除远端历史 jsonl）。
// 〔RW1 · 第四波 · 2026-09-24〕**这一段整个搬走了**：F11 按用户裁「按推荐改」经那台远端的后端删
// （`files-delete-session`，只收 sid —— 会话文件围栏唯一的例外，落点由远端后端按 sid 在它自己的记录树里找），
// 从前这里那道结构守卫 `is_safe_remote_jsonl`〔散文墓碑〕与 SFTP 直删 `remove_remote_file`〔散文墓碑〕零调用方 ⇒ 删了。
// 「哪几份才许删」那一问的住址从此是 `src/backend/agents/claudecode/paths.rs::session_file_for_delete`。
// ============================================================================

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
/// 注释行里那两条示例（`#   alphacc()  { … }`）靠「名字只许 `[A-Za-z0-9_]`」被剔掉 ——
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
///
/// 〔SR1b · 2026-09-24〕`.local/bin/ccm` → **`.cc-monitor/bin/ccm`**：远端写只许两处（V89），入口是部署物，
/// 落进部署那一根；这也正是 `设计/01 §6.7b`（用户 09-18「落点选 `~/.cc-monitor/bin`，两边尽量同形」）的落点，
/// 本机那一条早就在那儿。PATH：自带别名块（`src/shared/ccm-aliases.sh`）把 `~/.cc-monitor/bin` 排进去。
/// ⚠ 旧的 `~/.local/bin/ccm` **不删也不再更新**（那一格在两个写根之外）：它指的是同一个后端，照旧能用。
const CCM_CLI_REMOTE_PATH: &str = ".cc-monitor/bin/ccm";

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
    // 〔AL1 · 2026-09-24〕配对之后怎么拼，**也只剩一份**：`fenced_block::splice_in`（`71 §12.5`）。
    //   本函数只答「POSIX rc 里这一块长什么样」（方言的内容与围栏），不再自己切行拼接。
    let block = format!(
        "{CCM_PROFILE_BEGIN}\n{}\n{CCM_PROFILE_END}\n",
        snippet.trim()
    );
    crate::fenced_block::splice_in(
        existing,
        CCM_PROFILE_BEGIN,
        CCM_PROFILE_END,
        &block,
        what,
        crate::fenced_block::Layout::Posix,
    )
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
    // 〔AL1〕拼接走 `fenced_block::splice_out`（与装那一半同一份，`71 §12.5`）。
    crate::fenced_block::splice_out(
        existing,
        CCM_PROFILE_BEGIN,
        CCM_PROFILE_END,
        what,
        crate::fenced_block::Layout::Posix,
    )
}

/// 〔AL1 · 2026-09-24〕远端那一份原语（`fenced_block::Store`）。**规则不住这里** ——
/// 「读 → 备份 → 原子替换 → 回读比对 → 回滚」那一个序列是 `fenced_block::apply`，
/// 本机那一份原语是 `fenced_block::LocalFile`。这里只回答「这台远端上怎么做这四件事」。
///
/// 路径是远端 home 下的相对路径。读走 [`read_profile_text`]（fail-closed：
/// 读不出 / 非 UTF-8 / 有字节却读到空 一律 `Err`，理由在 [`interpret_profile_read`] 头注）。
/// 〔SR1b〕四件事都经本机常驻后端那条 `files` 链路（[`RemoteFs`]）；写只许两处 ⇒ 今天唯一的用户
/// （`ccm` 入口，`~/.cc-monitor/bin/ccm`）落在部署那一根里。〔墓碑 —— 从前它叫 `SftpFile`〔散文墓碑〕、手里拿一条 SFTP 会话。〕
pub(crate) struct RemoteFile<'a> {
    pub fs: &'a RemoteFs,
    pub path: String,
    /// 新写入时的权限位（可执行的入口是 `0o755`）。
    pub mode: u32,
    /// 给人看的名字（报错用），如「远端 ~/.cc-monitor/bin/ccm」。
    pub what: String,
}

impl crate::fenced_block::Store for RemoteFile<'_> {
    fn label(&self) -> String {
        self.what.clone()
    }

    async fn read(&self) -> Result<Option<String>, String> {
        read_profile_text(self.fs, &self.path, &self.what).await
    }

    async fn save_backup(&self, original: &str) -> Result<String, String> {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let backup = format!("{}.ccm-backup-{ms}", self.path);
        self.fs
            .put(&backup, original.as_bytes(), 0o600, false)
            .await?;
        Ok(backup)
    }

    async fn put_atomic(&self, content: &str) -> Result<(), String> {
        // 上级目录逐级建（相对 home；每一级都过后端那道围栏）。
        if let Some((parent, _)) = self.path.rsplit_once('/') {
            self.fs.mkdirs(parent).await?;
        }
        self.fs
            .put(&self.path, content.as_bytes(), self.mode, false)
            .await
            .map(|_| ())
    }

    async fn delete_created(&self) -> Result<(), String> {
        self.fs.remove(&self.path).await.map(|_| ())
    }
}

/// `profile` 只许是远端 home 下的一个文件名。空 ⇒ `.bashrc`。
fn remote_profile_name(profile: &str) -> Result<String, String> {
    let p = profile.trim();
    let p = if p.is_empty() { ".bashrc" } else { p };
    if p.contains('/') || p.contains('\\') || p.contains("..") {
        return Err("profile 只能是 home 下的文件名（如 .bashrc / .zshrc）".to_string());
    }
    Ok(p.to_string())
}

/// 〔MC1 · 2026-09-24〕**别名块**卸载（机器页 ②「别名」里的「卸载别名块」）：从远端 rc 删 BEGIN/END 块。
///
/// 从前它叫 `uninstall_remote_ccm_helper`〔散文墓碑〕、按钮叫「卸载 ccm」——「ccm 助手」这个词
/// 盖着两件事（`设计/71 §13.1`：① 推入口 ② 写别名块），而这一条只做过 ②。用户 2026-09-17 逐字
/// 「装/卸 ccm 助手是假的，删掉这个东西」⇒ 名字跟着它真做的事走。
/// 〔RW1〕读改写经那台远端的后端（`files-peek` / `files-put`）：没有块 ⇒ 一个字节都不写；否则
/// **先备份**（`.ccm-backup-<ms>-<序号>`）→ 写 → **读回逐字比对**，不符则回滚（规则住后端）。
#[tauri::command]
pub async fn uninstall_remote_alias_block(
    cfg: RemoteConfig,
    profile: String,
) -> Result<String, String> {
    let profile = remote_profile_name(&profile)?;
    // 〔RW1 · 第四波 09-24〕F10 按推荐改：**经那台远端的后端**写（`user_files`），不再 SFTP 直写 rc。
    //   备份 · 原子替换 · 回读 · 回滚那一份规则住后端（`files-put`），与本机同一条路、只差 origin。
    let door = crate::user_files::BackendDoor::new(crate::origin::Origin(cfg.origin_label()));
    let home = crate::user_files::Door::home(&door).await?;
    let what = format!("远端 ~/{profile}");
    let mut missing = false;
    let done =
        crate::user_files::edit(
            &door,
            &home,
            &profile,
            true,
            false,
            |existing| match existing {
                None => {
                    missing = true;
                    Ok(None)
                }
                Some(t) => strip_profile_block(t, &what).map(Some),
            },
        )
        .await?;
    let crate::user_files::Edited::Written(landed) = done else {
        return Ok(if missing {
            format!("远端 {profile} 不存在，没有别名块可卸载。")
        } else {
            format!("远端 {profile} 里没有别名块，不用卸载。")
        });
    };
    tracing::info!("远端 [{}] 已卸载别名块（{profile}）", cfg.origin_label());
    Ok(match landed.backup {
        Some(b) => format!("已从远端 {profile} 删掉别名块（原文件备份在 {b}）。"),
        None => format!("已从远端 {profile} 删掉别名块。"),
    })
}

/// 〔MC1 · 2026-09-24〕**别名块**装进远端 rc（机器页 ②「别名」里的「装别名块」）。
///
/// 从前它叫 `install_remote_ccm_helper`〔散文墓碑〕，一次做两件事：① 推 `ccm` 入口到
/// `~/.local/bin/ccm` ② 把别名块合进 rc。`设计/71 §13.3`：① 并进「部署后端」（本文件
/// [`deploy_remote_backend`]），② 并进「别名」⇒ 本函数只剩 ②。
///
/// `profile` 默认 `.bashrc`（相对远端后端的 home；拒 `/`、`\`、`..` 防写 home 外）。
/// 写入的 snippet 是**后端拥有**的 [`CCM_WRAPPER_SNIPPET`]（审计 S-1：不接受前端传入可执行
/// bash）。〔RW1〕读改写经那台远端的后端（与本机同一条路）：相同则不写；否则
/// 备份 → 原子写 → 读回逐字比对 → 不符回滚。别名块引用 `ccm` —— 那条入口由「部署后端」放。
///
/// 注：〔RW1〕替换沿用原文件的权限位（从前 SFTP 那一路统一写 `0o644`，`chmod 600` 的 rc 会被归一 —— 那一形没了）；
/// rc 是一条链接（dotfiles 仓）⇒ 改的是真文件，链接留着。
#[tauri::command]
pub async fn install_remote_alias_block(
    cfg: RemoteConfig,
    profile: String,
) -> Result<String, String> {
    let profile = remote_profile_name(&profile)?;
    // 〔RW1 · 第四波 09-24〕F10 按推荐改：经那台远端的后端写（同 `uninstall_remote_alias_block`）。
    // 损坏块 ⇒ `merge_profile_block` 回 `Err`，不动原文件。
    let door = crate::user_files::BackendDoor::new(crate::origin::Origin(cfg.origin_label()));
    let home = crate::user_files::Door::home(&door).await?;
    let what = format!("远端 ~/{profile}");
    let done = crate::user_files::edit(&door, &home, &profile, true, false, |existing| {
        merge_profile_block(existing.unwrap_or(""), CCM_WRAPPER_SNIPPET, &what).map(Some)
    })
    .await?;
    let crate::user_files::Edited::Written(landed) = done else {
        return Ok(format!("{profile} 里的别名块已是最新，没有改动。"));
    };
    let backup_note = landed
        .backup
        .map(|b| format!("（原文件备份在 {b}）"))
        .unwrap_or_default();
    tracing::info!("远端 [{}] 已装别名块到 {profile}", cfg.origin_label());
    Ok(format!(
        "别名块已写进 {profile}{backup_note}。重连 ssh 之后，终端里就有 cc / cct，\
         以及「别名」里生成的那几条。"
    ))
}

/// 〔MC1 · 2026-09-24〕把 `ccm` 入口放到远端（〔SR1b〕`~/.cc-monitor/bin/ccm`，[`deploy_remote_backend`] 的后半）。
///
/// 🔴 **它仍是那三行 shim**（[`ccm_entry_shim`]）。`设计/01 §6.7b` 的目标是「`ccm` 就是后端
/// 二进制本身、落 `~/.cc-monitor/bin/ccm`、没有 shim」—— 那一步卡在写区外：monitor 起远端后端的
/// 两处（`ssh_source.rs` 流模式与零参数探针）直接 exec `backend_path`，而后端 `main.rs` 头一件事就是
/// 「basename ＝ `ccm` ⇒ 进一次性 ccm 模式」⇒ 把后端文件名改成 `ccm`，探针那一发会变成
/// 「在当前目录起一个 agent」。改哪一侧都在本路写区外（摸底住 `tests/evidence/MC1-AL1-摸底.md`）。
/// ⇒ 本拍只做**界面与动作的归一**：装后端与放入口是**一个按钮、一次调用**，不再分成两颗。
async fn put_ccm_entry(
    fs: &RemoteFs,
    backend_path: &str,
) -> Result<crate::fenced_block::Applied, String> {
    let entry = RemoteFile {
        fs,
        path: CCM_CLI_REMOTE_PATH.to_string(),
        mode: 0o755,
        what: format!("远端 ~/{CCM_CLI_REMOTE_PATH}"),
    };
    // 走同一个序列：从前它写完只比对、**不回滚**（坏的入口会留在远端）；
    // 今天比对不上就恢复成原来那份（原来没有就删掉）。入口是我们自己的文件 ⇒ 不留备份。
    let shim = ccm_entry_shim(backend_path);
    crate::fenced_block::apply(&entry, false, |_| Ok(Some(shim))).await
}

#[cfg(test)]
#[path = "../../../tests/bridge/sftp_tests.rs"]
mod tests;
