//! SS-D：远端**自部署**的业务那一半（issue #29 自动部署 F08 · 手动安装 / 卸载 · `ccm` 入口）＋ 别名块的规划。
//!
//! 执行那一半（SFTP）不在本模块：经本机常驻后端的 `files` 链路（见下「本模块手里已经没有 SFTP 了」）。
//!
//! ## 只读铁律豁免（INVARIANT §1 / 账本 SS-G）—— 穷举登记见 `src/doc/INVARIANTS.md §1`
//! cc-monitor 对远端的写入均**用户显式触发**，各自独立路径守卫、绝不混用：
//! - **F08**：自部署后端二进制到 `~/.cc-monitor/bin/`（非用户数据、幂等、版本门控）。
//! - **F89a**：用户**显式**增/改/删远端**项目** `.mcp.json`。**写已不在本模块**；
//! 今天由那台后端自己算、自己写（帧命令 `mcp-sync-apply` / `ext-uninstall-apply`，`mcp_edit.rs::project_root` 守落点）。
//!   **SS-14**：写面**只** `.mcp.json`，非 Claude 会话数据。
//! **落盘已不在本模块**：那台后端自己读改写（文件管理那一面）；规划那一半
//!   （`merge_profile_block` / `strip_profile_block`）住 `profile_installer`。
//! - **F50**：追加公钥到远端 `~/.ssh/authorized_keys`今天是本机常驻后端的帧命令 `pubkey-push`（那台后端在就经它写 · 不在就一次 exec；不在本模块，登记于此备查）。
//!
//! 原子写（EXCL 临时件 → 旧目标先**改名成 `.bak`**（不是删）→ 上位 → 清 `.bak`）与它的来历住后端
//! `ssh-dial/sftp.rs::put_atomic`（F89a 审计后加固 · DN-7 订正「删旧」那句 · setstat 截断事故）。
//!
//! # **本模块手里已经没有 SFTP 了**
//!
//! 用户「SFTP 进本机常驻后端，只写暂存区」：SFTP 客户端住本机常驻后端（`src/backend/ssh-dial/sftp.rs`，
//! 与其它 SSH 同一条连接），**远端写只许两处**（`~/.cc-monitor/staging/` · `~/.cc-monitor/bin/`）。
//! **部署判定也不在本模块了**：该不该换 · 换成哪一格 · 落点那一份是谁由本机常驻后端出计划
//! （帧命令 `deploy-plan`，帧面 `src/backend/control/deploy_plan.rs`、判定住后端 `deploy` crate、两侧对上的形状住契约 crate `deploy-contract`）；
//! 本模块只**照计划放字节**（取这一版带着的那一格 · 身份戳自检 · mkdir · 原子上传 ＋ 读回判定 · 删旧落点），
//! 执行经 [`crate::dial_host::RemoteFs`]（本机后端那条 `files` 链路的一问一答）。
//! 原子上传的序列与它的两条事故教训（先备份不删旧 · **绝不** rename 之后 setstat 兜底 chmod）住后端
//! `ssh-dial/sftp.rs::put_atomic` 的头注。
//! ⇒ 落点变了一格：`ccm` 入口从 `~/.local/bin/ccm` 挪到 **`~/.cc-monitor/bin/ccm`**（两个写根之内；也正是
//! 要的落点；自带别名块把 `~/.cc-monitor/bin` 加进 PATH）。

use crate::copy_table::copy_text;
use crate::detail::Said;

use crate::dial_host::RemoteFs;
use crate::stream_source::RemoteConfig;

/// 上传 + **读回逐字节比对**，读回不对当场删掉传坏的那一份 —— 三步都在本机后端（`files` 链路 `put` 的 `verify` 那一格 ⇒
/// 拨号核心 `ssh-dial/sftp.rs::put_verified`，手机那一趟装用的是同一份）；那一句（坏在哪 · 删没删掉）也是那边写好的，这里原样带回。
///
/// ## 为什么这一步此前不存在（T04 审计①）
///
/// 当时 `deploy_remote_backend` 的**全部**上传写完**直接写版本标记**，中间没有任何读回 ⇒ 传输损坏的后端二进制照样被写上
/// 正确的标记 → 下次判「已是最新，跳过」→ **坏二进制永久驻留**，而用户看到的是部署成功。今天旁挂标记退役（身份读字节自己的戳），
/// 一份传坏的字节若恰好还带着对的戳一样会被判「已是这一版」⇒ 读回不对就删，下次就是「落点没有 ⇒ 装」。
pub(crate) async fn upload_verified(
    fs: &RemoteFs,
    remote_path: &str,
    bytes: &[u8],
    mode: u32,
) -> Result<(), Said> {
    fs.put(remote_path, bytes, mode, true).await
}

/// 远端那台要的那一份后端（字节从 `byte_table` 按那台的 (OS, arch) 取，`include_bytes!` 不在本文件）。
pub struct BackendBinary {
    /// 🔴 `K-R70`：**这份字节自报的身份**（`build.rs` 从二进制里扫 `CC_MONITOR_BUILD_STAMP`
    /// 得来，不是从旁边那个 `.build_id` 文本文件抄的）。
    pub build_id: &'static str,
    pub bytes: &'static [u8],
    /// 那台机器是哪一格（说给人听：「Linux / x86_64」）。
    pub machine: String,
}

/// 身份戳的两个界标（住契约 crate，后端拼戳用的是同一份；本文件不许出现那两个字面量）。本机那一份的身份（`local_backend.rs`）也用它。
pub(crate) const STAMP_MARKS: deploy_contract::Marks<'static> = deploy_contract::Marks {
    open: deploy_contract::STAMP_OPEN,
    close: deploy_contract::STAMP_CLOSE,
};

/// 部署计划答话的形状住契约 crate `deploy-contract`（判定住后端 `deploy` crate）；本 crate 里还要它的几处从这里拿同一份名字。
pub use deploy_contract::DeployAction;

/// **本机常驻后端出的部署计划**（帧命令 `deploy-plan`，线上形状 `tests/__fixtures__/deploy-plan.golden.json`）。
/// 该不该换 · 换成哪一格 · 落点那一份是谁 —— 全是后端判的；本模块只照它放字节。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    pub(crate) key: deploy_contract::Key,
    /// 那一格这一版带着的字节自报的身份（后端拿它当对照物）。
    pub(crate) expected: String,
    pub(crate) action: DeployAction,
    /// 落点目录里没人要的上传残件（家目录相对；后端判的，这里只照删）。
    pub(crate) leftovers: Vec<String>,
    /// 问 `uname` 那一趟拨号的 ack（拨号在本机后端里）：逐地址指纹由 [`ask_plan_for`] 交给
    /// `dial_host::settle_host_key` 固化 —— 与 monitor 自己开链路那几条同一个判定，不另写。
    pub(crate) ack: crate::ssh_link::Ack,
}

/// `deploy-plan` 的应答 → [`Plan`]（**严格收**：少一格、多一格、认不出的值都是错 —— 两侧漂了要当场说出来）。
pub(crate) fn decode_plan(v: &serde_json::Value) -> Result<Plan, String> {
    const KEYS: [&str; 9] = [
        "ack",
        "action",
        "arch",
        "expected",
        "label",
        "leftovers",
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
    let key = deploy_contract::key_of(text("os").unwrap_or(""), text("arch").unwrap_or(""))
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
    let leftovers: Vec<String> = obj
        .get("leftovers")
        .cloned()
        .and_then(|a| serde_json::from_value(a).ok())
        .ok_or_else(bad)?;
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
        leftovers,
    })
}

/// 问本机常驻后端要计划那一趟的上限：一次 `uname` ＋ 两次 stat ＋ 至多两次扫身份戳 ＋ 一次读回（每样一两个往返）。
const PLAN_BUDGET: std::time::Duration = std::time::Duration::from_secs(120);

/// 本机常驻后端那条命令的名字（与 `src/backend/stream/inbound/mod.rs::REGISTRY` 同名）。
pub(crate) const PLAN_CMD: &str = "deploy-plan";

/// 问本机常驻后端要一份计划。入参只有事实：怎么够到那台（拨号请求）· 这一版带着哪几格字节、各自自报的身份。
async fn ask_plan(cfg: &RemoteConfig) -> Result<Plan, Said> {
    ask_plan_for(cfg, &crate::byte_table::carried_backends()).await
}

/// [`ask_plan`] 的本体，「带着哪几格」由调用方交（生产 = 这一版的槽；回环台架交它送去的那份字节的身份）。
async fn ask_plan_for(
    cfg: &RemoteConfig,
    carried: &[(deploy_contract::Key, &str)],
) -> Result<Plan, Said> {
    use crate::backend_route::{route_call_error, Routed};
    let carried: Vec<serde_json::Value> = carried
        .iter()
        .map(|(k, id)| serde_json::json!({ "os": k.os.label(), "arch": k.arch.label(), "id": id }))
        .collect();
    let command = |said: String| Said::new(said, PLAN_CMD, None);
    let dial = crate::dial_host::transfer_dial(cfg);
    let args = serde_json::json!({
        "dial": dial,
        "carried": carried,
        "machine": cfg.origin_label(),
    });
    let client = crate::dial_host::local_backend_accepting(PLAN_CMD)
        .await
        .map_err(command)?;
    // 计划由本机常驻后端出 ⇒ 它写的那份详情（那台答 `uname` 的原话也在里面）原样带上。
    let data = client
        .call(PLAN_CMD, args, PLAN_BUDGET)
        .await
        .map_err(|e| {
            let said = match route_call_error(&e, &copy_core::local_machine(), |_code, message| {
                message.to_string()
            }) {
                Routed::NoChannel(s) | Routed::Refused(s) => s,
            };
            Said::of_call(said, PLAN_CMD, &crate::origin::Origin::local(), &e)
        })?;
    let plan = decode_plan(&data.ok_or_else(|| command(copy_text("rsSftp.plan.internal", &[])))?)
        .map_err(command)?;
    crate::machine_state::note_os(&cfg.origin_label(), plan.key.os.label());
    // 第一次连一台没钉过指纹的机器就在这一跳 ⇒ 照 monitor 自己开链路那几条同一个判定固化。
    crate::dial_host::settle_host_key(cfg, &dial, &plan.ack);
    Ok(plan)
}

/// 照计划取字节：那一格这一版带着的那一份（`byte_table::pick`）。计划说的身份与字节自报的对不上 ⇒ 两侧漂了，不推。
fn planned_binary(plan: &Plan) -> Result<BackendBinary, Said> {
    crate::byte_table::pick(plan.key)
        .and_then(|p| Some((p.bytes, p.build_id?)))
        .filter(|(_, id)| *id == plan.expected)
        .map(|(bytes, build_id)| BackendBinary {
            build_id,
            bytes,
            machine: plan.key.label(),
        })
        .ok_or_else(|| {
            Said::of(
                "rsSftp.plan.notThisSlot",
                &[("machine", &plan.key.label()), ("expected", &plan.expected)],
            )
        })
}

/// 自动部署没成的两种说法 —— **类型上与「部署成功」分得开**（「返回类型上不许有『成功』这一支」）。
#[derive(Debug)]
pub enum DeployError {
    /// 那台要不了这份 / 这一版没带 / 判不清它是谁 —— 那句话是本机常驻后端说的（`deploy-plan` 失败那一形），原样带回（带它的详情）。
    Refused(Said),
    /// 做了但没做成（配置、链路、放字节那几步）—— 一句说清楚的话 ＋ 复制详情。
    Failed(Said),
}

impl DeployError {
    /// 对用户说的那一句 ＋ 复制详情。
    pub fn said(self) -> Said {
        match self {
            DeployError::Refused(s) | DeployError::Failed(s) => s,
        }
    }
}

impl From<Said> for DeployError {
    fn from(s: Said) -> Self {
        DeployError::Failed(s)
    }
}

/// 后端的落点：SFTP 那一侧（家目录相对）· 给人看的。**本机与远端同一个**，常量住 `relay_route_core`。
pub(crate) const LANDING_REL: &str = relay_route_core::BACKEND_LANDING_REL;
const LANDING_SHOWN: &str = "~/.cc-monitor/bin/ccm";

/// 照计划删上一趟没收拾掉的上传残件（后端判的哪几份；删不掉只进日志、不挡连接，下次连上再来）。
async fn sweep_leftovers(leftovers: &[String], fs: &RemoteFs, origin: &str) {
    for rel in leftovers {
        match fs.remove(rel).await {
            Ok(_) => tracing::info!("远端 [{origin}] 删了上一趟留下的上传残件 ~/{rel}"),
            Err(e) => tracing::warn!("远端 [{origin}] 上传残件 ~/{rel} 没删掉：{e}"),
        }
    }
}

/// 连接前确保远端后端已（自动）部署到固定落点 `~/.cc-monitor/bin/ccm`（issue #29；那个文件就是后端本身）。
///
/// 流程：① 问本机常驻后端要计划（[`ask_plan`]：那台是什么机器、要哪一格、落点那一份是谁、换不换）——
/// 它拒了 ⇒ [`DeployError::Refused`]；② 照计划取字节（[`planned_binary`]）、需要则开 `files` 链路 mkdir -p + 原子上传；
/// ③ 照计划清旧落点那份字节。
///
/// **不阻断**：调用方（stream_source::run）拿到 `Err` 仍接着试连已有后端（手动部署的后端照样能连），
/// 但那句话经远端健康通道（`kind = "deploy"`）发到界面上，不再只是一行日志。
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
        return Err(DeployError::Failed(Said::of(
            "rsSftp.deploy.noBuildId",
            &[],
        )));
    }
    // 经本机常驻后端那条 `files` 链路（写只许 `~/.cc-monitor/bin/` 与暂存区）。
    let fs = RemoteFs::open(cfg).await?;
    let theirs = match plan.action {
        // 不比这一版旧 ⇒ 一个字节不写、照旧连上那一份；回**那台上的**身份（不是这一版的 ——
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
            crate::machine_state::deploying_auto(&cfg.origin_label());
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
    // 每次连上（预检）都照计划删一次上传残件；结局只进日志，不挡连接。
    sweep_leftovers(&plan.leftovers, &fs, &cfg.origin_label()).await;
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

/// 🔴 `K-R70`：**这份字节自己说得出它是 `build_id` 吗** —— 不看它旁边任何文件。
///
/// 找的是后端那侧那段 `#[used] static CC_MONITOR_BUILD_STAMP`：`<开>` ＋ `BUILD_ID` ＋ `<关>`，
/// 扫法只有一份（`deploy_contract::identity_of_bytes`，恰好一个戳才是身份），界标见 [`STAMP_MARKS`]
/// ⇒ 本文件里**不许出现那两个字面量**（`the_embedded_identity_comes_from_the_bytes_not_from_a_label` 在数它）。
///
/// # 它买到的与买不到的
///
/// ✅ 买到：「这份字节是不是 `build_id` 那一次构建的产物」——**戳与字节同生共死**，
///    改一份旁文件、换一张清单都动不了它。
/// ⚠ 买不到：**防篡改**。谁都能往一段字节里塞一个假戳。它防的是漂移与手滑
///    （拿错文件 / 铺了旧产物 / 只 bump 源码没重编），不防恶意 —— 那要签名，不是戳。
pub fn bytes_carry_build_stamp(bytes: &[u8], build_id: &str) -> bool {
    !build_id.is_empty()
        && deploy_contract::identity_of_bytes(bytes, STAMP_MARKS)
            == deploy_contract::RemoteIdentity::Stamp(build_id.to_string())
}

// 这里原来是按 arch 取字节的那个函数：两份 musl 的 `include_bytes!` 与一个只认 arch 的 `match`。
//   槽与它们的 `K-R70` 身份取值口（`BACKEND_EMBEDDED_ID_<ARCH>`）逐字搬进了 `byte_table.rs`（全仓唯一的取字节口）。

// ============================================================================
// F08c：手动安装 / 卸载后端（设置面板两个按钮）。安装逻辑同自动部署、但返回人读结果；
// 卸载删落点那个文件（固定落点 `~/.cc-monitor/bin/ccm`，没有外来路径要守）。
// ============================================================================

/// 手动安装 / 更新远端后端（机器页 ①「部署后端」按钮）。逻辑同自动部署 [`ensure_backend_deployed`]，
/// 但**返回人读结果**，且把自动部署里「优雅跳过」的几种情况（探测不到 arch / 无该 arch 内嵌）显式报错。
/// 落点就是 `~/.cc-monitor/bin/ccm`（后端本体，没有 shim）⇒ 部署后端就是放 `ccm`，没有第二样要放。
#[tauri::command]
pub async fn deploy_remote_backend(cfg: RemoteConfig) -> Result<String, Said> {
    let r: Result<String, Said> = async move {
        // 与自动部署同一份计划（本机常驻后端判）、同一个取字节口、同一句拒绝的话。
        let plan = ask_plan(&cfg).await?;
        let bin = planned_binary(&plan)?;
        // 经本机常驻后端那条 `files` 链路。
        let fs = RemoteFs::open(&cfg).await?;
        let backend_msg = match plan.action {
            // 手动点也不降级：出路与「它不说自己是谁」那一格同一句（先卸载再部署 = 明确授权覆盖）。
            DeployAction::Keep { why, .. } => {
                copy_text("rsSftp.deploy.keptNotOlder", &[("why", &why)])
            }
            DeployAction::Skip => copy_text(
                "rsSftp.deploy.upToDate",
                &[
                    ("buildId", &bin.build_id.to_string()),
                    ("machine", &bin.machine.to_string()),
                    ("path", &LANDING_SHOWN.to_string()),
                ],
            ),
            DeployAction::Deploy(reason) => {
                crate::machine_state::deploying(&cfg.origin_label(), false);
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
        // 上传残件（后端判的哪几份）。
        sweep_leftovers(&plan.leftovers, &fs, &cfg.origin_label()).await;
        Ok(backend_msg)
    }
    .await;
    r.map_err(|s| s.named("deploy_remote_backend"))
}

/// 卸载远端后端（设置面板「卸载后端」按钮）：删落点那个文件（它就是 `ccm`，卸后端就是卸 `ccm`）。
/// 只读铁律豁免（SS-G）：用户显式触发的删。注意：若该机器仍启用，自动部署会在下次连接重新装回——提示见返回消息。
#[tauri::command]
pub async fn uninstall_remote_backend(cfg: RemoteConfig) -> Result<String, Said> {
    let r: Result<String, Said> = async move {
        // 经本机常驻后端那条 `files` 链路删（写只许 `~/.cc-monitor/bin/` 与暂存区 —— 围栏拒 ⇒ 原话带回）。
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
    .await;
    r.map_err(|s| s.named("uninstall_remote_backend"))
}

// F10（别名块）这一段搬去了 `profile_installer.rs`（B §2 第 12 条：本文件已经不做 SFTP，
//   别名块的真相 —— 围栏那一对 · 块的内容 · 自带的名字 · 合 / 剥 · 远端装 / 卸两条命令 —— 归别名域）。
//   本文件只剩部署：后端字节 · 身份 · 读回 · `ccm` 入口。

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/sftp_tests.rs"]
mod tests;
