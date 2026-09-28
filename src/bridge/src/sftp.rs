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
//! - **F89a**：用户**显式**增/改/删远端**项目** `.mcp.json`。〔RW1 · 第四波 09-24〕**写已不在本模块**；
//!   〔MIG-3a〕今天由那台后端自己算、自己写（帧命令 `mcp-server-put` / `-remove`，`mcp_edit.rs::project_root` 守落点）。
//!   **SS-14**：写面**只** `.mcp.json`，非 Claude 会话数据。
//! - **F10**：别名块装/卸——〔AL2 · 第四波 4D〕今天是 `lib.rs` 的 `aliases_block_install` / `aliases_block_remove`（带 `origin`，本机远端同一条）
//!   （〔MC1〕从前这一对叫 `install_remote_ccm_helper`〔散文墓碑〕/ `uninstall_…`，推入口那一半并进了 [`deploy_remote_backend`]）
//!   （BEGIN/END 块 + 备份 + 写后校验回滚）；本机 profile 写在 `profile_installer`。（batch20 审计修：原「非远端」措辞误——本模块确写远端 `~/.bashrc`。）
//!   〔RW1 · 第四波 09-24〕**落盘已不在本模块**：经那台后端读改写（`user_files`）；规划那一半
//!   （`merge_profile_block` / `strip_profile_block`）〔W5-ALIAS〕住 `profile_installer`。
//! - **F50**：追加公钥到远端 `~/.ssh/authorized_keys`〔MIG-3b 续〕今天是本机常驻后端的帧命令 `pubkey-push`（那台后端在就经它写 · 不在就一次 exec；不在本模块，登记于此备查）。
//!
//! 原子写（EXCL 临时件 → 旧目标先**改名成 `.bak`**（不是删）→ 上位 → 清 `.bak`）与它的来历住后端
//! `dial/sftp.rs::put_atomic`（F89a 审计后加固 · DN-7 订正「删旧」那句 · setstat 截断事故）。
//!
//! # 〔SR1b · 2026-09-24〕**本模块手里已经没有 SFTP 了**
//!
//! 用户 V89「SFTP 进本机常驻后端，只写暂存区」：SFTP 客户端住本机常驻后端（`src/backend/dial/sftp.rs`，
//! 与其它 SSH 同一条连接），**远端写只许两处**（`~/.cc-monitor/staging/` · `~/.cc-monitor/bin/`）。
//! 〔MIG-3b · 4d-lanes 子步 1〕**部署判定也不在本模块了**：该不该换 · 换成哪一格 · 落点那一份是谁由本机常驻后端出计划
//! （帧命令 `deploy-plan`，本体 `src/backend/control/deploy_plan.rs`，纯判定住共享 crate `deploy-core`）；
//! 本模块只**照计划放字节**（取这一版带着的那一格 · 身份戳自检 · mkdir · 原子上传 ＋ 读回判定 · 删旧落点），
//! 执行经 [`crate::dial_host::RemoteFs`]（本机后端那条 `files` 链路的一问一答）。〔墓碑 —— 从前本模块自己开 SFTP：`connect_sftp`〔散文墓碑〕在一条
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
/// `deploy_remote_backend` 的**全部**上传（〔MIG-3a · 09-28〕另一条 cc-acct-iso 那条随字节进后端退役）
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

// 〔MIG-3a · 09-28 预裁〕`read_marker` · `put_marker`〔散文墓碑〕与标记读上限随 `acct_iso_deploy` 删了（按目录取标记那条路的唯一消费者）。

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

/// 身份戳的两个界标（`build.rs` 从后端源码抠出来交进来；本文件不许出现那两个字面量）。本机那一份的身份（`local_backend.rs`）也用它。
pub(crate) const STAMP_MARKS: deploy_core::Marks<'static> = deploy_core::Marks {
    open: env!("BACKEND_STAMP_OPEN"),
    close: env!("BACKEND_STAMP_CLOSE"),
};

/// 〔MIG-3b〕部署决策住 `deploy-core`（本机常驻后端出计划用它）；本 crate 里还要它的几处从这里拿同一份名字。
pub use deploy_core::DeployAction;

// 〔MIG-3a · 09-28 预裁〕`deploy_decision`〔散文墓碑〕（比旁挂版本标记）删了：它只留给 `acct_iso_deploy` 那条按目录取标记的路，那条路整条退役
//   （字节随后端二进制走、逐份比内容，`src/backend/assets/acct_iso_install.rs`）。后端那条路的判定住 `deploy_core::identity_decision`。

/// 〔MIG-3b · 4d-lanes 子步 1〕**本机常驻后端出的部署计划**（帧命令 `deploy-plan`，线上形状 `tests/__fixtures__/deploy-plan.golden.json`）。
/// 该不该换 · 换成哪一格 · 落点那一份是谁 · 旧落点那份删不删 —— 全是后端判的；本模块只照它放字节。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    pub(crate) key: deploy_core::Key,
    /// 那一格这一版带着的字节自报的身份（后端拿它当对照物）。
    pub(crate) expected: String,
    pub(crate) action: DeployAction,
    pub(crate) legacy: deploy_core::LegacyVerdict,
    /// 〔MIG-3b 续 · VIS2〕问 `uname` 那一趟拨号的 ack（拨号在本机后端里）：逐地址指纹由 [`ask_plan_for`] 交给
    /// `dial_host::settle_host_key` 固化 —— 与 monitor 自己开链路那几条同一个判定，不另写。
    pub(crate) ack: crate::ssh_link::Ack,
}

/// `deploy-plan` 的应答 → [`Plan`]（**严格收**：少一格、多一格、认不出的值都是错 —— 两侧漂了要当场说出来）。
pub(crate) fn decode_plan(v: &serde_json::Value) -> Result<Plan, String> {
    const KEYS: [&str; 10] = [
        "ack",
        "action",
        "arch",
        "expected",
        "label",
        "legacy",
        "legacy_why",
        "os",
        "theirs",
        "why",
    ];
    let bad = || copy_text("rsSftp.plan.badProduct", &[("v", &v.to_string())]);
    let obj = v.as_object().ok_or_else(bad)?;
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    if keys != KEYS {
        return Err(bad());
    }
    let text = |k: &str| obj.get(k).and_then(serde_json::Value::as_str);
    let key = deploy_core::key_of(text("os").unwrap_or(""), text("arch").unwrap_or(""))
        .map_err(|_| bad())?;
    let why = text("why").ok_or_else(bad)?.to_string();
    let action = match (text("action"), text("theirs")) {
        (Some("skip"), None) => DeployAction::Skip,
        (Some("deploy"), None) => DeployAction::Deploy(why),
        (Some("keep"), Some(theirs)) => DeployAction::Keep {
            theirs: theirs.to_string(),
            why,
        },
        _ => return Err(bad()),
    };
    let legacy = match (text("legacy"), text("legacy_why")) {
        (Some("absent"), None) => deploy_core::LegacyVerdict::Absent,
        (Some("remove"), None) => deploy_core::LegacyVerdict::Remove,
        (Some("keep"), None) => deploy_core::LegacyVerdict::Keep,
        (Some("unknown"), Some(e)) => deploy_core::LegacyVerdict::Unknown(e.to_string()),
        _ => return Err(bad()),
    };
    let ack: crate::ssh_link::Ack = obj
        .get("ack")
        .cloned()
        .and_then(|a| serde_json::from_value(a).ok())
        .ok_or_else(bad)?;
    Ok(Plan {
        ack,
        key,
        expected: text("expected")
            .filter(|s| !s.is_empty())
            .ok_or_else(bad)?
            .to_string(),
        action,
        legacy,
    })
}

/// 问本机常驻后端要计划那一趟的上限：一次 `uname` ＋ 两次 stat ＋ 至多两次扫身份戳 ＋ 一次读回（每样一两个往返）。
const PLAN_BUDGET: std::time::Duration = std::time::Duration::from_secs(120);

/// 本机常驻后端那条命令的名字（与 `src/backend/inbound.rs::REGISTRY` 同名）。
pub(crate) const PLAN_CMD: &str = "deploy-plan";

/// 问本机常驻后端要一份计划。入参只有事实：怎么够到那台（拨号请求）· 这一版带着哪几格字节、各自自报的身份。
async fn ask_plan(cfg: &RemoteConfig) -> Result<Plan, String> {
    ask_plan_for(cfg, &crate::byte_table::carried_backends()).await
}

/// [`ask_plan`] 的本体，「带着哪几格」由调用方交（生产 = 这一版的槽；回环台架交它送去的那份字节的身份）。
async fn ask_plan_for(
    cfg: &RemoteConfig,
    carried: &[(deploy_core::Key, &str)],
) -> Result<Plan, String> {
    use crate::backend::control::backend_route::{route_call_error, Routed};
    let carried: Vec<serde_json::Value> = carried
        .iter()
        .map(|(k, id)| serde_json::json!({ "os": k.os.label(), "arch": k.arch.label(), "id": id }))
        .collect();
    let dial = crate::dial_host::transfer_dial(cfg)?;
    let args = serde_json::json!({
        "dial": dial,
        "carried": carried,
        "machine": cfg.origin_label(),
    });
    let client = crate::dial_host::local_backend_accepting(PLAN_CMD).await?;
    let data =
        client
            .call(PLAN_CMD, args, PLAN_BUDGET)
            .await
            .map_err(
                |e| match route_call_error(&e, |_code, message| message.to_string()) {
                    Routed::NoChannel(s) | Routed::Refused(s) => s,
                    Routed::Done => copy_text("rsSftp.plan.internal", &[]),
                },
            )?;
    let plan = decode_plan(&data.ok_or_else(|| copy_text("rsSftp.plan.internal", &[]))?)?;
    // 〔MIG-3b 续 · VIS2〕第一次连一台没钉过指纹的机器就在这一跳 ⇒ 照 monitor 自己开链路那几条同一个判定固化。
    crate::dial_host::settle_host_key(cfg, &dial, &plan.ack);
    Ok(plan)
}

/// 〔MIG-3b〕照计划取字节：那一格这一版带着的那一份（`byte_table::pick`）。计划说的身份与字节自报的对不上 ⇒ 两侧漂了，不推。
fn planned_binary(plan: &Plan) -> Result<BackendBinary, String> {
    crate::byte_table::pick(crate::byte_table::Product::Backend, plan.key)
        .and_then(|p| p.build_id.map(|id| (p.bytes, id)))
        .filter(|(_, id)| *id == plan.expected)
        .map(|(bytes, build_id)| BackendBinary {
            build_id,
            bytes,
            machine: plan.key.label(),
        })
        .ok_or_else(|| {
            copy_text(
                "rsSftp.plan.notThisSlot",
                &[("machine", &plan.key.label()), ("expected", &plan.expected)],
            )
        })
}

/// 〔DP1 · 第四波〕自动部署没成的两种说法 —— **类型上与「部署成功」分得开**（`设计/96 §7.1.4` 第 3 条：
/// 「返回类型上不许有『成功』这一支」）。〔墓碑 —— 从前是 `Result<Option<String>, String>`：`Ok(None)` 就是
/// 「没部署也算成功」那一支，路径含 `~` / 问不出 arch / 没这格字节 / 字节问不出身份全落在它上面、只留一行 `debug!`。〕
#[derive(Debug)]
pub enum DeployError {
    /// 那台要不了这份 / 这一版没带 / 判不清它是谁 —— 〔MIG-3b〕那句话是本机常驻后端说的（`deploy-plan` 失败那一形），原样带回。
    Refused(String),
    /// 做了但没做成（配置、链路、放字节那几步）—— 一句说清楚的话。
    Failed(String),
}

impl DeployError {
    /// 对用户说的那一句。
    pub fn say(&self) -> String {
        match self {
            DeployError::Refused(why) | DeployError::Failed(why) => why.clone(),
        }
    }
}

impl From<String> for DeployError {
    fn from(why: String) -> Self {
        DeployError::Failed(why)
    }
}

/// 〔E2 · V28 · `设计/01 §6.7b`〕后端的落点：SFTP 那一侧（家目录相对）· 给人看的。**本机与远端同一个**，常量住 `relay_route_core`。
pub(crate) const LANDING_REL: &str = relay_route_core::BACKEND_LANDING_REL;
const LANDING_SHOWN: &str = "~/.cc-monitor/bin/ccm";

/// 〔E2 · E-c〕旧落点那份后端字节：照计划删（后端认出身份戳恰一个 = 我们编的）· 不在 ⇒ 不说话 · 别的 ⇒ 不动、说一句为什么。
/// 回「要对人说的那一句」（空 = 没东西）。
async fn apply_legacy(verdict: &deploy_core::LegacyVerdict, fs: &RemoteFs) -> String {
    let shown = format!("~/{}", deploy_core::LEGACY_BACKEND_REL);
    match verdict {
        deploy_core::LegacyVerdict::Absent => String::new(),
        deploy_core::LegacyVerdict::Remove => {
            match fs.remove(deploy_core::LEGACY_BACKEND_REL).await {
                Ok(_) => copy_text("rsSftp.legacyBackend.removed", &[("rel", &shown)]),
                Err(e) => copy_text("rsSftp.legacyBackend.failed", &[("rel", &shown), ("e", &e)]),
            }
        }
        deploy_core::LegacyVerdict::Keep => {
            copy_text("rsSftp.legacyBackend.kept", &[("rel", &shown)])
        }
        deploy_core::LegacyVerdict::Unknown(e) => {
            copy_text("rsSftp.legacyBackend.failed", &[("rel", &shown), ("e", e)])
        }
    }
}

/// 连接前确保远端后端已（自动）部署到固定落点 `~/.cc-monitor/bin/ccm`（issue #29；〔E2〕那个文件就是后端本身）。
///
/// 流程：① 〔MIG-3b〕问本机常驻后端要计划（[`ask_plan`]：那台是什么机器、要哪一格、落点那一份是谁、换不换）——
/// 它拒了 ⇒ [`DeployError::Refused`]；② 照计划取字节（[`planned_binary`]）、需要则开 `files` 链路 mkdir -p + 原子上传；
/// ③ 〔E2〕照计划清旧落点那份字节。
///
/// **不阻断**：调用方（ssh_source::run）拿到 `Err` 仍接着试连已有后端（手动部署的后端照样能连），
/// 但〔DP1〕那句话经远端健康通道（`kind = "deploy"`）发到界面上，不再只是一行日志（`设计/96 §7.1.4` 第 2 条）。
/// 返回值：`Ok(build_id)` = 已**确认**远端后端就是手上这份字节（部署成功或已是这一版）。调用方据此决定
/// 是否传新版才认识的流模式参数（如 `--with-bg`）——`Err` 一律降级不传，
/// 避免旧后端把未知参数当一次性查询处理后退出（无 hello 死循环）。
pub async fn ensure_backend_deployed(cfg: &RemoteConfig) -> Result<String, DeployError> {
    let plan = match ask_plan(cfg).await {
        Ok(p) => p,
        Err(why) => return Err(DeployError::Refused(why)),
    };
    let bin = planned_binary(&plan)?;
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
    let theirs = match plan.action {
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
    // 〔E2 · E-c〕每次连上（预检）都照计划处理一次旧落点；结局只进日志，不挡连接。
    let swept = apply_legacy(&plan.legacy, &fs).await;
    if !swept.is_empty() {
        tracing::info!("远端 [{}] {swept}", cfg.origin_label());
    }
    Ok(theirs.unwrap_or_else(|| bin.build_id.to_string()))
}

/// 远端路径的父目录（远端恒为 POSIX `/` 分隔，不用 std::path）。
fn remote_parent(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) => "/",
        Some(i) => &path[..i],
        None => ".",
    }
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

// 〔MIG-3a · 09-28 预裁〕`is_safe_remote_managed_path`〔散文墓碑〕删了：第 2 个消费者（`acct_iso_deploy` 的落点围栏）随那条命令退役，只剩零个。

/// 手动安装 / 更新远端后端（机器页 ①「部署后端」按钮）。逻辑同自动部署 [`ensure_backend_deployed`]，
/// 但**返回人读结果**，且把自动部署里「优雅跳过」的几种情况（探测不到 arch / 无该 arch 内嵌）显式报错。
/// 〔E2 · V28〕落点就是 `~/.cc-monitor/bin/ccm`（后端本体，没有 shim）⇒ 部署后端就是放 `ccm`，没有第二样要放。
#[tauri::command]
pub async fn deploy_remote_backend(cfg: RemoteConfig) -> Result<String, String> {
    // 〔MIG-3b〕与自动部署同一份计划（本机常驻后端判）、同一个取字节口、同一句拒绝的话。
    let plan = ask_plan(&cfg).await?;
    let bin = planned_binary(&plan)?;
    // 〔SR1b〕经本机常驻后端那条 `files` 链路。
    let fs = RemoteFs::open(&cfg).await?;
    let backend_msg = match plan.action {
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
    // 〔E2 · E-c〕旧落点那份后端字节（后端认出是我们编的才删）。
    let swept = apply_legacy(&plan.legacy, &fs).await;
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
