//! P2s（定框 `C8`）：**本机后端的生命周期** —— 起 / 停 / 状态。
//!
//! 违反此约束见 `src/doc/INVARIANTS.md` § 48（本机常驻后端的宿主三条：门由内核给 · 不留僵尸 · 测试隔离用户 tmux；照「修改本文档」第 2 条补的反指）。
//!
//! # 为什么它不住 `lib.rs`
//!
//! ⚠ **本段刻意不把那个宏的名字连着 `!` 写全** —— `parity_ledger` 的命令清单解析器取的是
//! `lib.rs` 里**第一处**那个字面量，散文里写全会被它当成清单去解析（实测：只解析出 1 条命令）。
//! 这已经是本轮第二次被自己的散文绊到（另一次是位置比较判据扫 `rfind` 带括号那个）。
//!
//! `backend_status` 是 `#[tauri::command]`，而 tauri 的 IPC 命令清单宏会把命令的辅助宏
//! **`use` 回它所在的那个模块** ⇒ 命令与 `generate_handler!` 同住一个模块必然重名冲突
//! （实测 `E0255: __cmd__backend_status is defined multiple times / reimported here`）。
//! 仓里每条命令都写成 `模块::名字`，正是这个缘故。**这是结构性理由，不是嫌 `lib.rs` 长。**
//!
//! # 它与 `local_backend.rs` 的分工
//!
//! 那边是**平台无关的监护机制**（起、看住、判死、重起）。这边是**宿主知识**：
//! 落点目录在哪、当前 arch 是什么、平台怎么置可执行位、句柄存哪。
//! **`backend-split` 的 C10**〔用 08-01〕要求前者不认识后者，所以两边不能合并。
//!
//! ⚠ **编号在跨工作区之间会撞**：`control-parity` 也有一条 C10，讲的是**单 exe 内嵌自释放**
//! （完全不同的事）。一次补审就是因为只读了后者的定框，把这里的引用判成了「引错 charter」。
//! ⇒ 代码里引 charter **一律带工作区名**，裸编号在几个月后没人分得清指哪一条。

use crate::copy_table::copy_text;
use crate::detail::Said;
use crate::local_backend::{self, Resolved, SuperviseHandle};

/// F05a：本机后端监护句柄。存起来是为了退出前按策略 `stop()`（`C8`）。
///
/// ⚠ **P2s 把它从 `OnceLock` 换成了 `Mutex<Option<_>>`，理由是结构性的**：
/// `SuperviseHandle::stop()` 把 `stopping` 永久置位、并杀掉当前子进程 ⇒ **那个句柄之后就是死的**，
/// 再起必须换一个新的。`OnceLock` 写一次就锁死 ⇒ 有「停」就不可能有「再起」，
/// 而 `C8`② 要的正是「起 / 停 / 状态」三件。
pub static LOCAL_BACKEND: std::sync::Mutex<Option<SuperviseHandle>> = std::sync::Mutex::new(None);

/// 起 / 停本机后端**串行**的那一把锁：一趟起或停从头到尾攥着它（「查有没有 → 起 → 存句柄」之间不许开窗口，
/// 理由见 [`start_local_backend`] 头注）。句柄表（[`LOCAL_BACKEND`] · [`DETACHED`]）只在读写那一下拿 ⇒
/// 查状态（`backend_status`）与退出那一臂不会被一趟几十秒的起 / 停挡住。锁序：本锁 → `LOCAL_BACKEND` → `DETACHED`。
static LIFECYCLE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// P2s（`C8`②）：**起本机后端并把句柄存进 `LOCAL_BACKEND`**。
///
/// 从 `run()` 里抽出来，因为「再起」要走同一条路 —— 抽出来之前，
/// 那段宿主知识（落点目录 / 当前 arch / 平台注入）只在 `run()` 的一个块里存在，
/// 「停了还能起回来」就无从谈起。
///
/// **已经在跑就不重复起**：`C8`① 是「每台机各一个」。
///
/// # ⚠ 诚实边界 11b：这里的「一个」只到「**一个 monitor 进程内一份**」
///
/// 判据是 `LOCAL_BACKEND`（进程内的 `Mutex<Option<_>>`）⇒ 同机**两个 monitor 进程**
/// 仍然是两个后端，而且互相认不到。
///
/// 单实例插件各平台都注册（`lib.rs` 那处），但 Linux 上它靠会话总线：没有会话总线（或两个登录会话）时
/// 两个 monitor 仍能并存，会撞同一个 `~/.cc-monitor/bin/.<name>.partial`（临时名带进程 id 治的就是这个）。
/// 起本机后端的结局 —— **三态，不是两态**〔D 阶段补审 08-11 新增，A6〕。
///
/// 原来三种结局全塞在 `Resolved` 里：`Found` 与两种 `Missing`（「已经在跑」与「起不来」）。
/// 而 `backend_control::backend_start` 把 `Missing{reason}` 当 `Ok(reason)` 返回
/// ⇒ 「没内嵌后端」「释放失败」「已经在跑」三种完全不同的结局在前端**都走 `console.info`**，
/// **一个 toast 都不弹**（补审 A6）。
///
/// 要分开就得在类型上分开 —— 靠 `reason` 字符串去猜是哪一种，是下一个人一定会写错的东西。
pub enum StartOutcome {
    /// 起来了。
    Started(std::path::PathBuf),
    /// 已经在跑（`C8`①）—— **不是失败**。
    AlreadyRunning,
    /// 起不来：没内嵌 / 释放失败 / exe 旁边也没有。
    /// `reason` 是给人看的那一句 ＋ 复制详情（下层原话在详情里，不在句子里）。
    Failed {
        reason: Said,
        looked_at: Vec<std::path::PathBuf>,
    },
}

impl StartOutcome {
    /// 起不来那一支的两个字段。`None` = 这一次不是「从来没起来」。
    ///
    /// # ⚠ 这里为什么写 `Self::` 而不是把类型名写全〔`K-P3b`，别改回去〕
    ///
    /// 同文件那条 [`tests::the_user_actionable_start_failures_all_reach_the_user`] 的第 ① 格
    /// 数的是**字面** `StartOutcome::Failed {`，它的分母自称是「**失败的构造点**」——
    /// 而**解构与构造在 Rust 里长得一模一样**。
    /// 照那个写法写这一处，那把尺子会多数出一处**不需要分档的东西**，
    /// 而它红出来的诊断（「新增一处失败就要给它分档」）**是假的**
    /// —— 本文件逐字记过：「假诊断比不红更贵：它把人引到错的地方」。
    /// ⇒ 这一处写 `Self::`：同一件事，而那把尺子的分母仍然只装构造点。
    fn failure(&self) -> Option<(&str, &[std::path::PathBuf])> {
        match self {
            Self::Failed { reason, looked_at } => {
                Some((reason.said.as_str(), looked_at.as_slice()))
            }
            Self::Started(_) | Self::AlreadyRunning => None,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
// `K-P1`：常驻那条路 —— **真脱离 · 一个监听口 · 起时认得出已有实例**
//
// 这一段全部住在**宿主知识层**，理由是硬的（现打）：
// `process_group(0)` 来自 `std::os::unix::process::CommandExt`，而 `std::os::unix`
// 在 `backend_client_guard_tests.rs::the_backend_half_stays_platform_agnostic` 的禁针里
// ⇒ **写进 `backend/` 当场红**；而「加一条平台例外」这条路被**递减棘轮**堵着
// （`PLATFORM_EXCEPTIONS`：今天 0 条、上限 0；壳里平台形态另只许住 `platform/`，`platform_home_guard` 钉）。
// ⇒ 落点只能是这里，形状照 `platform::fs::make_executable` 那个**注入**先例。
// ══════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════
// 🪦〔散文墓碑〕 `CREATE_NO_WINDOW` / `hide_console_window` **搬走了**
//
// 那是止血，它自己的头注逐字写着终局：
// 「正确形状是 `spawn_managed(bin, args, ConsolePolicy, Lifetime, StderrSink)` ……
//   本函数是那个枚举的 `Hidden` 分支**提前落一处**，代价写明：`backend/` 这一侧因此
//   多了一条 `crate::local_backend_host::` 的反向边（A3 落地时它会被换成注入参数，
//   和 `make_executable` 一样）。」
//
// ⇒ 今天就是那一天：`ConsolePolicy::Hidden` 住 `spawn_managed.rs`，
// 那条反向边换成了 `local_backend::supervise_with_stdio` 的 `spawn` 注入参数。
// ⚠ 那条止血头注里的另一句也一起搬过去了，一个字没丢：
// **别把 `Hidden` 铺到 `platform/terminal.rs::launch_powershell_window` 头上** —— 它是 `NewVisible`。
// ═══════════════════════════════════════════════════════════════════════════

/// 握手那一行（hello / attach 应答）的字节上限。
///
/// hello 帧本机实测 ~1.1 KB（能力集 + emits + commands 三张表）。8 KiB 给了 7 倍余量，
/// 同时把「对端一直发字节不发换行」这条路堵死 —— 这条连接的对端是**同机任何进程**，
/// 不是我们自己的子进程，不能假设它讲道理。
/// 超限语义：**拒收 + 出声**（不静默截断成一行「看起来对」的 JSON）。
/// **登记住址** `tests/frontend/shell/byte_cap_registry.rs`（那张表默认拒绝：不登记就红）。
pub(crate) const LISTEN_HANDSHAKE_LINE_CAP: usize = 8 * 1024;

/// backend 那侧收「你是常驻的那一个」的 env 名（值 `1`）。听哪个套接字不交：按家算（`relay_route_core::listen_socket_for`），
/// 两边同一个函数。
///
/// ⚠ **跨 crate 字面量**：backend 那边是 `src/backend/stream/listen.rs::ENV_RESIDENT`。
/// 两边漂了**不会报错** —— 起出来的后端会当成「没设」而走 stdio 那条路，
/// 于是宿主等在一个永远不会有人绑的套接字上，日志里只有一句「连不上」。
/// 由 `the_listen_env_names_are_the_same_string_on_both_sides` 逐字对拍。
pub(crate) const RESIDENT_ENV: &str = "CCM_RESIDENT";

/// 关掉「脱离」的逃生口。**存在的理由不是好心，是判据**：
/// `KPY5` 要一格「起的时候**没走**脱离那条路 ⇒ `detached` 必须为假」的**负例**，
/// 而没有这个开关，那一支在 Linux 上永远走不到 —— 那正是「只有正例的测试永远绿」那一形。
pub const NO_DETACH_ENV: &str = "CCM_NO_DETACH";

/// 「谁在听」的住址（常驻后端拿到锁之后自己写；本宿主只读）。
fn pid_path(dir: &std::path::Path) -> std::path::PathBuf {
    relay_route_core::listen_pid_for(dir)
}

// `~/.cc-monitor` 这一层建的那一下就只给本人：那个函数住 `platform::fs::ensure_private_dir`，
//   与释放后端二进制那几处（`local_backend.rs`，经注入）共用一份。

/// 解那两行。**纯函数**，所以「只有一行」「pid 不是数字」这些残缺形都测得到 ——
/// 这个文件是上一个 monitor 写的，它可能被中途打断、可能是旧版本写的。
pub(crate) fn parse_listen_owner(body: &str) -> Option<(u32, std::path::PathBuf)> {
    let mut lines = body.lines();
    let pid: u32 = lines.next()?.trim().parse().ok()?;
    if pid == 0 {
        return None;
    }
    let bin = lines.next()?.trim();
    if bin.is_empty() {
        return None;
    }
    Some((pid, std::path::PathBuf::from(bin)))
}

fn read_listen_owner(dir: &std::path::Path) -> Option<(u32, std::path::PathBuf)> {
    parse_listen_owner(&std::fs::read_to_string(pid_path(dir)).ok()?)
}

/// 一条 hello 行的裁决。**纯函数**，所以三张脸都测得到。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HelloVerdict {
    /// 是我们的后端：build_id 与宿主交给它的那几格（中转口 · 家）都对得上。
    Ours,
    /// 有人在听，但**不是**我们要找的那个（另一份构建 · 另一个家）。带上说得清的理由。
    Stranger(String),
}

/// 判那一行 hello。
///
/// ⚠ **连得上只说明「有一个后端在听」，不说明它是我们要的那一版、住我们这个家。**
/// ⇒ 连上去**先读 hello 比对**，对不上就出声并拒绝，**不许静默复用**
/// （`P2t §1` 第 3 问「陈旧端点怎么识别」问的正是这一格）。
///
/// 第三项：`want_env` = 这一趟要交给后端的那份环境；其中名在 [`HANDED_ENVS`] 的那几格必须与 hello 的
/// `host_env`（后端原样回显它被交的那几格）**两向相等** —— 其中 `CCM_DATA_DIR` 那一格就是「它住哪个家」：
/// 套接字虽按家算，接上另一个家的后端就会把凭据与历史注解写进那个家（两向比是兜底）。
///
/// 第二项：`want_build` = 「我这一版」（`byte_table::my_backend_id`）。`None`（手上没带后端字节）⇒ 不比 build、
/// 照那台报的接（没有对照物，比了也只是拿一个不存在的值去拒人）；家与中转口那几格照比。
pub(crate) fn hello_verdict(
    line: &str,
    want_build: Option<&str>,
    want_env: &[(String, String)],
) -> HelloVerdict {
    let Ok(frame) = crate::stream_source::parse_frame(line) else {
        return HelloVerdict::Stranger(copy_text(
            "rsLocalBackendHost.hello.notBackendLine",
            &[("bytes", &(line.len()).to_string())],
        ));
    };
    let crate::stream_source::InboundFrame::Hello { build_id, .. } = &frame else {
        return HelloVerdict::Stranger(
            copy_text("rsLocalBackendHost.hello.notBackend", &[]).into(),
        );
    };
    if let Some(ours) = want_build.filter(|w| build_id != w) {
        return HelloVerdict::Stranger(copy_text(
            "rsLocalBackendHost.hello.buildMismatch",
            &[
                ("theirs", &build_id.to_string()),
                ("ours", &ours.to_string()),
            ],
        ));
    }
    // 它看哪个 Claude 目录不比：那是那台后端自己解析的（门牌按家算，与 Claude 目录无关）。
    if let Some(why) = host_env_mismatch(line, want_env) {
        return HelloVerdict::Stranger(why);
    }
    HelloVerdict::Ours
}

/// 起本机后端时交给它、且它会在 hello 里原样回显的那几格环境的**名字**（后端那一侧 `wire::HOST_ECHO_ENVS`，
/// 两向对拍）：中转端口 · 家（只在隔离跑、设了 `CCM_DATA_DIR` 时才有这一格）。
/// 凭据与历史注解的位置不交：那台后端按家自己推（位置只跟着家走）。
pub(crate) const HANDED_ENVS: [&str; 2] = ["CCM_RELAY_PORT", crate::config::DATA_DIR_ENV];

/// hello 的 `host_env` 与这一趟要交的那几格（名在 [`HANDED_ENVS`] 的）两向比；不等 ⇒ 一句点名哪一格、两边各是什么的话。
/// hello 里没有 `host_env`（旧后端 / 一格都没被交）⇒ 当空表比。**纯函数**。
fn host_env_mismatch(line: &str, want_env: &[(String, String)]) -> Option<String> {
    let theirs: std::collections::BTreeMap<String, String> =
        serde_json::from_str::<serde_json::Value>(line.trim())
            .ok()
            .and_then(|v| v.get("host_env").and_then(|h| h.as_object()).cloned())
            .map(|m| {
                m.into_iter()
                    .filter_map(|(k, v)| Some((k, v.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default();
    let ours: std::collections::BTreeMap<String, String> = want_env
        .iter()
        .filter(|(k, _)| HANDED_ENVS.contains(&k.as_str()))
        .cloned()
        .collect();
    if theirs == ours {
        return None;
    }
    let say = |m: &std::collections::BTreeMap<String, String>, k: &str| {
        m.get(k)
            .cloned()
            .unwrap_or_else(|| copy_text("rsLocalBackendHost.dataDir.absent", &[]))
    };
    let diffs: Vec<String> = HANDED_ENVS
        .iter()
        .filter(|k| theirs.get(**k) != ours.get(**k))
        .map(|k| {
            copy_text(
                "rsLocalBackendHost.dataDir.slot",
                &[
                    ("name", &k.to_string()),
                    ("theirs", &say(&theirs, k)),
                    ("ours", &say(&ours, k)),
                ],
            )
        })
        .collect();
    Some(copy_text(
        "rsLocalBackendHost.dataDir.otherDataDir",
        &[(
            "diffs",
            &diffs.join(&copy_text("rsLocalBackendHost.dataDir.slotSep", &[])),
        )],
    ))
}

/// 探那个套接字上有没有一个**我们的** backend。
pub(crate) enum Probe {
    /// 没人在听。
    Nobody,
    /// 是我们的那个。把连接与它的 hello 行原样交出来（**别再连第二次** ——
    /// 第二次连的可能已经是另一个进程了）。
    Ours(own_chan::BlockingStream, String),
    /// 有人占着，但不是我们的。**出声**。
    Stranger(Said),
}

/// 一次握手的读写期限。
///
/// 它**不是定时器**：`SO_RCVTIMEO`/`SO_SNDTIMEO` 说的是「**这一次**阻塞的读写最多等多久」，
/// 有字节就立刻返回、没字节就报错返回，不让任何线程自己醒来、不驱动任何循环。
/// 值给 3 秒：对端**就在本机**，一行 ~1 KB 的 hello 是微秒级的事；
/// 3 秒比它高五六个量级，而它同时保证「起 monitor 时不会被一个哑的后端卡住」。
const HANDSHAKE_DEADLINE: std::time::Duration = std::time::Duration::from_millis(3_000);

fn probe_listen_socket(sock_path: &std::path::Path, want_env: &[(String, String)]) -> Probe {
    let sock = match own_chan::connect_blocking(sock_path) {
        Ok(s) => s,
        // 文件不在 / 拒连 = 没人在听。这是**最常见**的那一格（第一次起 monitor）。
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
            ) =>
        {
            return Probe::Nobody
        }
        // 别的（不归本人 · 路径坏了）：不是「没人」—— 起一个也抢不到那个家，出声。
        Err(e) => {
            return Probe::Stranger(Said::with_raw(
                copy_text(
                    "rsLocalBackendHost.probe.cannotConnect",
                    &[
                        ("path", &sock_path.display().to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                &e,
            ))
        }
    };
    if let Err(e) = sock
        .set_read_timeout(Some(HANDSHAKE_DEADLINE))
        .and_then(|()| sock.set_write_timeout(Some(HANDSHAKE_DEADLINE)))
    {
        return Probe::Stranger(Said::with_raw(
            copy_text(
                "rsLocalBackendHost.probe.noDeadline",
                &[("why", &copy_core::io_reason(e.kind()))],
            ),
            &e,
        ));
    }
    let line = match read_handshake_line(&sock) {
        Ok(l) => l,
        Err(e) => {
            return Probe::Stranger(Said::restate(
                copy_text(
                    "rsLocalBackendHost.probe.noAnswer",
                    &[("path", &sock_path.display().to_string()), ("why", &e.said)],
                ),
                e,
            ))
        }
    };
    let mine = crate::byte_table::my_backend_id();
    match hello_verdict(&line, mine, want_env) {
        HelloVerdict::Ours => {
            crate::machine_state::note_local_foreign(None);
            Probe::Ours(sock, line)
        }
        HelloVerdict::Stranger(why) => {
            // 在听的是另一份构建（终端里先起的那一份）⇒ 本机那一台的状态成品按两边构建的序说。
            if let Ok(crate::stream_source::InboundFrame::Hello { build_id, .. }) =
                crate::stream_source::parse_frame(&line)
            {
                if mine.is_some_and(|m| m != build_id) {
                    crate::machine_state::note_local_foreign(Some(&build_id));
                }
            }
            Probe::Stranger(why.into())
        }
    }
}

/// 从一条 socket 上读**一行**，字节数有上限。
///
/// 逐字节读：这条握手只有一两行、每行 ~1 KB，而**带缓冲的读会读过头** ——
/// 读过头的那些字节留在 `BufReader` 里，而这条 socket 之后要原样交给流那一档，
/// 那几个字节就凭空丢了（第一版就是这么写的）。
/// 形状抄 `relay/http1::read_head` 的 `r.read(&mut one)?`。
fn read_handshake_line(mut sock: &own_chan::BlockingStream) -> Result<String, Said> {
    use std::io::Read;
    let mut out: Vec<u8> = Vec::new();
    let mut one = [0u8; 1];
    loop {
        match sock.read(&mut one) {
            Ok(0) => return Err(copy_text("rsLocalBackendHost.handshake.closed", &[]).into()),
            Ok(_) => {
                if one[0] == b'\n' {
                    return String::from_utf8(out).map_err(|e| {
                        Said::with_raw(copy_text("rsLocalBackendHost.handshake.notUtf8", &[]), &e)
                    });
                }
                out.push(one[0]);
                if out.len() > LISTEN_HANDSHAKE_LINE_CAP {
                    // **拒收 + 出声**，不静默截断成一行「看起来对」的 JSON。
                    return Err(copy_text(
                        "rsLocalBackendHost.handshake.tooLong",
                        &[("cap", &LISTEN_HANDSHAKE_LINE_CAP.to_string())],
                    )
                    .into());
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                return Err(Said::with_raw(
                    copy_text(
                        "rsLocalBackendHost.handshake.readFailed",
                        &[("why", &copy_core::io_reason(e.kind()))],
                    ),
                    &e,
                ))
            }
        }
    }
}

/// attach 行：「我要流」＋ 看的这一台的时区（同远端那一条 `remote_resident::attach_line` 的 `tz`；`None` ⇒ 不带，后端按 UTC 写推来的钟面）。
pub(crate) fn attach_line(tz: Option<&str>) -> String {
    match tz {
        Some(tz) => format!("{}\n", serde_json::json!({ "attach": true, "tz": tz })),
        None => "{\"attach\":true}\n".to_string(),
    }
}

/// 交 attach 行，换一句「可以」。门在连上那一刻（套接字只给本人 ＋ 对端 uid），这一行只是「我要流」。
fn send_attach(sock: &own_chan::BlockingStream) -> Result<(), Said> {
    use std::io::Write;
    let mut w = sock;
    w.write_all(attach_line(host_core::viewer_tz().as_deref()).as_bytes())
        .and_then(|()| w.flush())
        .map_err(|e| {
            Said::with_raw(
                copy_text(
                    "rsLocalBackendHost.attach.sendFailed",
                    &[("why", &copy_core::io_reason(e.kind()))],
                ),
                &e,
            )
        })?;
    let line = read_handshake_line(sock)?;
    let v: serde_json::Value = serde_json::from_str(line.trim()).map_err(|e| {
        Said::with_raw(
            copy_text("rsLocalBackendHost.attach.notJson", &[]),
            format!("{e}\n{line}"),
        )
    })?;
    match v.get("attach").and_then(|x| x.as_str()) {
        Some("ok") => Ok(()),
        // 对面**出声地**拒了 —— 把它的理由原样带上来，别翻译成一句更含糊的话。
        Some("refused") => {
            let reason = v.get("reason").and_then(|x| x.as_str()).unwrap_or("?");
            Err(copy_text(
                "rsLocalBackendHost.attach.refused",
                &[("reason", &reason.to_string())],
            )
            .into())
        }
        _ => Err(copy_text(
            "rsLocalBackendHost.attach.unreadable",
            &[("line", &line.to_string())],
        )
        .into()),
    }
}

/// 起一个**真脱离**的后端。
///
/// 三样一起才叫脱离，缺一样都不算：
/// - **`process_group(0)`** —— 否则终端里 Ctrl-C 的 SIGINT 会打到整个前台进程组，
///   monitor 和它一起走。
/// - **stdio 全 null** —— 今天它死掉的**真正原因**就在这儿：`Stdio::piped()` 之后
///   宿主一退读端就断，它在 **153 毫秒**内 broken-pipe 退出（`backend_policy.rs` 头注实测）。
/// - **协议改走这台家里的套接字** —— 管子没了就得有别的说话方式，那正是 `listen::ENV_RESIDENT`。
///
/// ⚠ **`process_group` 不改变父子关系** ⇒ **不 `wait` 就留僵尸**
/// （`launch.rs:196-198` 头注逐字）。收尸走 [`reap_detached`]，由「流断了」这个**事件**触发，
/// 不是轮询。monitor 自己退出之后那个进程被 init 接管，由 init 收 —— 那一格不归我们。
///
/// 哪些平台能脱离由 `platform::proc::CAN_DETACH` 答（原先是这里两份 `cfg` 分身）；不能 ⇒ 那句「不支持」照旧。
fn spawn_detached(
    bin: &std::path::Path,
    extra_env: &[(String, String)],
) -> Result<crate::spawn_managed::ManagedChild, Said> {
    use crate::spawn_managed::{managed_spawner, ConsolePolicy, Lifetime, StderrSink};
    if !crate::platform::proc::CAN_DETACH {
        return Err(copy_text(
            "rsLocalBackendHost.spawn.unsupported",
            &[("bin", &(bin.display()).to_string())],
        )
        .into());
    }
    let mut cmd = std::process::Command::new(bin);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    // 流模式起参与 stdio 那条载体共用一份（`local_backend::LOCAL_STREAM_ARGS`：`--tail-only --with-bg`；
    // ＋ `--with-pid`，让 `session_added` 带 pid）。
    cmd.args(local_backend::LOCAL_STREAM_ARGS)
        // ★★ **`TMUX` 一律不继承**〔08-11 事故订正，与 `supervise_with_stdio` 同一条〕：
        //   tmux 客户端在 `TMUX` 有值时按它给的 socket 走，`TMUX_TMPDIR` 完全不起作用。
        //   漏这一条，被起的后端会去改「monitor 恰好从哪个 tmux 里被启动」的那个 server。
        .env_remove("TMUX")
        .env(RESIDENT_ENV, "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null());
    // stderr 仍是 null（理由见下），但交它一份自己的诊断文件路径 ⇒ 它把 fd 2
    //   接进一份有上限、滚动的文件（后端 `stderr_log.rs`），设置「日志」里看得到。**只这条载体交**：被监护那条的
    //   stderr 已经进本进程的滚动日志（`StderrSink::ToLog`），交了反而分成两处。目录由这里建好（后端只新建文件、不建目录）；
    //   拿不到数据目录 / 建不了目录 ⇒ 不交，后端照旧不落盘（不猜路径）。
    if let Some(p) = crate::config::resolve_monitor_data_dir()
        .map(|d| crate::logging::backend_stderr_log_path(&d))
        .filter(|p| {
            p.parent()
                .is_some_and(|d| std::fs::create_dir_all(d).is_ok())
        })
    {
        cmd.env(crate::logging::BACKEND_STDERR_LOG_ENV, p);
    }
    // ★★ 三条策略—— 「三样一起才叫脱离」里的两样现在写在这儿：
    // · `Hidden` —— 脱离起来的后端**绝不该**在用户桌面上留一个黑框（那个框可关，
    //   一关就是 `CTRL_CLOSE_EVENT` ⇒ 常驻当场没了，而它的全部意义就是「常驻」）。
    //   🔴 这一格**先前没人回答过**：这条路上一个 creation flag 都没有。
    // · `Detached` —— 先前那句 `process_group(0)` 就是它：否则终端里 Ctrl-C 的 SIGINT
    //   会打到整个前台进程组，monitor 和这个常驻实例一起走。
    //   ⚠ 它**不改变父子关系** ⇒ 不 `wait` 就留僵尸，收尸仍走 `reap_detached`。
    // · `Null` —— stdio 全 null 是**刻意**的：`Stdio::piped()` 之后宿主一退读端就断，
    //   它在 **153 毫秒**内 broken-pipe 退出（`backend_policy.rs` 头注实测）。
    //   stdin/stdout 那两根在上面一行，stderr 这一根由策略说了算。
    let spawn = managed_spawner(ConsolePolicy::Hidden, Lifetime::Detached, StderrSink::Null);
    // ★★ `K-R28`：与 `supervise_with_stdio` **同一份分类**（住 `local_backend`，不各写一份）。
    //    这条路 exec 的正是 `resolve_backend_bin` 刚拿到的那个文件 —— 而它可能是
    //    `extract_embedded_to` 刚写出来的那一份（`K-R43` 之后经 `resolve_or_extract` 走）
    //    ⇒ 它是本仓两处「写了一个文件、随后 exec 它」的落点之一。
    local_backend::spawn_with_etxtbsy_retry(&mut cmd, &*spawn).map_err(|f| match f {
        // 这一刻恰好撞上了会自己过去的竞态 —— 那句话也是共用的那一份。
        local_backend::SpawnFailure::TransientBusy { tries, last } => {
            local_backend::etxtbsy_gave_up_reason(bin, tries, &last).into()
        }
        // 别的错 ⇒ **今天那句照旧，一个字不改**。
        local_backend::SpawnFailure::Broken(e) => Said::with_raw(
            copy_text(
                "rsLocalBackendHost.spawn.failed",
                &[("bin", &(bin.display()).to_string())],
            ),
            &e,
        ),
    })
}

/// 脱离之后手里剩下的东西。
pub struct DetachedHandle {
    /// 那个后端的 pid。**0 表示不知道**（接管来的、而上一次那个 monitor 没记下来）。
    pid: u32,
    /// **我们起的**那个 `Child`；接管别人起的那个时是 `None`。
    ///
    /// 它同时是收尸的凭据 —— `process_group` 不改父子关系，不 `wait` 就留僵尸。
    /// ⚠ **「是不是我们起的」不另存一个 `bool`**：那样同一个事实就有了两份表示，
    /// 而两份表示会漂（本区最贵的那一族：「一个值装了两件事」的近亲）。
    /// 要问这句话就问 `child.is_some()`。
    child: Option<crate::spawn_managed::ManagedChild>,
    /// 那个二进制的路径。杀它之前拿它核对 `/proc/<pid>/exe`（防 pid 复用误伤）。
    /// 接管来的那个从 pid 文件的第二行读回；读不回就是空的，而空的**不许杀**。
    bin: std::path::PathBuf,
}

/// `K-P1`：**常驻那条路的句柄**。
///
/// ⚠ **锁序**：要同时拿 [`LOCAL_BACKEND`] 时先拿它、再拿本表；两表都只在读写那一下拿。
/// 「查」与「存」之间那个窗口（`start_local_backend` 头注花了一整段治的双起）由 [`LIFECYCLE`] 关：
/// 起 / 停都从头到尾攥着它，本表的写者只在它里面。
pub static DETACHED: std::sync::Mutex<Option<DetachedHandle>> = std::sync::Mutex::new(None);

/// 被监护那条路（非常驻：Windows / `CCM_NO_DETACH`）起来的那一份二进制。
/// 常驻那条的记在 [`DETACHED`] 里（`bin`）；这一格只为被监护那条补上同一个事实。
/// **锁序同 `DETACHED`**（排在 `LOCAL_BACKEND` 之后）；写它的两处都在 [`LIFECYCLE`] 里。
static SUPERVISED_BIN: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

/// **正在跑的那份**本机常驻后端的二进制 —— 给终端窗口导 `CCM_BACKEND_BIN`（`D11`）。
///
/// 常驻那条（起的 / 接管的）⇒ `DETACHED.bin`（接管来的从 pid 文件第二行读回；读不回是空的 ⇒ `None`，不猜）；
/// 被监护那条 ⇒ 起它时解析出的那一份。都不在 ⇒ `None`（窗口里不设，同「本机后端不在」）。
/// 此前窗口那一格自己去找 exe 旁边那份文件，不认自释放之后正在跑的那一份（RT1 F2 / WIN1 报备）。
pub(crate) fn running_backend_bin() -> Option<std::path::PathBuf> {
    let g = LOCAL_BACKEND.lock().unwrap_or_else(|e| e.into_inner());
    {
        let d = DETACHED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(h) = d.as_ref() {
            return (!h.bin.as_os_str().is_empty()).then(|| h.bin.clone());
        }
    }
    if g.is_none() {
        return None;
    }
    SUPERVISED_BIN
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// `K-P1-D1` `重-2`：**上一次「那个口上有东西，但接不上它」的下一步该干什么。**
///
/// # 为什么要有这么一个格子
///
/// 「对不上就**出声**并拒绝」这句话（`§0b-2㈡` / `listen.rs` 诚实边界②）在
/// **手动点「起」**那条路上是兑现的（`backend_control::backend_start` 回 `Err` ⇒ 前端 toast），
/// 而在**自动起**那条路上（`lib.rs` 的 `setup`，用户每天真正走的那条）
/// 回修前只进了 `tracing::info!` —— **不是 `warn`，也没有任何东西到用户眼前。**
///
/// 它有一个具体的触发场景，不是理论：套接字按家目录确定性算（`relay_route_core::listen_socket_for`），
/// 而 `hello_verdict` 拿「我这一版」（`byte_table::my_backend_id`）逐字比 —— **升级 monitor 之后，
/// 上一次脱离留下的那个后端还在听同一个套接字** ⇒ `Stranger` ⇒ `Adopt::Refused`
/// ⇒ **本机后端起不来，而界面上什么都不说。**
/// ⚠ 这一格是**常驻带来的新场景**：翻面之前 backend 153ms 就死了，根本不存在「上一个还在听」。
///
/// # 为什么是一条**记录**，而不是从 `reason` 串里反推
///
/// 反推要拿字符串去认「这是不是一次拒绝」——那正是 `KPY5` 花一整条 DoD 治的那件事
/// （假信号不会报错，它只是一直说是）。⇒ 这里与 [`DETACHED`] 同一个形状：
/// **只在真的走过那条路时才被写下**，写它的唯一入口是 [`note_start_refusal`]。
///
/// ⚠ `StartOutcome` 的形状**不能动**（加一个变体或一个字段，`backend_control.rs`
/// 那个 `match` 当场编不过，而它在本轮写区之外）⇒ 走这条旁路。
static LAST_START_REFUSAL: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// 记下「这一次失败**用户动得了手**，而且下一步是什么」。**写 [`LAST_START_REFUSAL`] 的唯一入口。**
///
/// # 为什么要有这个函数〔`D2` `重-D2-1`，08-27 补〕
///
/// 上一轮只在 `Adopt::Refused` 那一臂写了记录，而 `StartOutcome::Failed` 的构造点
/// **现打 6 处**（`the_user_actionable_start_failures_all_reach_the_user` 把这个分母钉住了）。
/// 其中当时「宿主自己铸钥匙失败」那一支（今天钥匙由常驻后端自己写，那一支随之删了）——
/// 走的是 `None =>` ⇒ `tracing::info!` ⇒ **用户什么都看不到**。
///
/// ★ 而这撞的是 `lib.rs` 自己写下的分档标准〔引的是 **2026-09-10 之前**那一版原话，
/// 逐字保着 —— 这一段说的是 08-27 当时的判断，改写引文就是改写历史〕：
/// 「**拒绝**（口上有东西、接不上）=
/// 一件**用户能动手解决的事** ⇒ 说到眼前；别的失败（安装包里还没有 local_backend…）=
/// 诚实降级 ⇒ 仍走日志」。
/// ⚠ 括号里那个例子今天不成立（09-10 干净 win11 现打，PM：装出来那份跑着 2 个
/// `cc-monitor-backend.exe`、裸 `monitor.exe` 那份 0 个 ⇒ 安装包里带着本机后端），
/// `lib.rs` 那处已订正为「这一份产物里没带本机后端」。**分档标准本身一格没动。**
/// 「盘上有个零字节的 token 文件，删掉它再起一次」按这条标准
/// **属于前者**，而上一轮把它落在了后者。⇒ **`阻-4` 只修了一半**：它让诊断指对了地方，
/// 而「指对了的那句话被谁听见」落在了 `重-2` 的人群外面。
///
/// # 用它的口径（新增失败构造点时照这一条分档）
///
/// - **用户动得了手**（删一个文件、停一个进程、改一个权限位）⇒ 调本函数，
///   并且那句话要**说得出下一步**（判据钉着「下一步」这三个字）；
/// - **诚实降级**（**这一份产物里没带 local_backend** —— 裸 exe / 开发树、
///   或释放内嵌那一份也失败了；或这台机不走脱离那条路）⇒ **不要**调本函数：
///   每次启动都弹一次就成了噪音。那一档仍走 `tracing::info!`。
///   〔订正 2026-09-10（v3.7.0）—— 原话逐字：「**诚实降级**（安装包里还没有本机后端、
///    这台机不走脱离那条路）」。「安装包里还没有」今天不成立：09-10 干净 win11 现打
///    （PM）装出来那份跑着 2 个 `cc-monitor-backend.exe`、裸 `monitor.exe` 那份 0 个。
///    ⚠ **分档口径一格没动**，换掉的只是它举的那个例子。〕
fn note_start_refusal(next_step: String) {
    *LAST_START_REFUSAL.lock().unwrap_or_else(|e| e.into_inner()) = Some(next_step);
}

/// 取走上一次拒绝的「下一步能做什么」。**取走**（不是读）——
/// 同一次拒绝不许被两条路各说一遍，也不许留到下一次启动还在那儿。
pub fn take_start_refusal() -> Option<String> {
    LAST_START_REFUSAL
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
}

/// `KPY5` 的源头：**起它的时候走没走脱离那条路**。
///
/// ⚠ **不许拿 `channel` 或 `pid` 反推** —— 那正是 `P2d §0a` 翻掉的 `SSH_CONNECTION` 那一形
/// （假信号不会报错，它只是**一直说是**，而在只有正例的测试里永远绿）。
/// 这里读的是一条**只在真的走过那条路时才会被写下**的记录。
pub fn is_detached() -> bool {
    DETACHED.lock().map(|g| g.is_some()).unwrap_or(false)
}

/// 这台机今天走不走脱离那条路。**纯函数**，两个入参都是外面喂进来的 ——
/// 否则「关掉了」那一支在 Linux 上永远走不到（`KPY5` 要的负例就没了）。
pub(crate) fn detach_wanted(is_linux: bool, no_detach_env: Option<&str>) -> bool {
    // 空串按「没设」算：shell 里 `export CCM_NO_DETACH=` 是常态。
    is_linux && !no_detach_env.is_some_and(|v| !v.trim().is_empty())
}

/// 收尸。**由「流断了」这个事件触发，不是轮询。**
///
/// `process_group` 不改变父子关系 ⇒ 我们起的那个进程死掉之后会变成 `Z`，
/// 直到有人 `wait` 它。断言必须落在**进程表**上而不是落在我们自己的事件上
/// —— `KPY3` 钉的就是这一格。
/// ⚠⚠ **先把 `Child` 从锁里 `take()` 出来，再在锁外等** —— 这不是讲究，是一次实测死锁。
///
/// 〔`K-P1` e2e 08-26 实测〕第一版是「持着 `DETACHED` 的锁调 `c.wait()`」，
/// 而「流断了」**不等于**「那个进程死了」：backend 现在也会在**它自己**读到 EOF 时主动收掉
/// 这一条流（那是上一格修的东西）。于是 `wait()` 会等一个**还活着**的进程，
/// **一直持着那把锁** ⇒ 下一次 `backend_status` / `backend_start` / `backend_stop` 全部卡死。
/// 实测形状：测试线程 `futex_do_wait`、一个 tokio worker `do_wait`，200 秒不动。
/// ⇒ 与 `supervise_with_stdio` 头注记的是**同一条**（那边逐字写过「guard 活在闭包里
/// ⇒ `wait()` 整段都持着锁，而 `stop()` 第一件事就是取那把锁」）—— 本仓第二次。
///
/// 收尸落在一条**专用线程**上（形状抄 `platform/terminal.rs::launch_local_posix_via` 里那条），
/// 它随子进程结束而结束；`Child` 被取走之后句柄里只剩 pid + 二进制路径，
/// 「停」那一步照样有凭据（一次性 `--resident-stop` 用这个二进制、按它自己记的 pid 核身份）。
fn reap_detached(
    handshake: crate::backend_policy::Handshake,
    reader: crate::backend_policy::ReaderEnd,
) {
    let taken = {
        let mut g = DETACHED.lock().unwrap_or_else(|e| e.into_inner());
        g.as_mut().and_then(|h| h.child.take())
    };
    let Some(mut c) = taken else { return };
    std::thread::Builder::new()
        .name("ccm-detached-reaper".into())
        .spawn(move || {
            // `process_group` 不改变父子关系 ⇒ 不 `wait` 就留僵尸（`launch.rs:198` 逐字）。
            // ★ `K-P3b`：**`wait()` 回来那一拍就是这条路上唯一拿得到退出状态的时刻** ——
            //   在这里记账，不在别处猜。
            match c.wait() {
                Ok(status) => note_detached_death(status, handshake, reader),
                Err(e) => tracing::warn!(
                    "收不了脱离那个后端的尸（{e}）⇒ 这一次死亡的退出状态拿不到，账上不记 —— \
                     ⚠ 如实降级：编一个退出状态比不记更坏"
                ),
            }
        })
        .map(|_| ())
        .unwrap_or_else(|e| tracing::warn!("起不来收尸线程（{e}）⇒ 那个 pid 会留成僵尸"));
}

/// 把一次 `wait()` 的结果翻成 [`crate::backend_policy::Outcome`]。
///
/// ★ **信号号只有这一层取得到**：它要 `std::os::unix::process::ExitStatusExt`，
/// 而 `std::os::unix` 在 `backend_client_guard_tests.rs::the_backend_half_stays_platform_agnostic`
/// 的禁针里 ⇒ `backend/` 那半只能把 `ExitStatus` **原样**交上来
/// （`SuperviseEvent::Exited.status`），翻译落在这里。
///
/// `None` = 既没有退出码也没有信号号。那一格**没有事实可说** ⇒ 调用方出声、不上账；
/// 编一个 `Exited(0)` 顶上去正是 `platform/fallback_guard.rs` 治的那一族。
fn outcome_of_status(status: std::process::ExitStatus) -> Option<crate::backend_policy::Outcome> {
    if let Some(sig) = crate::platform::proc::exit_signal(&status) {
        return Some(crate::backend_policy::Outcome::Signalled(sig));
    }
    status.code().map(crate::backend_policy::Outcome::Exited)
}

/// 脱离那条路上「它跟我们说过话没有」这一维。
///
/// ★ **它是从一个事实推出来的，不是一个孤零零的字面量**：入参就是那道门的产物。
/// [`attach_stream`] 开头两行是 `parse_frame` + `BackendHello::from_hello_frame`，
/// 任一给不出东西就 `Err` 返回、**根本进不到流循环**，也就没有人调得到 [`reap_detached`]。
/// ⇒ **拿得出一份 [`crate::inbound_client::BackendHello`] = 一帧合法 hello 已经到手**，
/// 而那正是「它说过话」的定义。要这个参数是为了让这条推理**在类型上**成立：
/// 没有见证就调不出这个函数（见证的构造入口只有 `from_hello_frame` 一个，
/// 由 `inbound_client` 那条「见证不许凭空造」的判据钉着）。
///
/// ⚠ 前提哪天变了（有人让 `attach_stream` 不带 hello 也能进流循环），
/// 这一句就成了假的 —— 由 [`tests::the_detached_handshake_is_derived_from_the_hello_gate`]
/// 钉住那道门还在。
fn handshake_from_hello(
    _witness: &crate::inbound_client::BackendHello,
) -> crate::backend_policy::Handshake {
    crate::backend_policy::Handshake::Spoke
}

/// 脱离那条路的死亡账。**三维各自的来历逐条写在这儿，一个都不是默认值。**
fn note_detached_death(
    status: std::process::ExitStatus,
    handshake: crate::backend_policy::Handshake,
    reader: crate::backend_policy::ReaderEnd,
) {
    let Some(outcome) = outcome_of_status(status) else {
        tracing::warn!(
            "脱离的后端退出状态里既没有退出码也没有信号号（{status:?}）—— \
             这一格没有事实可说，账上不记"
        );
        return;
    };
    let ev = crate::backend_policy::DeathEvidence {
        // ① `Outcome`：`wait()` 的 `ExitStatus`，信号号在这一层取（见 `outcome_of_status`）。
        outcome,
        // ② `Handshake`：由 `attach_stream` 那道 hello 门推出来（见 `handshake_from_hello`）。
        handshake,
        // ③ `ReaderEnd`：流循环两个出口各自带上来的（EOF / 那句读错误原样）。
        reader,
        // 这条路上进程真的起来过 ⇒ 没有「起不来」那一支的两个字段。
        start_failure: None,
    };
    // 脱离那条路没有监护：它走了就没人再起它 ⇒ 本机连不上，等用户点［重新连接］。
    after_death(
        crate::backend_policy::record_death(
            crate::inbound_client::LOCAL_ORIGIN,
            &ev,
            &mut crate::backend_policy::MonitorLog,
        ),
        crate::ui_error::Then::Down,
    );
}

/// 记完一笔之后：落点拒收就再喊一声（[`shout_if_the_ledger_refused`]），再告诉用户（[`crate::ui_error::tell`]，
/// 只记日志的那几格它自己不推）。
fn after_death(rec: Option<crate::backend_policy::Recorded>, then: crate::ui_error::Then) {
    let Some(r) = rec else { return };
    let tell = crate::ui_error::UiError::LocalBackendDied {
        death: r.death.clone(),
        then,
        ledger_line: r.line.clone(),
    };
    shout_if_the_ledger_refused(Some(r));
    crate::ui_error::tell(tell);
}

/// 记完一笔之后**不许把 [`crate::backend_policy::Recorded`] 丢掉**。
///
/// `#[must_use]` 拦得住 `let _ =` 之外的忘记，拦不住「写了 `let _ =`」。
/// ⇒ 三处接线一律把它交到这里：落点拒收时**再喊一声**。
/// `record_death` 自己已经 `error!` 过一次 —— 这一声证的是**调用方没把它吞了**
/// （`KP3A` 死值验那一刀分开的正是「没写」与「写了但吞了错」）。
fn shout_if_the_ledger_refused(rec: Option<crate::backend_policy::Recorded>) {
    let Some(r) = rec else { return };
    if let Some(why) = &r.sink_error {
        tracing::error!(
            "死亡账写不进去（{why}）—— 调用方这一侧也喊一声，别让它只死在落点里：{}",
            r.line
        );
    }
}

/// 把一条已经认证过的连接接成入方向通道。
///
/// # 它与 `local_backend::local_stdio_consumer` 是同一件事的两种载体
///
/// 那一份吃 `ChildStdin`/`ChildStdout`，这一份吃一条套接字的两半。
/// **复用的是纯零件**（`parse_frame` · `BackendHello::from_hello_frame` ·
/// `park_owned_writer` · `read_capped_line`），没有把一个绑传输的循环硬掰成泛型 ——
/// 那是 `local_stdio_consumer` 头注自己给的分寸。
fn attach_stream(sock: own_chan::BlockingStream, hello_line: &str) -> Result<(), Said> {
    let frame = crate::stream_source::parse_frame(hello_line)
        .map_err(|_| Said::from(copy_text("rsLocalBackendHost.stream.badFirstLine", &[])))?;
    let witness = crate::inbound_client::BackendHello::from_hello_frame(&frame)
        .ok_or_else(|| Said::from(copy_text("rsLocalBackendHost.stream.notHello", &[])))?;
    // ★ `K-P3b`：**「它说过话没有」这一维就在这一行变真的** —— 上面那道门给出见证的那一刻。
    //   下面把它一路带到收尸那一拍，而不是在那边写一个 `Handshake::Spoke` 字面量。
    let handshake = handshake_from_hello(&witness);
    // 期限只属于**握手**那一段；进了流之后这条连接是长连接，装着期限反而会把它掐断（`into_async` 摘掉）。
    let (rd, wr) =
        tauri::async_runtime::block_on(async move { sock.into_async().map(tokio::io::split) })
            .map_err(|e| {
                Said::with_raw(copy_text("rsLocalBackendHost.stream.asyncFailed", &[]), &e)
            })?;
    let client = crate::inbound_client::park_owned_writer(wr).into_client(witness);
    crate::inbound_client::register(crate::inbound_client::LOCAL_ORIGIN, client.clone());
    tracing::info!(
        "本机入方向通道已登记（常驻载体）：origin={}",
        crate::inbound_client::LOCAL_ORIGIN
    );
    tauri::async_runtime::spawn(async move {
        use crate::stream_source::{CappedLine, BACKEND_FRAME_LINE_CAP};
        let mut reader = tokio::io::BufReader::new(rd);
        let mut buf: Vec<u8> = Vec::new();
        // ★ `K-P3b`：**我们这一侧的读端怎么结束的**。初值只在真读到 EOF 时才成立 ——
        //   下面那条 `Err` 支会把它换掉，两个出口各写各的。
        let mut reader_end = crate::backend_policy::ReaderEnd::CleanEof;
        // 这条载体上丢了几行 / 几帧 —— 原先两处裸 `continue` 一声不吭；记账，流结束出总账。
        let mut tally = crate::frame_tally::FrameTally::new("本机常驻后端（脱离载体）");
        // 解不出来的帧每种说一次（日志 ＋ 本机的健康信息）。
        let mut unread = crate::stream_source::UnreadNotes::new(
            crate::inbound_client::LOCAL_ORIGIN,
            copy_text("rsLocalBackendHost.local.machine", &[]),
        );
        loop {
            match crate::stream_source::read_capped_line(
                &mut reader,
                &mut buf,
                BACKEND_FRAME_LINE_CAP,
            )
            .await
            {
                Ok(CappedLine::Eof) => break,
                Ok(CappedLine::TooLong(bytes)) => {
                    // 丢弃 + 原位说出来，绝不静默（定框 E4；与远端同形：订阅收一格 `Gap`）。
                    tracing::warn!("本机常驻后端发来一行 {bytes} 字节，超过单行上限；整行丢弃");
                    crate::local_lines::line_lost().await;
                    continue;
                }
                Ok(CappedLine::Line) => {}
                Err(e) => {
                    tracing::warn!("本机常驻后端读错误（{e}）；按流结束处理");
                    // ★ `K-P3b`：那句错**原样**带到账上 —— 「读坏了」说的是我们这一侧，
                    //   而它**不算它崩了一次**（`B1` 那条错误诊断的全部内容）。
                    reader_end = crate::backend_policy::ReaderEnd::Broken(e.to_string());
                    break;
                }
            }
            let Ok(line) = std::str::from_utf8(&buf) else {
                if let Some(n) = tally.note_bad_utf8(&buf) {
                    tracing::warn!("{n}");
                }
                continue;
            };
            let Some(f) = unread.take(line, &mut tally, &crate::stream_source::local_health) else {
                continue;
            };
            // 本机的 tmux 帧（`P3` 刀 1）·应答 · 链路帧 —— 与 stdio 那条载体**同一个吸收点**，
            // 理由与前置条件写在 `local_backend::absorb_local_frame` 的头注上，这里不再抄一份散文。
            // 交回来的内容帧送进本机内容通道 —— 这是 tokio 任务 ⇒ `.await` 那一形（满了就停读：级 1 回推）。
            if let Some(f) = crate::local_backend::absorb_local_frame(
                f,
                Some(&client),
                &crate::origin::Origin::local(),
            ) {
                crate::local_lines::deliver(f).await;
            }
        }
        // 告诉本机内容消费者这条流结束了。
        crate::local_lines::stream_ended().await;
        // 流没了 ⇒ 经它开的在飞链路全部带原因结束（不让调用方干等到超时）。
        crate::link_mux::fail_owned_by(&client, &copy_text("rsLocalBackendHost.stream.lost", &[]));
        // 经它开的传输也一律收场（后端的票表随那条流一起撤了）。
        crate::sftp_pool::fail_owned_by(&client, &copy_text("rsLocalBackendHost.stream.lost", &[]));
        // 流结束 ⇒ 摘掉登记，别在表里留一个写不进去的 client。monitor 不再存 tmux 原文（没有要清的陈旧证据了）。
        crate::inbound_client::unregister(crate::inbound_client::LOCAL_ORIGIN, &client);
        // 收尸：`process_group` 不改父子关系，不 `wait` 就留 `Z`。**事件驱动，不是轮询。**
        // ★ `K-P3b`：两维证据一起交下去 —— 收尸那一拍才拿得到第三维（退出状态）。
        reap_detached(handshake, reader_end);
        tracing::info!("本机常驻后端的流结束（EOF）");
    });
    Ok(())
}

/// 走常驻那条路的结局。
enum DetachOutcome {
    /// 这条路今天不走（平台不支持 / 被 `CCM_NO_DETACH` 关掉）⇒ 回落今天那条。
    NotTaken,
    /// 走了，结局在里面（含失败 —— 失败也不回落，见下）。
    Done(StartOutcome),
}

/// 常驻那条路的正题：**认得出已有实例 ⇒ 接上它；没有 ⇒ 起一个脱离的。**
///
/// 两个入参都是**注入**，理由各不相同：
/// - `resolve_bin`：生产路径喂 [`resolve_backend_bin`]（exe 旁 → 内嵌释放）；
///   而判据喂一个现成的二进制 —— 否则那条真进程判据只能去动用户真实的 `~/.cc-monitor`。
/// - `extra_env`：生产路径喂**空表**；e2e 用它塞一条**前面挂着 shim 的 PATH**。
///   ⚠ 这一条不是方便，是 `C7i` 红线：被起的后端一上来就往它连得到的 tmux server
///   装三条全局 hook、固定槽位 `[50]`、**没有关掉它的开关**。不隔离就是去改用户真实
///   tmux server 的状态（08-11 那次事故打没了用户 9 个真实会话）。
fn start_detached(
    resolve_bin: &dyn Fn() -> Result<std::path::PathBuf, (Said, Vec<std::path::PathBuf>)>,
    extra_env: &[(String, String)],
) -> DetachOutcome {
    if !detach_wanted(
        crate::platform::proc::CAN_DETACH,
        std::env::var(NO_DETACH_ENV).ok().as_deref(),
    ) {
        return DetachOutcome::NotTaken;
    }
    // 门牌只跟着家走（`~/.cc-monitor`，隔离跑时 `CCM_DATA_DIR`）：与 Claude 目录、与哪一家 agent 都无关。
    let Some(home) = crate::config::resolve_monitor_data_dir() else {
        return DetachOutcome::NotTaken;
    };
    // 套接字与「谁在听」都住这个家（后端 `--resident-stop` 在同一个家里找同一份）。
    let dir = home;
    let sock = relay_route_core::listen_socket_for(&dir);

    // ── ① 起时先认已有实例 ────────────────────────────────────────────
    //
    // ★★ 这一条是硬的：backend 一起来就**无条件**往它连得到的 tmux server 装三条全局 hook、
    //    **固定槽位 `[50]`**、**没有关掉它的开关**，载荷里烤着那一个后端的 pid+starttime。
    //    ⇒ **脱离而不认已有实例 = 每台机 N 个后端互相盖槽位，比今天更糟。**
    match adopt_existing(&sock, extra_env) {
        Adopt::Attached => {
            let (pid, bin) = read_listen_owner(&dir).unwrap_or((0, std::path::PathBuf::new()));
            let mut g = DETACHED.lock().unwrap_or_else(|e| e.into_inner());
            *g = Some(DetachedHandle {
                pid,
                child: None,
                bin,
            });
            return DetachOutcome::Done(StartOutcome::AlreadyRunning);
        }
        // ⚠ **不静默复用，也不静默再起一个** —— 两条都会让状态更糟。
        Adopt::Refused(why) => {
            // ★★ `重-2`：**这一臂是「出声并拒绝」里「出声」那一半的源头。**
            //    自动起那条路（`lib.rs`）拿它决定要不要把话说到用户眼前 ——
            //    而不是去 `reason` 串里认字（那是 `KPY5` 治的那种假信号）。
            note_start_refusal(copy_text(
                "rsLocalBackendHost.start.refusedNotice",
                &[
                    ("why", &why.said),
                    ("pidPath", &(pid_path(&dir).display()).to_string()),
                ],
            ));
            return DetachOutcome::Done(StartOutcome::Failed {
                reason: Said::restate(
                    copy_text(
                        "rsLocalBackendHost.start.refused",
                        &[("path", &sock.display().to_string()), ("why", &why.said)],
                    ),
                    why,
                ),
                looked_at: vec![pid_path(&dir), sock.clone()],
            });
        }
        // 〔HOST 余项〕常驻后端多客户之后不再有「被另一个 monitor 占着」那一臂（连同两句话删了）。
        Adopt::None => {}
    }

    // ── ② 没人在听 ⇒ 起一个脱离的 ─────────────────────────────────────
    let bin = match resolve_bin() {
        Ok(b) => b,
        Err((reason, looked_at)) => {
            return DetachOutcome::Done(StartOutcome::Failed { reason, looked_at })
        }
    };
    let child = match spawn_detached(&bin, extra_env) {
        Ok(c) => c,
        Err(e) => {
            return DetachOutcome::Done(StartOutcome::Failed {
                reason: e,
                looked_at: vec![bin],
            })
        }
    };
    let pid = child.id();
    // 「谁在听」由它自己绑上口之后记（后端 `control/resident.rs::record_owner`，本机远端同一个写者）。
    {
        let mut g = DETACHED.lock().unwrap_or_else(|e| e.into_inner());
        *g = Some(DetachedHandle {
            pid,
            child: Some(child),
            bin: bin.clone(),
        });
    }
    // 起来了之后自己连上去 —— **走与「接管」完全同一条路**，不另写一份。
    match probe_and_attach_after_spawn(&sock, extra_env) {
        Ok(()) => DetachOutcome::Done(StartOutcome::Started(bin)),
        Err(e) => {
            // 起来了但连不上 ⇒ 这不是「起了」。把它收掉，别留一个谁都够不着的进程。
            stop_detached();
            DetachOutcome::Done(StartOutcome::Failed {
                reason: Said::restate(
                    copy_text(
                        "rsLocalBackendHost.start.unreachableKilled",
                        &[("why", &e.said)],
                    ),
                    e,
                ),
                looked_at: vec![bin],
            })
        }
    }
}

/// 等刚起的那个后端把套接字绑上 —— **次数与间隔都有上限**。
///
/// `spawn()` 返回时子进程可能还没跑到绑那一步；没人在听的套接字上连一下是**立刻**失败的，不等。
///
/// # 它是 `wait-for-condition`，不是节拍器
///
/// 等的是**一次性条件**（那个套接字起没起来），有明确上限（`LISTEN_WAIT_TRIES` ×
/// `LISTEN_WAIT_INTERVAL_MS` ≈ 1 秒），等到就走、等不到就如实报错，**不无限重试**。
/// **登记住址** `tests/frontend/shell/rust_timer_registry.rs`（那张表按类别收，`wait-for-condition`
/// 这一类要求「说清等什么、上限是多少」）。
///
/// ⚠ 上限为什么是这个量级：对端**就在本机**，从 `execve` 到绑上是毫秒级；
/// 1 秒高两个量级。给得再大只会让「那个进程起来就崩了」这一格拖着不报错。
const LISTEN_WAIT_TRIES: u32 = 50;
const LISTEN_WAIT_INTERVAL_MS: u64 = 20;

/// 接管已有实例的结果。
enum Adopt {
    /// 没人在听 —— 该起一个了。
    None,
    /// 接上了。
    Attached,
    /// 有人在听，但接不上。**出声**，绝不静默复用、也绝不换个地方再起一个。
    Refused(Said),
}

/// 认已有实例并接上它。只有一支会等：**还没人在听**、而我们刚亲手 spawn 完（`wait_for_bind` = true）。
/// 接管路上遇到它就是「真没人」，立刻回 [`Adopt::None`]。别的一律不重试 —— 对不上、接不上，重试只是把一个确定的坏消息拖晚。
fn adopt_existing(sock: &std::path::Path, env: &[(String, String)]) -> Adopt {
    adopt_with(sock, env, false)
}

/// 刚起完之后连上去。**与接管走同一条路**，差别只有一句：
/// 这一次「没人在听」是**还没绑完**（我们刚亲手起了一个），要等。
fn probe_and_attach_after_spawn(
    sock: &std::path::Path,
    env: &[(String, String)],
) -> Result<(), Said> {
    match adopt_with(sock, env, true) {
        Adopt::Attached => Ok(()),
        Adopt::Refused(why) => Err(why),
        Adopt::None => Err(copy_text(
            "rsLocalBackendHost.afterSpawn.neverListened",
            &[("path", &sock.display().to_string())],
        )
        .into()),
    }
}

/// `env` = 这一趟交给（或会交给）后端的那份环境 —— 身份比对的第三项（[`hello_verdict`]）。
fn adopt_with(sock: &std::path::Path, env: &[(String, String)], wait_for_bind: bool) -> Adopt {
    let mut last: Said = copy_text("rsLocalBackendHost.adopt.nobody", &[]).into();
    for _ in 0..LISTEN_WAIT_TRIES {
        match probe_listen_socket(sock, env) {
            Probe::Stranger(why) => return Adopt::Refused(why),
            Probe::Nobody => {
                if !wait_for_bind {
                    return Adopt::None;
                }
                last = copy_text(
                    "rsLocalBackendHost.adopt.notYet",
                    &[("path", &sock.display().to_string())],
                )
                .into();
            }
            Probe::Ours(conn, hello) => {
                return match send_attach(&conn).and_then(|()| attach_stream(conn, &hello)) {
                    Ok(()) => Adopt::Attached,
                    Err(e) => Adopt::Refused(e),
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(LISTEN_WAIT_INTERVAL_MS));
    }
    Adopt::Refused(Said::restate(
        copy_text(
            "rsLocalBackendHost.adopt.waited",
            &[
                ("last", &last.said),
                ("tries", &LISTEN_WAIT_TRIES.to_string()),
                ("interval", &LISTEN_WAIT_INTERVAL_MS.to_string()),
            ],
        ),
        last,
    ))
}

/// 找那个二进制 —— **这一层只做适配，答案取自那一份共用的解析**
/// （`local_backend::resolve_or_extract`：先找 exe 旁边、再问产物自己带没带、再释放）。
///
/// # 🔴 `K-R43`：本函数**曾经是第二份手写实现**，今天不是了
///
/// 原来这里自己走一遍「找旁边 → 释放内嵌」，与 `local_backend::start_or_extract` 并列两份，
/// 中间只有 `the_two_resolution_paths_still_agree_on_the_order` **对拍顺序**。
/// ⚠ **那条判据眼皮底下真的漂过一次，而它全程绿**：`K-R42` 只给那一份接上了
/// 「问产物自己带没带」与「释放失败说一句分得开的话」，本函数一个字没动 ——
/// 顺序没变 ⇒ 判据没红，而同一台机上两条路对同一个失败给出了两句性质不同的话
/// （本函数那一句逐字是 `K-R42` 从对面删掉的那一句）。读数住件文件。
/// ⇒ 处置是**只留一份**，本函数降为适配器：把 `Resolved` 换成这条路要的 `Result`，
/// 并补上两样**宿主知识**（目标三元组常量 · `make_executable`）。
///
/// ⚠ **本层不许再自己拼失败串** —— 拼了就是第二份说法。由
/// `the_two_resolution_paths_still_agree_on_the_order` 钉着（它今天钉的是「两条路都走那一份」）。
fn resolve_backend_bin(
    extract_dir: &std::path::Path,
    embedded: Result<(&str, &[u8]), String>,
) -> Result<std::path::PathBuf, (Said, Vec<std::path::PathBuf>)> {
    match local_backend::resolve_or_extract(
        env!("CCM_TARGET_TRIPLE"),
        extract_dir,
        embedded,
        // `backend-split` 的 C10：平台知识由宿主注入。
        &crate::platform::fs::make_executable,
        &crate::platform::fs::ensure_private_dir,
        // 放不放问手上那份字节自己（起进程的三条策略由宿主给）。
        &crate::ccm_probe::ask_place_verdict,
    ) {
        Resolved::Found(p) => Ok(p),
        Resolved::Missing { reason, looked_at } => Err((reason, looked_at)),
    }
}

/// 停掉常驻那个。**调用方攥着 [`LIFECYCLE`]**（起停串行）。
/// `None` = 没有常驻那个；`Some(Ok)` = 结局（`graceful` / `killed` / `not_running`）；`Some(Err)` = 没停掉（句柄放回去，它还算在跑）。
///
/// **本机远端同一条**：在这台机器上起一次 `<后端> --resident-stop`（同机监督者：请它收尾 → 宽限期内等 → 到点强杀，
/// 住后端 `control/resident.rs::stop_pid`），这里只拿回结局。monitor 这一侧不再自己发信号、自己等、自己强杀（HX1 那套 `stop_grace` 删了）。
/// 句柄**先取出来、出锁再等**：那一趟可能几十秒，句柄表的锁不跨这段等（查状态不被挡住）；停不掉再放回去。
/// 自己起的那个随后在这里 `wait` 收尸（`process_group` 不改父子关系，`INVARIANTS §48.2`）。
fn stop_detached() -> Option<Result<crate::remote_resident::StopAnswer, Said>> {
    let mut h = DETACHED.lock().unwrap_or_else(|e| e.into_inner()).take()?;
    let answer = run_resident_stop(&h.bin);
    let put_back = |h: DetachedHandle| {
        *DETACHED.lock().unwrap_or_else(|e| e.into_inner()) = Some(h);
    };
    if answer.is_err() {
        put_back(h);
        return Some(answer);
    }
    if let Some(mut c) = h.child.take() {
        match c.try_wait() {
            // 退了（结局是 graceful / killed）⇒ 收尸，不留 `Z`。
            Ok(Some(_)) => {}
            Ok(None) if answer.as_ref().is_ok_and(|a| a.pid.is_some()) => {
                // 结局说它退了，而内核还没把退出交到我们这个父进程 —— 一瞬间的事：阻塞收尸（它已经不跑任何东西了）。
                if let Err(e) = c.wait() {
                    tracing::warn!(
                        "收不了自己起的那个后端的尸（{e}）⇒ 它会留成僵尸，直到 monitor 退出"
                    );
                }
            }
            // 结局说「没在跑」、我们起的那个却还活着（pid 记录丢了 / 对不上）⇒ 不是「停了」：句柄放回去，出声。
            Ok(None) => {
                h.child = Some(c);
                let pid = h.pid;
                put_back(h);
                return Some(Err(copy_text(
                    "rsLocalBackendHost.stop.noRecord",
                    &[("pid", &pid.to_string())],
                )
                .into()));
            }
            Err(e) => tracing::warn!("收不了自己起的那个后端的尸（{e}）"),
        }
    }
    Some(answer)
}

/// 停那一趟等多久：比后端那一趟自己的上限（宽限 35 秒 ＋ 强杀之后再等 5 秒，`control/resident.rs`）长出起进程的余量。
/// 到点 ⇒ 结束那一趟、如实说没答完（后端可能还在），不无限等。
pub(crate) const RESIDENT_STOP_DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);

/// 起一次 `<后端> --resident-stop`（不经 shell）、读它那一行结局（与远端同一个读法 `remote_resident::read_stop`）。
/// 交的环境与起它时同一份（[`backend_env`]）⇒ 它在同一个家里按同一个函数算出同一个口、找到同一份 `listen-<口>.pid`。
fn run_resident_stop(bin: &std::path::Path) -> Result<crate::remote_resident::StopAnswer, Said> {
    run_resident_stop_within(bin, RESIDENT_STOP_DEADLINE)
}

/// [`run_resident_stop`] 的本体（期限是入参：判据拿一个不答话的替身与一个短期限量「到点就回来」）。
fn run_resident_stop_within(
    bin: &std::path::Path,
    deadline: std::time::Duration,
) -> Result<crate::remote_resident::StopAnswer, Said> {
    use crate::spawn_managed::{spawn_managed_cmd, ConsolePolicy, Lifetime, StderrSink};
    if bin.as_os_str().is_empty() {
        return Err(copy_text("rsLocalBackendHost.stop.noBin", &[]).into());
    }
    let handed = backend_env();
    // 三条策略：`Hidden`（一次性子命令不该闪窗）· `JobKillOnClose`（就地等它退，别留后代）·
    // `Captured`（失败那一句 `{code,message}` 在 stderr 上，要读回来说给人听）。
    // 等法与本机探针同一段（`ccm_probe::capture_full`：读线程 ＋ 带上限地等，到点杀掉收尸）。
    let got = crate::ccm_probe::capture_full(
        deadline,
        &|| {
            let mut cmd = std::process::Command::new(bin);
            // 本机落点就是 `ccm`：打头的 `--` 让它当后端用（没有它整行交给 claude）。
            cmd.args([local_backend::BACKEND_SEP, "--resident-stop"])
                .env_remove("TMUX")
                .envs(handed.iter().cloned())
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped());
            spawn_managed_cmd(
                &mut cmd,
                ConsolePolicy::Hidden,
                Lifetime::JobKillOnClose,
                StderrSink::Captured,
            )
        },
        None,
    );
    let out = match got {
        Ok(c) => c,
        Err(crate::ccm_probe::CaptureFail::Spawn(e)) => {
            return Err(Said::with_raw(
                copy_text(
                    "rsLocalBackendHost.stop.spawnFailed",
                    &[("bin", &bin.display().to_string())],
                ),
                &e,
            ))
        }
        Err(crate::ccm_probe::CaptureFail::TimedOut) => {
            return Err(copy_text(
                "rsLocalBackendHost.stop.timedOut",
                &[("dur", &copy_core::format_elapsed(deadline))],
            )
            .into())
        }
        Err(crate::ccm_probe::CaptureFail::Read(e)) => {
            return Err(Said::with_raw(
                copy_text("rsLocalBackendHost.stop.readFailed", &[]),
                &e,
            ))
        }
    };
    crate::remote_resident::read_stop(
        &crate::stream_source::RemoteExec {
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            exit_status: out.code.and_then(|c| u32::try_from(c).ok()),
        },
        &copy_core::local_machine(),
    )
    .map_err(Said::from)
}

/// backend 那条监护路的 `on_event` —— ★ **判与记都在这个闭包里**。
///
/// # 为什么不落进 `supervise_with_stdio` 体内
///
/// 死亡账是「**这台机的 backend**」的账，监护器本身是通用的（收 `bin` / `args` / `envs`）⇒ 落进监护器体内，
/// 它就得多一个「你在监护谁」的概念。⇒ 接线落在**客户这一侧**。
/// 这条理由先前还有一半是「监护器有两种客户（后端与中转）」—— 中转并进常驻后端之后，
/// 生产上的客户只剩这一条；理由的另一半（监护器不认识客户）照旧成立。
/// 由 `backend_policy::tests::the_supervisor_itself_never_records_a_death` 钉着。
///
/// # 为什么是一个**返回闭包的函数**，而不是内联在下面那个调用里
///
/// 内联的闭包测试够不着 ⇒ `KP3W3` 那条「三只假后端各判成哪一格」就只能读源码。
/// 抽出来之后，判据可以拿**同一个闭包**跑一遍真进程
/// （[`tests::three_fake_backends_land_in_three_different_cells`]）。
fn backend_supervise_events() -> std::sync::Arc<dyn Fn(local_backend::SuperviseEvent) + Send + Sync>
{
    std::sync::Arc::new(|e| {
        let (code, attempt, status, witness, next) = match e {
            local_backend::SuperviseEvent::Exited {
                code,
                attempt,
                status,
                witness,
                next,
            } => (code, attempt, status, witness, next),
            other => {
                tracing::info!("本机后端: {other:?}");
                return;
            }
        };
        tracing::info!("本机后端: 第 {attempt} 次那一命结束（code={code:?}，之后 {next:?}）");
        // 叫停的（停 / 重启 / 更新本机那一下）：不是死亡，不上账、不告诉用户。
        let then = match next {
            local_backend::AfterExit::Stopped => return,
            local_backend::AfterExit::Restart => crate::ui_error::Then::Restarted,
            local_backend::AfterExit::GiveUp => crate::ui_error::Then::Down,
        };
        // ① 两维证据：只认**观测到的**那一档。
        let (handshake, reader) = match witness {
            local_backend::StreamWitness::Observed { handshake, reader } => (handshake, reader),
            unwitnessed => {
                // 到这里说明这一命没有消费者、或者消费者自己 panic 了 ——
                // 两种情况下「它说过话没有」都**没有被观测过**，而那正是 2026-07-09
                // 判别式的第二个条件。⇒ **出声，不上账**：编一个 handshake 顶上去，
                // 判出来的「被拒了 / 崩了」是假的。
                //
                // ⚠ 如实登记的降级：这一命的死亡**不会出现在账上**。
                // backend 这条路今天恒有消费者（`start_or_extract` 无条件传
                // `Some(Arc::new(local_stdio_consumer_guarded))`，那条线由
                // `the_production_entry_hands_the_stdio_consumer_down` 钉着）
                // ⇒ 生产上只有「消费者 panic」那一形到得了这里，而那一形兜底外壳已经
                // LOUD 记过一条。真常来 ⇒ 回来重判，别在这里补一个默认值。
                tracing::error!(
                    "本机后端这一命结束了，而那两维证据没有观测者（{unwitnessed:?}）—— \
                     死亡账这一笔不记：拿一个没人观测过的「说过话没有」去判，\
                     判出来的是编的（2026-07-09 那次死循环的判别式就是它）"
                );
                return;
            }
        };
        // ② 退出状态：`code` 在被信号打死时是 `None`，信号号在 `status` 里，
        //    而翻译它要 `ExitStatusExt`（宿主层的事，见 `outcome_of_status`）。
        let Some(status) = status else {
            tracing::error!("本机后端这一命结束了，而收尸没拿到退出状态 —— 账上不记");
            return;
        };
        let Some(outcome) = outcome_of_status(status) else {
            tracing::error!(
                "本机后端的退出状态里既没有退出码也没有信号号（{status:?}）—— 账上不记"
            );
            return;
        };
        let ev = crate::backend_policy::DeathEvidence {
            outcome,
            handshake,
            reader,
            // 这条路上进程真的起来过 ⇒ 没有「起不来」那一支的两个字段。
            start_failure: None,
        };
        after_death(
            crate::backend_policy::record_death(
                crate::inbound_client::LOCAL_ORIGIN,
                &ev,
                &mut crate::backend_policy::MonitorLog,
            ),
            then,
        );
    })
}

/// 「从来没起来」那条臂的**唯一生产喂点**：[`start_local_backend`] 的两个出口都从这里过。
///
/// ⚠ `reason` / `looked_at` **原样转**，不另写一份 —— 本文件那一族逐字的纪律：
/// 「两份措辞迟早对不上」。账上那一行里的原因与 `StartOutcome::Failed` 手上的那一句
/// **逐字相同**，由 `the_never_started_reason_is_the_same_string_the_caller_gets` 钉着。
fn note_never_started(out: StartOutcome) -> StartOutcome {
    if let Some((reason, looked_at)) = out.failure() {
        let ev = crate::backend_policy::DeathEvidence {
            outcome: crate::backend_policy::Outcome::NeverSpawned,
            // 进程一次都没存在过 ⇒ 它当然一个字节都没说过。这不是猜，是那条路的定义。
            handshake: crate::backend_policy::Handshake::NeverSpoke,
            // ★ **这条路上根本没有读端** ⇒ 明写「没观测到」，不冒充一次干净 EOF。
            reader: crate::backend_policy::ReaderEnd::NotObserved,
            start_failure: Some((reason.to_string(), looked_at.to_vec())),
        };
        shout_if_the_ledger_refused(crate::backend_policy::record_death(
            crate::inbound_client::LOCAL_ORIGIN,
            &ev,
            &mut crate::backend_policy::MonitorLog,
        ));
    }
    out
}

pub fn start_local_backend() -> StartOutcome {
    // ★★ **锁全程持有**〔D 阶段补审 08-11 修，原版是阻塞级缺陷〕。
    //
    // # 原来错在哪
    //
    // 原判据是 `h.current_pid().is_some()`，而 `pid` 由 **supervise 线程**在 `cmd.spawn()`
    // 成功之后才 `store` —— 本函数返回时那个线程往往还没跑到那一行，`pid` 仍是 0。
    // ⇒ **串行点两下「起」就够**：第二次进来 `current_pid()` 是 `None`，门放行，
    // 起出 h2 并**覆盖** h1。h1 的句柄被 drop，但 `stopping`/`child` 都是 `Arc`
    // ⇒ 它的 supervise 线程照常活、子进程照常跑、崩了照常重起，**再没有任何代码能 `stop()` 它**。
    // 之后按「停」只 take 到 h2 ⇒ `backend_status` 回 `channel: true` / `pid: null`，
    // **「状态说停了」与「进程真没了」当场分叉** —— 正是 `P2s-Y2` 自陈要守的那件事，
    // 而 Y2 的实测走的是被绕开的另一条路径（它自己 spawn，不经本函数）。
    //
    // # 现在的不变量
    //
    // **句柄在表里 = 在跑**（`stop_local_backend` 会把它 `take` 走）⇒ 判据不再问 pid。
    // 并且**起停串行锁（[`LIFECYCLE`]）横跨整个起的过程**，把「检查」与「存句柄」之间那个窗口关掉；
    // 句柄表本身只在读写那一下拿 ⇒ 这一趟里释放 ~10MB、等绑口、握手的那几秒，查状态与退出那一臂不被挡住
    // （从前锁的就是句柄表，`backend_status` 跟着等）。
    let _life = LIFECYCLE.lock().unwrap_or_else(|e| e.into_inner());
    let g = LOCAL_BACKEND.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_some() {
        return StartOutcome::AlreadyRunning;
    }
    // 句柄表只拿这一下：起的那几秒里不攥着它（查状态与退出那一臂要读它）；「查」与「存」之间由上面那把串行锁关窗。
    drop(g);
    // ★ `K-P1`：常驻那条路也算「在跑」。
    //
    // ⚠ 写 `DETACHED` 的只有起 / 停，都在 [`LIFECYCLE`] 里 ⇒ 这里**没有**第二个窗口。
    // 上面那一段头注花了一整段讲的就是「检查」与「存句柄」之间那个窗口，
    // 常驻这条路不许把它重新开出来。
    if is_detached() {
        return StartOutcome::AlreadyRunning;
    }
    // 两样宿主知识在这里给（backend 层不认识它们）：
    //   · 落点 `~/.cc-monitor/bin`：与远端自部署同一个目录，但**文件名带 build_id**
    //     ⇒ 与远端那份结构上不可能撞（理由见 `extract_embedded_to` 头注的 D1 段）。
    //   · 这台机器的 (OS, arch)：`byte_table::choose` 按它挑内嵌字节（下面那一段）；取不到时交进去的是那句拒绝的话，
    //     函数把它接在「旁边没有」后面，不伪造理由。
    let extract_dir = creds_core::store::home_dir()
        .map(|h| h.join(".cc-monitor").join("bin"))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/.cc-monitor/bin"));
    // 〔RK1 小尾巴〕本机上第一个建 `~/.cc-monitor` 的就是这里（释放后端二进制之前）⇒ 先把这一层按「只给本人」建好；
    //   `bin/` 那一层由释放那一步照旧建。建不了不挡起后端（释放那一步会出声说它自己的失败）。
    if let Some(home_dir) = extract_dir.parent() {
        if let Err(e) = crate::platform::fs::ensure_private_dir(home_dir) {
            tracing::warn!("{}（{}）", e.said, e.raw());
        }
    }
    // **本机的字节也按 (OS, arch) 从那一张表里取**（规矩 4：本机只是「目标机器恰好是自己」）。
    //
    // 〔墓碑 —— 这里原来是一道 `cfg!(target_os = "linux")` 的闸（D 阶段补审 08-11）：远端那两份 musl 只按 arch 取、不认 OS，
    //  于是在 Windows 构建上会释放一个 Linux ELF、再报「已起」；那道闸挡住了它，代价是非 Linux 本机一份字节都拿不到，
    //  由 `local_backend` 那一层再问一次「这份产物自己带没带」补上。：「这道闸要消失 —— 它是
    //  『表没有 OS 轴』逼出来的补丁」。〕今天 `byte_table::choose` 按这台机器的键查表：Windows 那一格就是这一份产物
    //  按 `TARGET` 内嵌的那份，Linux 那一格是 musl（开发树只有原生那份时给原生那份），不承诺 / 没带 ⇒ 那句拒绝的话。
    let this_machine = crate::byte_table::Key::this_machine();
    let embedded: Result<(&str, &[u8]), String> = crate::byte_table::choose(this_machine)
        .map_err(|r| r.say(&copy_text("rsLocalBackendHost.local.machine", &[])))
        .and_then(|p| {
            p.build_id
                .map(|id| (id, p.bytes))
                .ok_or_else(|| copy_text("rsLocalBackendHost.local.noStamp", &[]))
        });
    // ★★ `K-P1`：**先走常驻那条路** —— 认得出已有实例就接上它，没有就起一个脱离的。
    //
    // 这就是「怎么起」那个注入点：`start_detached` 是**这一层**（宿主知识层）的东西，
    // 它认识 `process_group(0)` / `/proc` / `~/.cc-monitor`，而 `backend/` 那半一样都不许认识
    // （`the_backend_half_stays_platform_agnostic` 的禁针含 `std::os::unix`）。
    // 走不了（平台不支持 / `CCM_NO_DETACH` 关掉了）才回落到下面那条今天的路。
    // **中转不再是 monitor 另起的第三个进程**：它住本机常驻后端进程里，
    //   由后端自己 `bind`（`relay::listen::host`）。monitor 这一侧只剩一件事 —— 起后端时把
    //   那份环境交给它（[`backend_env`]），**两条载体同一份**（常驻 / 被监护的 stdio）。
    let handed = backend_env();
    // `embedded` 里可能是那句拒绝的话（`String`），不再是 `Copy` ⇒ 常驻那条拿一份拷贝。
    let resolve = || resolve_backend_bin(&extract_dir, embedded.clone());
    match start_detached(&resolve, &handed) {
        // ★ `K-P3b`：这是本函数**两个**返回 `Failed` 的出口之一 —— 都从 `note_never_started` 过。
        DetachOutcome::Done(out) => return note_never_started(out),
        DetachOutcome::NotTaken => {}
    }
    let (resolved, sup) = local_backend::start_or_extract(
        env!("CCM_TARGET_TRIPLE"),
        &extract_dir,
        embedded,
        // `backend-split` 的 C10：平台知识由宿主注入，backend 那半不认识 `#[cfg(unix)]`。
        &crate::platform::fs::make_executable,
        &crate::platform::fs::ensure_private_dir,
        // ★ `K-P3b`：backend 这条监护路的死亡账**就记在这个闭包里**（见它的头注）。
        backend_supervise_events(),
        // ★：起进程那一下的三条答案由**宿主**给（backend 那半不认识平台）。
        crate::spawn_managed::local_backend_supervised(),
        // 与常驻那条路交的是**同一份**（上面那个 `handed`）。
        handed,
        // 同上：放不放问手上那份字节自己。
        &crate::ccm_probe::ask_place_verdict,
    );
    if let Some(h) = sup {
        let mut g = LOCAL_BACKEND.lock().unwrap_or_else(|e| e.into_inner());
        *g = Some(h);
        // 记下被监护那一份的二进制（给窗口导 `CCM_BACKEND_BIN`，见 [`running_backend_bin`]）。
        if let Resolved::Found(p) = &resolved {
            *SUPERVISED_BIN.lock().unwrap_or_else(|e| e.into_inner()) = Some(p.clone());
        }
    }
    // ★ `K-P3b`：另一个返回 `Failed` 的出口，同样从 `note_never_started` 过。
    note_never_started(match resolved {
        Resolved::Found(p) => StartOutcome::Started(p),
        Resolved::Missing { reason, looked_at } => StartOutcome::Failed { reason, looked_at },
    })
}

/// 本机独有的两个读数（远端没有对应物：那个进程在别人机器上）。
pub fn local_pid_and_attempts() -> Result<(Option<u32>, Option<u32>), Said> {
    let g = LOCAL_BACKEND
        .lock()
        .map_err(|e| Said::with_raw(copy_text("rsLocalBackendHost.lock.poisoned", &[]), &e))?;
    if let Some(h) = g.as_ref() {
        return Ok((h.current_pid(), Some(h.attempts())));
    }
    // ★ `K-P1`：常驻那条路。`attempts` 这里**恒 `None`** 而不是 0 ——
    // 那一格的含义是「监护器起过它几次」，而常驻这条路**没有监护器**（`K14` 裁的第一档）。
    // 报 0 会让 UI 显示一个看起来正常的数，而它背后没有任何东西在数。**空值 ≠ 0。**
    let d = DETACHED
        .lock()
        .map_err(|e| Said::with_raw(copy_text("rsLocalBackendHost.lock.poisoned", &[]), &e))?;
    Ok(match d.as_ref() {
        Some(h) if h.pid != 0 => (Some(h.pid), None),
        Some(_) => (None, None),
        None => (None, None),
    })
}

// ════════════════════════════════════════════════════════════════════════════
// **本机中转住本机常驻后端进程里** —— monitor 只交端口与凭据路径
// ════════════════════════════════════════════════════════════════════════════
//
// 先前这里是 `K-H2b` 那一族：monitor 起本机后端那一刻**另监护一个** `--relay` 子进程（第三个进程），
// 「停」按钮与退出臂各收它一次，「中转在不在」问的是 monitor 自己内存里那张句柄表
// （跨 monitor 重启认不出上一次那一个 ⇒ 孤儿必然，「孤儿总是中转」）。
// 「中转 ＋ 上游选择住本机常驻后端进程，对外端口由它绑；
// monitor 不再单独起 / 收中转（本机固定两个进程）」⇒ 那一族整个删掉：
// - 起：后端流模式进程被交了端口就在本进程里起（`src/backend/relay/listen.rs::host`）；
// - 收：随常驻后端按「退出行为」留或退，monitor 一行都不管；
// - 在不在：起会话那一发由那台的 `ccm` 在最终 exec 那一处判；界面上那几格由本机常驻后端自己答（`apikey-routing` 的 `running`）；
// monitor 这一侧先前那个「回环上连一次」的探针（`relay_running`〔散文墓碑〕）随上游选择整块进后端删了。

/// 起本机后端时交给它的那份**环境**。起它的两条载体（常驻那条 `start_detached` 的 `extra_env` ·
/// 被监护那条 `local_backend::start_or_extract` 的 `envs`）与停它的那一趟（[`run_resident_stop`]）交的是**同一份**。
///
/// - 中转端口**显式交**：注入侧（`payload::RELAY_PORT`）与后端里 bind 的是同一个值；后端那一侧没有缺省值。
/// - 家：只在本进程设了 `CCM_DATA_DIR`（隔离跑）时交那一格，原值；没设 ⇒ 缺席，两侧都按家目录推 `~/.cc-monitor`。
///   凭据文件 · 历史注解都住家里，由那台后端按家自己推，这里不交任何「另指位置」的路径。
/// - Claude 目录：用户在设置里填了覆盖 ⇒ 原值交成 `CLAUDE_CONFIG_DIR`（后端只认这一个名字，解析只在后端那一处）；
///   没填 ⇒ 不交、不改，后端照它自己那条规矩解析（`CLAUDE_CONFIG_DIR` → `~/.claude`）。门牌与它无关。
pub(crate) fn backend_env() -> Vec<(String, String)> {
    backend_env_from(
        &|k| std::env::var(k).ok(),
        crate::config::claude_dir_override(),
    )
}

/// 后端认的 Claude 目录那个名字（后端 `agents/claudecode/paths.rs::CONFIG_DIR_ENV`，判据对拍）。
pub(crate) const CLAUDE_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";

/// [`backend_env`] 的本体（环境取值器与设置里的覆盖都注入，判据不改进程环境、不读真配置）。
pub(crate) fn backend_env_from(
    get: &dyn Fn(&str) -> Option<String>,
    claude_dir: Option<std::path::PathBuf>,
) -> Vec<(String, String)> {
    let [port_env, home_env] = HANDED_ENVS;
    let mut envs = vec![(port_env.into(), relay_route_core::PORT.to_string())];
    if let Some(v) = get(home_env).filter(|v| !v.trim().is_empty()) {
        envs.push((home_env.into(), v));
    }
    if let Some(d) = claude_dir {
        envs.push((CLAUDE_DIR_ENV.into(), d.display().to_string()));
    }
    envs
}

// `relay_running` / `relay_listening_at`〔散文墓碑〕退役：本机中转在不在由本机常驻后端自己答
//   （`apikey-routing` · `relay-optin` 的成品里那一格读后端进程内的监听状态；起会话那一发归 `ccm`，见 `INVARIANTS §48.1a`）。

/// P2s（`C8`②）：停本机后端。**句柄取走**（`take`）而不是留着 ——
/// `stop()` 之后那个句柄就是死的（`stopping` 永久置位），留着只会让下一次「起」
/// 误以为还在跑。
pub fn stop_local_backend() -> Result<crate::remote_resident::StopAnswer, Said> {
    use crate::remote_resident::{StopAnswer, StopWord};
    // 起停串行：一趟停从头到尾攥着它（句柄表只在读写那一下拿）。
    let _life = LIFECYCLE
        .lock()
        .map_err(|e| Said::with_raw(copy_text("rsLocalBackendHost.lock.poisoned", &[]), &e))?;
    // 中转住本机后端进程里 ⇒ 停后端就是停中转，这里不再另收一个。
    // ★ `K-P1`：常驻那条路的「停」。
    if let Some(r) = stop_detached() {
        return r;
    }
    let taken = LOCAL_BACKEND
        .lock()
        .map_err(|e| Said::with_raw(copy_text("rsLocalBackendHost.lock.poisoned", &[]), &e))?
        .take();
    match taken {
        // 被监护那条（Windows / `CCM_NO_DETACH`）：`SuperviseHandle::stop()` 直接杀子进程 —— 如实报「强杀」。
        Some(h) => {
            let pid = h.current_pid();
            h.stop();
            *SUPERVISED_BIN.lock().unwrap_or_else(|e| e.into_inner()) = None;
            Ok(StopAnswer {
                stopped: StopWord::Killed,
                pid,
            })
        }
        None => Ok(StopAnswer {
            stopped: StopWord::NotRunning,
            pid: None,
        }),
    }
}

// ══ 取 shim 那个唯一入口的**跨文件出口**〔`K-R7` 09-01，`§0q` 裁一 · 出路乙〕════
//
// `local_backend.rs` 的三个落点也要走这个口 ⇒ 这个测试模块必须 `pub(crate)`，
// 它们按 `crate::local_backend_host::tests::demand_tmux_shim(..)` 取。
//
// **这里原先多一道绕道，现在拆掉了**：模块写成私有 `mod tests`，再在
// 文件末尾补一行 `pub(crate) use tests::demand_tmux_shim;` 重导出一次。那道绕道**不是随手写的**，
// 它当时有一个真理由 —— 09-01 实打，直接写成 `pub(crate) mod tests` **当场打红 17 条**
// （`cargo test -p monitor --lib`：`1194 passed; 17 failed; 13 ignored`；`guard-core` 的反向
// 自检逐字「剥完仍残留 23 个测试属性 —— 剥法坏了」）：当时 `guard_core::test_module_ranges`
// 按**字面前缀**认 `mod `，`pub(crate) mod tests {` 过不了那一关 ⇒ 整段测试代码被当成
// 生产段，各条扫生产段的守卫跟着去扫测试代码。
// `K-R75`（09-12）把那一步换成**按形状**剥可见性修饰（`guard_core::strip_visibility`）之后
// **那个理由不再成立** ⇒ 绕道拆掉，模块写回它本来的样子。
//
// 🔴 **上一版这里指的是一个裸行号**（`guard-core/src/lib.rs` 加一个数字）——
//   那份文件 09-12 一动那个数就漂了，而**漂了没有任何东西会说话**（本区 `[J5 审计正文]` 那一族）。
//   ⇒ 今天指的是**函数名 ＋ 一段机检着的逐字校验位**，不是行号：剥法住
//   `src/common/guard-core/src/lib.rs` 的 `test_module_ranges`；校验位（那一行的原文）
//   **只有一个家** —— 下面 [`tests::the_strip_rule_this_file_leans_on_is_still_on_disk`]
//   里那个 `PIN`，它在盘上找不着就当场红。
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/local_backend_host_tests.rs"]
pub(crate) mod tests;
