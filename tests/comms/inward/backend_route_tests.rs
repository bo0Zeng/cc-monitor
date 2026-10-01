use super::*;
use std::time::Duration;

fn plain(code: &str, message: &str) -> String {
    format!("{code}/{message}")
}

/// ★★ **本模块的核心性质**：只有「能证明没发出去」的三档才允许回落。
///
/// 反过来错法（把 `Remote{wrong_owner}` 也当成 `NoChannel`）会让一次**被门拒绝**
/// 转头走 SSH 再做一次 —— 那是把门拒绝洗成另一条路的成功。
#[test]
fn only_the_errors_that_prove_nothing_was_sent_allow_a_fallback() {
    let fallback_ok = [
        CallError::Unsupported {
            cmd: "kill".into(),
            offered: vec!["ping".into()],
        },
        CallError::TooManyPending,
    ];
    for e in &fallback_ok {
        assert!(
            matches!(route_call_error(e, plain), Routed::NoChannel(_)),
            "{e:?} 是在写出去之前返回的，应当允许回落"
        );
    }
    let no_fallback = [
        CallError::Disconnected,
        CallError::Timeout {
            after: Duration::from_secs(1),
            withdraw: crate::chan::wire::Withdraw::Asked,
        },
        CallError::Cancelled,
        CallError::Remote {
            code: "wrong_owner".into(),
            message: "sid=".into(),
        },
        CallError::Remote {
            code: "too_many_windows".into(),
            message: "windows=3".into(),
        },
        CallError::Remote {
            code: "kill_failed".into(),
            message: "boom".into(),
        },
        CallError::Remote {
            code: "no_such_session".into(),
            message: "".into(),
        },
        CallError::Remote {
            code: "invalid_args".into(),
            message: "未知 mode `attach-only`".into(),
        },
    ];
    for e in &no_fallback {
        assert!(
            matches!(route_call_error(e, plain), Routed::Refused(_)),
            "{e:?} **不能证明**这条命令没发出去（或后端已经说了话）——\n\
                 允许回落就等于在未知状态上再做一次动作，\
                 而 `wrong_owner`/`too_many_windows` 更是把门拒绝洗成另一条路的成功"
        );
    }
    // 「没有通道」那一档也必须是可回落的。
    assert!(matches!(no_channel("h1"), Routed::NoChannel(_)));
}

/// `backend/control/` 里每个**走后端的发送端**的判定。
///
/// # ⚠ 为什么这里是登记表而不是一句「都必须用分流器」
///
/// 第一版是**手写的两条清单**（`backend_kill.rs` / `backend_send_keys.rs`）——
/// Phase G 的 `/full-audit` 当场指出：同一个目录里**第三个**走后端的发送端
/// `backend_launch.rs` 里的 `backend_send_into`〔散文墓碑〕（U8a-2c-1，早于本工作区；〔C4e〕随就地 resume 迁到界面删了）**不在清单里**，
/// 于是它自己 match 一整套 `CallError`、把**每一档**都折成「诚实降级」，
/// 而调用方拿到降级就**回落到 TS 渲染的整串**（那条串没有 §34 的 Gate 2）。
///
/// ⇒ **一次 `wrong_owner`（门说「这不是本工具的会话」）会被回落成一条无门的 shell 路。**
/// 那正是本模块开头声称要挡的「把被门拒绝洗成另一条路的成功」，
/// **而它就发生在守卫脚下** —— 因为守卫的发现机制是手写清单。
///
/// ★ 「硬编码清单 vs 递归遍历」这一族在本仓这是**第三个模块**
/// （`readonly_guard::spawn_registry` · `tmux_backend_gate_guard` 的两文件表 · 本条），
/// 而这一次是**我自己本轮亲手挖的**。⇒ 改成**遍历 + 登记表**：
/// 目录里每个 `.call(` 的文件都必须在下表里，要么用分流器、要么是**带理由的刻意例外**。
#[cfg(test)]
const SENDERS: &[(&str, Verdict)] = &[
    // 〔C4e · 第四波 4C〕这里原来头三行是 `backend_kill.rs` / `backend_send_keys.rs` / `backend_launch.rs`
    //   （杀会话 · 送键 · 就地 resume 三个发送端，都 `UsesRouter`；`backend_launch` 那一行还记着 F14 的
    //   「`may_fall_back` 第三个字段 · 上一版误记成例外、改好当天如设计般红过一次」）。三条 Tauri 命令迁到界面
    //   （`src/frontend/ui/tmux-control.ts` 经通道直接说 `kill` / `launch`），发送端整份删了，发现阶段扫不到它们 ⇒ 三行摘掉。
    //   它们守的那件事没丢：Rust 这一侧的分层判定照旧只在 `backend_route::layer_call_error` 一处（通道宿主 `host.rs` 用它）；
    //   F14 那条「能不能回落」在界面那一侧的同义一份（`ipc/chan-caller.ts::provablyNotSent`）由跨语言金样
    //   `tests/__fixtures__/reach-collapse.golden.json` 与本侧 `route_call_error` 对拍（`chan/webview_tests.rs`）。
    // 〔MIG-1 续〕`ssh_source.rs` 那一行（08-08 扩面逮出的第四个发送端：`probe_backend`〔散文墓碑〕 里那条 `ping`，只渲染诊断串、不做回落决策）
    //   摘了：测试连接搬进本机后端（`dial/probe.rs`），monitor 这一侧不再发那一问。

    // 〔C4e · 第四波 4C〕**`cc_bus.rs` 那一行退役了**：P4f 起它把 cc-bus 写面（发消息 · 收掉 · 派生 · 查在线）经后端的
    //   `bus-*` 原语转一手、走分流器；现在界面经通道直接说（`src/frontend/ui/cc-bus-control.ts`，广播的挑人也搬进了后端 `bus-broadcast`），
    //   monitor 的 `cc_bus.rs` 只剩读名单 / 读收件箱两条 shell 读，不再是走后端的发送端 —— 从登记表删，不留过渡格。
    //   「monitor 里写面一条路都不剩」由 `cc_bus_tests.rs::the_monitor_has_no_cc_bus_write_path_any_more` 两向判。
    // ★ `K-R112` 09-13：**第七个发送端** —— 抓屏（`capture_via_backend`〔散文墓碑〕，〔C4e〕已删）
    //   改走帧面 `capture-pane`。它**没有第二条路可回落**（那条一次性 SSH 本件删净了），
    //   但**照样走分流器**，理由与 `cc_bus.rs` 那条逐字相同：
    //   〔`设计/50`：原话还并列了 `account_usage.rs`（`K-R104` 的第六个发送端）——
    //    用量 ③ 轴整轴退役，那个发送端不存在了，发送端从七个变回六个。〕
    //   本模块的三态是从 `Routed` 搬过来的，不是它自己 match 一遍错误枚举。
    //   ⚠ 它回的是 `Result<String, Routed>` —— 「拿到了那一屏」与「三态里的另外两态」
    //   在类型上分得开，怎么对用户说由调用方 `capture_remote_pane` 决定。〔散文墓碑〕
    // 〔C4e · 第四波 4C〕**上面那一行（`tmux.rs`）退役了**：抓屏改由界面经通道直接问那台机器的后端
    //   （`src/frontend/ui/tmux-control.ts::capturePane`），monitor 里那个发送端（`capture_via_backend`〔散文墓碑〕）删了 ——
    //   `tmux.rs` 从此不再直连 `inbound_client`，发现阶段扫不到它，登记跟着摘。分层判定照旧只在
    //   `backend_route::layer_call_error` 一处（通道宿主 `host.rs` 用它），界面那一侧只把分好层的结果翻成一句话。
    // ★ 〔步 `24f` 第四刀 09-21〕**第七个发送端** —— 原生文件窗口那一侧的搜索
    //   （`filewin/find.rs`，`files-find` / `files-index-status` /
    //   `files-index-rebuild` / `files-browse` 四条）。
    //   它**没有第二条路可回落**，而且这一次那句话是硬的：`设计/60 §2 档①` 逐字
    //   「SFTP 只能递归 `READDIR`，N 次往返；而且**协议里没有「放一份常驻索引」这个概念**」
    //   ⇒ 搜索在 SFTP 那一侧**结构上不存在**，不是「今天还没做」。
    //   但**照样走分流器**，理由与 `cc_bus.rs` / `tmux.rs` 那两条逐字相同：
    //   分流规则一有第二份实现，「被门拒绝」就会在某一份里被洗成「换条路重做」。
    //   ⚠ 它回的是 `Result<Value, Routed>`（同 `tmux.rs` 那一处的形状）——
    //   「拿到了那一份 data」与「三态里的另外两态」在类型上分得开。
    // 〔F2 · 2026-09-24〕**上面那一行（`find.rs`）退役了**：文件窗口成了独立进程，只经通道说 `call`，
    //   搜索不再直连进程级登记表（`inbound_client`），分层判定在通道宿主那一侧（`host.rs`，
    //   走分流器的分层出口）。窗口一侧只把分好层的结果翻成一句人话（`filewin/source.rs::said`）。
    //   ⇒ 两份新发现的 `.call(` 都**不做回落决策、也碰不到 inbound**，牙与纯路由器那一档同一套：
    // · `source.rs` —— 窗口进程里说 `Comms::call` 的唯一一处（`filewin/source.rs::ask`）。
    ("source.rs", Verdict::PureRouterNoFallbackDecision),
    // · 〔MIG-3a · 主会话 09-28 裁 3〕`entry.rs` 那一行删了：monitor 开窗前替窗口列第一屏那一问进了窗口进程
    //   （`filewin/proc.rs::first_screen` → 上面 `source.rs` 那一处），`entry.rs` 生产段里一个 `.call(` 都没有了。
    // ★ 〔面 A 通道，2026-09-24〕**不是发送端，是纯路由器**（`chan/router.rs`）。
    //   发现阶段看见它，是因为它生产段里有 `.call(` —— 那是**注入的** `Backends` 句柄的
    //   `call`，不是 `inbound_client` 的；它生产段里的 `CallError::` 也是通道自己的
    //   （`05 §3.3.1` 那三层），不是本模块分流的那个枚举。
    //   ⇒ 登记成 `PureRouterNoFallbackDecision`，牙见那一档的判法：它连
    //   `inbound_client` 这个名字都不许碰 —— 碰了就说明它不再「只转交」了。
    ("router.rs", Verdict::PureRouterNoFallbackDecision),
    // ★ 〔C4a · 第四波 · 2026-09-24〕**主界面那一跳的宿主**（`chan/webview.rs`）。发现阶段看见它，是因为
    //   它生产段里有 `.call(` —— 那是**注入的** `Backends` 句柄（生产注入 `chan::host::InboundBackends`，
    //   从前与 `entry.rs` 同一个〔09-28 裁 3 起它不再问〕），期限与撤单交给路由器那一份 `settle`。它不碰 inbound、不做回落判断，
    //   牙与纯路由器那一档同一套。
    ("webview.rs", Verdict::PureRouterNoFallbackDecision),
    // ★ 〔面 A 通道，2026-09-24〕**第八个发送端** —— 通道的生产句柄（`chan/host.rs`），
    //   外部前端经路由器转来的 `call` 在这里走 `inbound_client`。
    //   它要的不是三态而是 `05 §3.3.1` 的分层结果（层 × `reach` × `why`），
    //   ⇒ 走分流器的**第二个出口** `layer_call_error`（三态正是从它收拢出来的，同出一源），
    //   「没有控制通道」走 `layer_no_channel`。它自己**不** match inbound 的错误枚举。
    ("host.rs", Verdict::UsesRouter),
    // ★ 〔`C1` · 2026-09-24〕**第九个发送端** —— 只读查询面（`frame_query.rs`，
    //   `history-*` / `accounts-*` 八条）。它**没有第二条路可回落**：逐次拨 SSH 那条正是
    //   本件要删的东西，长连接不在时明说「没有控制通道」。但**照样走分流器**
    //   （`route_call_error` ＋ `no_channel`），理由与 `cc_bus.rs` / `tmux.rs` / `find.rs` 逐字相同。
    ("frame_query.rs", Verdict::UsesRouter),
    // ★ 〔SR1a · 2026-09-24〕**又一个发送端** —— 链路的 monitor 这一侧（`link_mux.rs`，`link-*` 四条，
    //   经本机常驻后端那条流开到各远端的字节流）。它**没有第二条路可回落**（`D11`：不起代理进程、
    //   不进程内拨），失败只渲染成一句话；**照样走分流器**，理由与 `frame_query.rs` 那一行逐字相同。
    ("link_mux.rs", Verdict::UsesRouter),
    // ★ 〔B2 · 条 66 · 2026-09-24〕**第十个发送端** —— 「退出行为」那个值搬到后端所在那台机器上之后，
    //   monitor 问它 / 交它写 / 退出臂现问它，都经 `backend_policy.rs::exit_policy_call` 这一口。
    //   没有第二条路可回落（值只在那台机器上），长连接不在时明说「没有控制通道」；**照样走分流器**，
    //   理由与 `frame_query.rs` 那条逐字相同。
    ("backend_policy.rs", Verdict::UsesRouter),
    // 〔MIG-2〕`apikey_remote.rs`〔散文墓碑〕那一行摘了：那一口（上游选择的帧面发送口）最后只剩起会话问 `launch-endpoint` 一个调用方，
    //   起会话搬进后端之后零调用方、整个模块删了（界面经通道直问）。
    // 〔DEL〕`remote_relay.rs` 那一行摘了：远端「用到才起」的脱离中转一族删了（中转只住那台的常驻后端里）。
    // 〔MIG-3b 续〕「足迹」那一行（`footprint_remote.rs`〔散文墓碑〕）摘了：成品由那台后端出，界面经通道直问 `footprint-report`。
    // 〔MIG-3a〕`mcp_sync.rs`〔散文墓碑〕那一行摘了：MCP 推 / 拉的编排进了被写那台后端，界面经通道直问。
    // ★ 〔AS2 · 第四波 4B〕资产目录同步：把「怎么够到那台」交给**本机**后端 `assets-sync`，经
    //   `asset_sync.rs::ResidentBackend::call` 这一口；失败经共用分流器翻成人话。形状与理由同上几条。
    ("asset_sync.rs", Verdict::UsesRouter),
    // 〔MIG-3a〕skill「装到这台」那一行（原住 `skill_install.rs`）摘了：装 / 卸的编排进了被写那台后端，界面经通道直问。
    // ★ 〔RW1 · 第四波 · 2026-09-24〕**第十一个发送端** —— 用户文件的读改写 ＋ 删历史会话
    //   （`user_files.rs::BackendDoor`：`files-home` / `files-peek` / `files-put` / `files-rename` /
    //   `files-chmod` / `files-delete-session`）。用户裁「只允许后端的文件管理部分写文件」也管本机
    //   ⇒ **没有第二条路可回落**（直写正是被裁掉的那一形，`D11`）；长连接不在时明说「后端没连上」。
    //   **照样走分流器**（`route_call_error` ＋ `no_channel`），理由与 `frame_query.rs` 那条逐字相同；
    //   它要的 `stale` 那一档是从分流器递回来的 `(code, message)` 里认的，不自己 match 错误枚举。
    ("user_files.rs", Verdict::UsesRouter),
    // ★ 〔SR1b · 第四波〕传输台的中继（`sftp_pool.rs`）：窗口的开单 / 订阅经它转给**本机**常驻后端
    //   （`transfer-*` 四条，传输台住那里）。没有第二条路可回落（`D11`：不进程内开 SFTP），
    //   后端说的码原样带回窗口；**照样走分流器**，理由与 `link_mux.rs` 那一行逐字相同。
    ("sftp_pool.rs", Verdict::UsesRouter),
    // ★ 〔MIG-3b · 4d-lanes 子步 1〕部署计划：把「怎么够到那台 ＋ 这一版带着哪几格」交给**本机**后端 `deploy-plan`，
    //   经 `sftp.rs::ask_plan_for` 这一口；判定住后端，没有第二条路可回落（判定不回到 monitor）。形状与 `asset_sync.rs` 那一行同。
    ("sftp.rs", Verdict::UsesRouter),
    // 〔THIN〕远端常驻后端 hello 的新旧改问本机常驻后端（`resident-verdict`，判定只在后端）：一问一答，照样走分流器，
    //   理由与 `sftp.rs` 问部署计划那条逐字相同（长连接不在时明说，没有第二条路可回落）。
    ("remote_resident.rs", Verdict::UsesRouter),
    // 〔THIN〕旧入口 `~/.local/bin/ccm` 的去向改问本机常驻后端（`deploy-retired`）：一问一答，照样走分流器。
    ("ccm_legacy.rs", Verdict::UsesRouter),
];

/// 分流器的**两个出口**：分层结果（`05` 形状）与从它收拢出来的旧三态。
/// `UsesRouter` 认其中之一；两个名字在整棵源码树里都必须**恰好一处定义**、就在 `backend_route.rs`。
#[cfg(test)]
const ROUTER_EXITS: &[&str] = &["layer_call_error", "route_call_error"];

/// 一份生产段里 `fn <name>` 这一形（定义）出现了几处 —— 名字两侧有边界（`fn route_call_error2` 不算）。
#[cfg(test)]
fn definitions_of(prod: &str, name: &str) -> usize {
    let head = format!("fn {name}");
    prod.match_indices(head.as_str())
        .filter(|(i, _)| {
            let before_ok = prod[..*i]
                .chars()
                .next_back()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
            let after = prod[i + head.len()..].chars().next();
            before_ok && matches!(after, Some('(') | Some('<'))
        })
        .count()
}

#[cfg(test)]
#[derive(PartialEq, Eq, Debug)]
enum Verdict {
    UsesRouter,
    /// ⚠ **今天零个成员**（F14 之后）。**刻意保留这一档**：
    /// 它是「带理由的刻意例外」这个形态本身，下一个发送端要走例外时有地方落。
    /// 删掉它 = 逼下一个人要么硬改要么偷偷绕过登记表（铁律 13：别因「暂时没人用」删判据形态）。
    #[allow(dead_code)]
    ExemptPendingF14,
    /// **探测型发送端**：只把成败渲染成诊断文本，不决定「要不要回落到别的路」。
    ///
    /// ⚠ 与上面那一档**刻意分开**：`ExemptPendingF14` 说的是「本该走分流器、
    /// 但今天还差一步」，这一档说的是「**根本没有回落这回事**」。
    /// 合成一档会让「欠着」与「不适用」长得一样。
    /// 〔MIG-1 续〕今天没有住户（唯一那个 `ssh_source.rs` 的探测随测试连接搬进本机后端）；档位留着，下一个探测型发送端来了照样得表态。
    #[allow(dead_code)]
    ProbeOnlyNoFallbackDecision,
    /// **纯路由器**：它的 `.call(` 调的是**别人注入的句柄**，自己够不着任何后端发送端，
    /// 因此既没有「该不该回落」这个问题，也没有资格去分流。
    ///
    /// ⚠ 与 `ProbeOnlyNoFallbackDecision` **刻意分开**：那一档**真的**在走后端（发一条 `ping`），
    /// 只是不回落；这一档**根本不走后端**。合成一档会让「走后端但不回落」与「不走后端」长得一样
    /// —— 而后者一旦悄悄开始走后端，就该被逼回来重新表态。
    ///
    /// 🔴 牙（缺一当场红）：生产段**零** `route_call_error`（用了 ⇒ 它其实有回落决策）·
    /// **零** `inbound_client` / `InboundClient` / `client_for`（碰了 ⇒ 它不再只转交注入的句柄，
    /// 而且它生产段里那些 `CallError::` 从此可能是本模块分流的那个枚举 ——
    /// 这一档之所以能免掉 `own` 那条检查，**全靠**这一条把 inbound 的类型挡在文件外）。
    PureRouterNoFallbackDecision,
}

/// `PureRouterNoFallbackDecision` 那一档的牙：生产段里**不许出现**的名字（有边界匹配）。
/// 它们是「够得到本机/远端后端发送端」的全部入口：模块名、客户端类型名、取客户端的函数名。
#[cfg(test)]
const PURE_ROUTER_MUST_NOT_NAME: &[&str] = &[
    "inbound_client",
    "InboundClient",
    "client_for",
    "layer_no_channel",
];

/// 一份生产段是不是「纯路由器」—— 返回违反的那几条（空 = 过）。
#[cfg(test)]
fn pure_router_violations(prod: &str) -> Vec<String> {
    let mut out = Vec::new();
    for exit in ROUTER_EXITS {
        if guard_core::contains_word(prod, exit) {
            out.push(format!(
                "用了分流器出口 `{exit}`（它其实在走后端 ⇒ 登记该改成 `UsesRouter`）"
            ));
        }
    }
    for n in PURE_ROUTER_MUST_NOT_NAME {
        if guard_core::contains_word(prod, n) {
            out.push(format!("碰了 `{n}`（它不再只转交注入的句柄）"));
        }
    }
    out
}

/// ★★ **零命中守卫：分流规则不许有第二份实现 —— 而发现机制是遍历，不是手写清单。**
#[test]
fn every_backend_sender_is_registered_and_uses_the_one_router() {
    // ⚠⚠ **08-08：发现机制原本只扫 `src/backend/control/` 这一个目录**，而本模块头注
    //   声称的是「monitor 侧走后端的**所有**控制命令共用」。实测：把一个 `.call(`
    //   发送端放在 `tmux.rs`（目录之外）并让它自己判回落，**全仓 989 条判据一条不红**
    //   —— **声称的范围与人群的范围不是同一个**。改扫整棵 monitor 源码树。
    //   ★ 扩面当场逮出**第四个真实发送端**（`ssh_source.rs` 的探测 ping），此前整个在扫描面之外。
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    // 目录里所有「走后端」的文件：生产段出现 `.call(` 的。
    // 〔DL1〕另一个动词 `.call_until(`（`InboundClient::call_until`：截止时刻由调用方给的那一形）同样是发送 ——
    //   只认 `.call(` 的话，改用它的发送端会整个逃出扫描面（`frame_query.rs` 改成它的那一拍当场从人群里消失，本条因此红过）。
    let verb = format!(".call({}", "");
    let verb_until = format!(".call_until({}", "");
    let mut senders: Vec<String> = Vec::new();
    // ⚠ 发现阶段就把生产段留下：下面按名字取时**不能再用 `dir.join(name)`**，
    //   扩面之后 `dir` 是整棵 `src/`，而发送端散在子目录里（第一版就栽在这，
    //   报「backend_kill.rs 生产段却没有 route_call_error」—— 其实是文件根本没读到）。
    let mut by_name: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut all_prod: Vec<(String, String)> = Vec::new();
    // 〔P4〕通道编进共享 crate `chan-core` 之后，路由器那份不再挂在 monitor 的模块树上 —— 人群照旧：monitor 的 manifest 明写了它
    //   （`guard_core::population_trees`，`walk_tree` 顺着收）。
    for (p, src) in guard_core::scan_tree!(&dir, &["rs"]) {
        let prod = guard_core::production_code(&src);
        all_prod.push((p.to_string_lossy().replace('\\', "/"), prod.clone()));
        if prod.contains(verb.as_str()) || prod.contains(verb_until.as_str()) {
            senders.push(p.file_name().unwrap().to_string_lossy().to_string());
            by_name.insert(
                p.file_name().unwrap().to_string_lossy().to_string(),
                prod.clone(),
            );
        }
    }
    senders.sort();
    // ★ 抽取器自检：遍历坏掉时下面几条会零命中地绿。
    assert!(
        senders.len() >= 3,
        "只扫到 {} 个走后端的发送端（{senders:?}）—— 遍历坏了。\n\
             F12 实测是 3 个：backend_kill.rs · backend_launch.rs · backend_send_keys.rs",
        senders.len()
    );
    let mut registered: Vec<String> = SENDERS.iter().map(|(f, _)| f.to_string()).collect();
    registered.sort();
    assert_eq!(
        senders, registered,
        "**整棵 monitor 源码树**里走后端的发送端与登记表对不上。\n\
             ⚠ **新增一个发送端就必须在这里表态**：要么用共用分流器 `route_call_error`，\n\
             要么写成带理由的刻意例外。**手写清单看不见新文件** —— 那正是 F12 的\n\
             `/full-audit` 在本守卫身上逮到的东西（第三个发送端整个逃出了扫描面）。"
    );
    // ★ 两个出口**各恰好一处定义**，就在 `backend_route.rs`：有人在别处另起一份同名函数
    //   （分流规则的第二份实现换个住址），`UsesRouter` 那条「认名字」就会被它骗过去。
    for exit in ROUTER_EXITS {
        let homes: Vec<(String, usize)> = all_prod
            .iter()
            .map(|(p, prod)| (p.clone(), definitions_of(prod, exit)))
            .filter(|(_, n)| *n > 0)
            .collect();
        assert!(
            homes.len() == 1
                && homes[0].1 == 1
                && homes[0].0.ends_with("comms/inward/backend_route.rs"), // 〔RE〕通信层成员的家
            "分流器出口 `{exit}` 的定义应当**恰好一处、住 `backend_route.rs`**，实得 {homes:?}"
        );
    }
    for (name, verdict) in SENDERS {
        let prod = by_name
            .get(*name)
            .unwrap_or_else(|| panic!("`{name}` 在登记表里但发现阶段没扫到 —— 上面那条已保证不会"));
        let uses = ROUTER_EXITS
            .iter()
            .any(|x| guard_core::contains_word(prod, x));
        // 运行时拼，免得命中本行自己。
        let needle = format!("CallError::{}", "");
        let own = prod.contains(needle.as_str());
        match verdict {
            Verdict::UsesRouter => {
                assert!(
                    uses,
                    "`{name}` 登记为用分流器，生产段却两个出口（{ROUTER_EXITS:?}）一个都没用"
                );
                assert!(
                    !own,
                    "`{name}` 的生产段自己在 match `CallError` —— 那是分流规则的第二份实现。\n\
                         它一旦与本模块漂开，一次 `wrong_owner` 就可能被另一条路重做一遍。"
                );
            }
            Verdict::ProbeOnlyNoFallbackDecision => {
                // ★ **「不做回落决策」这件事本身也要钉**：不许自己 match `CallError`
                //   去分流（那就是第二份分流规则），也不必调分流器。
                //   只写「它是探测」而不判任何东西 = 把登记表变成免检章。
                assert!(
                    !uses,
                    "`{name}` 登记为「探测型、不做回落决策」，却在用 `route_call_error` —— \
                         那说明它其实有回落决策，登记该改成 `UsesRouter`"
                );
                assert!(
                    !own,
                    "`{name}` 登记为「探测型、不做回落决策」，却自己在 match `CallError` —— \
                         那就是第二份分流规则。要么改用 `route_call_error`，要么说清它判的不是回落。"
                );
            }
            Verdict::PureRouterNoFallbackDecision => {
                // ⚠ 这一档**不查** `own`：它的 `CallError::` 是通道自己的类型。
                //   能不查的前提是下面这条把 inbound 那个枚举整个挡在文件外。
                let bad = pure_router_violations(prod);
                assert!(
                    bad.is_empty(),
                    "`{name}` 登记为「纯路由器、不做回落决策」，生产段却：\n  {}\n\
                         ⇒ 它已经不只是转交注入的句柄了 —— 重新表态（多半是 `UsesRouter`）。",
                    bad.join("\n  ")
                );
            }
            Verdict::ExemptPendingF14 => {
                // ★ **例外也要钉**：它今天确实还没用分流器（那是 F14 的活）；
                // 一旦它改好了，本条会红 —— 逼人把登记改成 `UsesRouter`。
                assert!(
                    !uses,
                    "`{name}` 已经在用 `route_call_error` 了 —— **这多半是好事**（F14 做完了）：\n\
                         把它的登记从 `ExemptPendingF14` 改成 `UsesRouter`，并关掉 ROADMAP 的 F14。"
                );
            }
        }
    }
}

/// `PureRouterNoFallbackDecision` 的牙**活着**：四个名字各喂一份违例样本必须认出来，
/// 一份只调注入句柄的干净样本必须不认；被撑大的名字（`inbound_clients` 之类）不误判。
#[test]
fn the_pure_router_verdict_has_teeth() {
    let clean = "pub fn serve(b: Arc<dyn Backends>) { let f = b.call(o, op, body, left, c); \
                 let e = CallError::Ours { why: OursFault::Cancelled }; }\n";
    assert!(
        pure_router_violations(clean).is_empty(),
        "干净的纯路由器被判成违例：{:?}",
        pure_router_violations(clean)
    );
    for (bad, what) in [
        ("let r = route_call_error(&e, f);\n", "route_call_error"),
        ("use crate::inbound_client::CallError;\n", "inbound_client"),
        ("fn f(c: Arc<InboundClient>) {}\n", "InboundClient"),
        ("let c = client_for(&o);\n", "client_for"),
        ("let l = layer_call_error(&e, 1);\n", "layer_call_error"),
        ("let l = layer_no_channel(1);\n", "layer_no_channel"),
    ] {
        let hits = pure_router_violations(bad);
        assert_eq!(
            hits.len(),
            1,
            "`{what}` 那一形没被认出来（或被认成了别的）：{hits:?}"
        );
    }
    assert!(
        pure_router_violations("let inbound_clients_seen = 0; let my_client_format = 1;\n")
            .is_empty(),
        "被撑大的标识符被当成了那几个名字 —— 假红"
    );
}

/// ★★ **收拢表逐档穷举**〔面 A 通道那一拍，2026-09-24〕—— `route_call_error` 对
/// `inbound_client::CallError` 的**每一个变体**，产出的三态**连同那句话**逐字节钉死。
///
/// # 两侧为什么异源
///
/// 右侧（期望值）是**字面量**，抄自改动之前那份 `route_call_error` 在同一输入上的产出
/// —— 先在旧实现上跑绿、再动实现（「分层判定只许一份，三态从它收拢」那一拍）。
/// 它**不**调新的分层函数自证：拿新函数去算期望值，改错了两边一起错，恒真。
///
/// # 买到 / 买不到
///
/// **买到**：收拢前后同一输入 ⇒ 同一三态、同一句话（kill/launch/send_keys/cc_bus/tmux/find
/// 六个调用方看到的一个字节都没变）。
/// **买不到**：每个变体只喂了**一个**样本（字段值取一种）；字段值不同的输入由各调用方自己的判据管。
#[test]
fn the_collapse_to_three_states_is_byte_identical_to_the_table_before_layering() {
    // 穷尽见证：`inbound_client::CallError` 多一个变体，这里编译不过 —— 逼人回来补下表。
    fn witness(e: &CallError) {
        match e {
            CallError::Unsupported { .. }
            | CallError::Unavailable { .. }
            | CallError::TooManyPending
            | CallError::Disconnected
            | CallError::Cancelled
            | CallError::Timeout { .. }
            | CallError::Remote { .. } => {}
        }
    }
    // 〔CP2b · CP1 裁「改·§2.4」〕说法换了（去掉 ** 与「另一条路」，不点命令名与声明的能力），三态一格没动。
    // 〔FIX2 · 99 §2.1 ㉛②〕那句话按文案键断言、不抄原文：三态（NoChannel / Refused）与取的是哪一条仍逐格钉死。
    let t = |k: &str| copy_text(k, &[]);
    let unsure = |s: String| copy_text("rsBackendRoute.route.unsure", &[("s", &s)]);
    let table: Vec<(CallError, Routed)> = vec![
        (
            CallError::Unsupported {
                cmd: "kill".into(),
                offered: vec!["ping".into(), "cancel".into()],
            },
            Routed::NoChannel(t("rsBackendRoute.layer.unsupported")),
        ),
        (
            CallError::TooManyPending,
            Routed::NoChannel(t("rsBackendRoute.layer.tooMany")),
        ),
        (
            CallError::Disconnected,
            Routed::Refused(unsure(t("rsInboundClient.error.closed"))),
        ),
        (
            CallError::Timeout {
                after: Duration::from_millis(1500),
                withdraw: crate::chan::wire::Withdraw::Asked,
            },
            Routed::Refused(unsure(copy_text(
                "rsInboundClient.error.timeout",
                &[("after", "1500")],
            ))),
        ),
        (
            CallError::Cancelled,
            Routed::Refused(unsure(t("rsInboundClient.error.cancelled"))),
        ),
        (
            CallError::Remote {
                code: "wrong_owner".into(),
                message: "sid=x".into(),
            },
            Routed::Refused("wrong_owner/sid=x".into()),
        ),
        (
            CallError::Unavailable {
                cmd: "kill".into(),
                code: "no_tmux".into(),
            },
            Routed::Refused(format!(
                "no_tmux/{}",
                copy_text("rsInboundClient.error.unavailable", &[("code", "no_tmux")])
            )),
        ),
    ];
    assert_eq!(table.len(), 7, "收拢表的行数与穷尽见证的变体数对不上");
    for (e, want) in table {
        witness(&e);
        assert_eq!(
            route_call_error(&e, plain),
            want,
            "`{e:?}` 收拢出来的三态（或那句话）变了 —— 旧三态一个字节都不许变"
        );
    }
}

/// `definitions_of` 认得出定义、不把调用与被撑大的名字算进去。
#[test]
fn the_exit_definition_counter_sees_definitions_only() {
    let src = "pub(crate) fn route_call_error(e: &E) {}\n\
               fn layer_call_error<T>(x: T) {}\n\
               let r = route_call_error(&e, f);\n\
               fn route_call_error2() {}\n\
               fn my_route_call_error() {}\n";
    assert_eq!(definitions_of(src, "route_call_error"), 1);
    assert_eq!(definitions_of(src, "layer_call_error"), 1);
}

/// ★★ **分层表逐档穷举** —— `layer_call_error` 对 `inbound_client::CallError` 每一个变体的
/// `05 §3.3.1` 形状（层 · `reach` · `why` · 跳号标签 · 不透明 body）逐格钉死。
///
/// 期望值是**字面量**（与 `layer_call_error` 头注那张分层表逐行对应），不调任何映射函数去算。
/// 收拢那一侧由 `the_collapse_to_three_states_is_byte_identical_to_the_table_before_layering` 管；
/// 两张字面表各钉一层，改错哪一层红哪一张。
#[test]
fn the_layering_table_is_pinned_cell_by_cell() {
    use crate::chan::wire as w;
    fn witness(e: &CallError) {
        match e {
            CallError::Unsupported { .. }
            | CallError::Unavailable { .. }
            | CallError::TooManyPending
            | CallError::Disconnected
            | CallError::Cancelled
            | CallError::Timeout { .. }
            | CallError::Remote { .. } => {}
        }
    }
    let hop = |tag: &'static str, reach: w::Reach, why: w::HopFault| w::CallError::Hop {
        at: w::HopId { idx: 7, tag },
        reach,
        why,
    };
    let table: Vec<(CallError, w::CallError)> = vec![
        (
            CallError::Unsupported {
                cmd: "kill".into(),
                offered: vec![],
            },
            w::CallError::Peer {
                why: w::PeerFault::Unsupported,
            },
        ),
        (
            CallError::TooManyPending,
            hop("write", w::Reach::NotSent, w::HopFault::Overrun),
        ),
        (
            CallError::Disconnected,
            hop("read", w::Reach::Unknown, w::HopFault::Dropped),
        ),
        (
            CallError::Timeout {
                after: Duration::from_millis(5),
                withdraw: crate::chan::wire::Withdraw::Asked,
            },
            hop("wait", w::Reach::Unknown, w::HopFault::Overrun),
        ),
        (
            CallError::Cancelled,
            w::OursFault::Cancelled.into(),
        ),
        (
            CallError::Remote {
                code: "wrong_owner".into(),
                message: "x\u{0}y".into(),
            },
            w::CallError::Peer {
                why: w::PeerFault::Refused {
                    body: w::Body(br#"{"code":"wrong_owner","message":"x\u0000y"}"#.to_vec()),
                },
            },
        ),
        (
            CallError::Unavailable {
                cmd: "kill".into(),
                code: "no_tmux".into(),
            },
            w::CallError::Peer {
                why: w::PeerFault::Refused {
                    body: w::Body(
                        r#"{"code":"no_tmux","message":"这台机器做不到这件事（no_tmux），没有发出去"}"#
                            .as_bytes()
                            .to_vec(),
                    ),
                },
            },
        ),
    ];
    assert_eq!(table.len(), 7, "分层表的行数与穷尽见证的变体数对不上");
    for (e, want) in table {
        witness(&e);
        assert_eq!(
            layer_call_error(&e, 7).error,
            want,
            "`{e:?}` 的分层结果不是那张表那一格"
        );
    }
    assert_eq!(
        layer_no_channel(7),
        hop("open", w::Reach::NotSent, w::HopFault::Unreachable),
        "「没有控制通道」那一格不是 `Hop{{open, NotSent, Unreachable}}`"
    );
}
