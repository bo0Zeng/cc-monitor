//! 常驻后端的起 · 找 · 接 · 停（本机远端同形）。
//!
//! 三条一次性子命令，由 monitor 经本机常驻后端的链路在远端跑（本机那一份由宿主 `local_backend_host` 起 / 停，停也经 `--resident-stop`）：
//! - `--resident-ensure [--replace]`：这台家里那个套接字（`relay_route_core::listen_socket_for`）上已有人在听 ⇒ 不再起；
//!   没人 ⇒ 起一个脱离的自己（常驻载体，只交 `CCM_RESIDENT=1`）。回一行 `{"pid":n|null}`。本进程不等任何东西（零定时器）。
//!   `--replace`：先按下面那个停法停掉在听的那一位（它自己记的 pid 文件），再起（只升不降由 monitor 按 hello 判）。
//! - `--resident-attach`：**小中继**。连这台的套接字，stdin → 套接字、套接字 → stdout 原样对拷，任一边断就退。
//!   monitor 经 ssh exec 跑它（ssh 证明了本人，它在那台以本人身份连本人通道）；连不上 ⇒ stdout 回一行
//!   `{"attach":"refused","reason":"absent"|"unreachable"}` 再退（monitor 读第一行就分得出）。
//! - `--resident-stop [--grace <秒>]`：**同机监督者**那一形（k8s `terminationGracePeriodSeconds` · systemd `TimeoutStopSec`）：
//!   SIGTERM（它排空后自己退）→ 在宽限期内等内核通知 → 到点 SIGKILL → 回 `{"stopped":"graceful"|"killed"|"not_running","pid":n|null}`。
//!   等待住这个一次性进程里，常驻后端的事件循环不加定时器（`no_timer_guard::REGISTERED_ONE_SHOT_CLI_WAITS`）。
//!
//! 门牌按这台的家算（`~/.cc-monitor`，隔离跑时 `CCM_DATA_DIR`）⇒ 一台机器一个家一个常驻后端，与 Claude 目录、与哪一家 agent 都无关。
//! 套接字 · 目录独占锁 · 「谁在听」都住 `<家>/run/`，都由常驻后端自己写（本机远端一个写者）；没有钥匙（门由内核给：目录 `0700` ＋ 对端 uid，`own-chan`）。

use crate::platform::child::{Child, ChildFail};
use crate::platform::child_env::OWN_ENVS;
use std::path::{Path, PathBuf};

use copy_core::copy_text;

/// `--resident-ensure` 起子进程时给的流模式默认旗标（空转那份 watcher 用；每条连接按 attach 行自己的 `flags`）。
/// 与本机宿主 `LOCAL_STREAM_ARGS` 同一组。
/// 流模式显式词打头（本二进制就叫 `ccm` 时零参数是起会话；这里其实已有 `--tail-only` 打头，带上它是为了与宿主那组同形）。
pub(crate) const DEFAULT_STREAM_ARGS: &[&str] = &[
    // 打头的 `--`：后面是后端的词（本二进制就叫 `ccm`，没有它整行交给 claude）。
    "--",
    crate::STREAM_FLAG_EXPLICIT,
    "--tail-only",
    "--with-bg",
    "--with-pid",
];

/// 宽限期默认值（毫秒）。**必须大于**常驻后端自己的退出排空上限（`inbound::DRAIN_DEADLINE`）：
/// 后端先把「哪几条没做完」说出来、自己退；强杀只兜它连那一步都走不到的情形。关系由 `resident_tests` 钉住。
pub(crate) const STOP_GRACE_MS: u32 = 35_000;
/// 强杀之后再等它没了的上限（毫秒）：SIGKILL 是立刻的，还在 ⇒ 卡在内核里（D 态），如实报「没停掉」。
pub(crate) const KILL_WAIT_MS: u32 = 5_000;
/// `--grace` 的上限（秒）：再大就没有意义（monitor 那头一次性那一趟的期限先到）。
const GRACE_MAX_SECS: u32 = 3_600;

/// 远端常驻后端自己的 stderr 诊断文件（本机那一份由宿主交数据目录下的路径）；env 名由 `main.rs` 交（诊断文件的门在那里）。
/// 与本机同一层级 `logs/backend/stderr.log`（滚出来的旧那份在同目录 `stderr.old.log`）。
pub const STDERR_LOG_REL: &str = ".cc-monitor/logs/backend/stderr.log";

fn home() -> Option<PathBuf> {
    crate::platform::paths::home_dir()
}

/// 这台的家：门牌（口 · 钥匙 · 谁在听）只跟着它走。规矩与 monitor 同一份（`creds_core::store::monitor_data_dir`）。
fn data_home() -> Option<PathBuf> {
    creds_core::store::monitor_data_dir(
        std::env::var(creds_core::store::DATA_DIR_ENV)
            .ok()
            .as_deref(),
        home(),
    )
}

/// 这台的家（给 `main.rs` 记「谁在听」、升级那一跳用）。
pub fn here() -> Option<PathBuf> {
    data_home()
}

/// 按交进来的环境算这台的家（`CCM_DATA_DIR`，否则 `$HOME/.cc-monitor`）：常驻载体认门牌用（`main.rs::claim_then_log`，环境是注入的）。
pub fn data_home_from(get: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    creds_core::store::monitor_data_dir(
        get(creds_core::store::DATA_DIR_ENV).as_deref(),
        crate::platform::paths::home_dir_from(&|k| get(k).map(Into::into)),
    )
}

/// 本账号在账号数据库里的家目录（不看 `$HOME`；沙箱那一判的对照物）。
pub fn account_home() -> Option<PathBuf> {
    crate::platform::paths::account_home()
}

/// 一次性子命令的错误出口：stderr 一行 `{code,message}`、退出 2（协议 v1 §3）—— 与 CLI 控制面同一份信封。
/// 两条子命令在复制详情「命令」那一项里的名字。
const ENSURE: &str = "resident-ensure";
const STOP: &str = "resident-stop";
const ATTACH: &str = "resident-attach";

fn fail(cmd: &str, code: &str, message: impl Into<copy_core::said::Said>) -> i32 {
    super::cli_control::emit_err(cmd, code, message)
}

/// `--resident-ensure [--replace]`。`hosted`：宿主层（`main.rs`）交的额外环境 —— 中转口那一格
/// （本层不许伸手进 `relay/`，`layering_guard`）。
pub fn run_ensure(args: &[String], hosted: &[(&str, String)]) -> i32 {
    let (Some(home), Some(dh)) = (home(), data_home()) else {
        return fail(ENSURE, "no_home", copy_text("beResident.home.missing", &[]));
    };
    if args.iter().any(|a| a == "--replace") {
        // 旧的不先让出锁，新的必然起不来 ⇒ 与「停」同一个停法（等它真退了再起）。
        if let Err(e) = stop_owner(&dh, STOP_GRACE_MS) {
            return fail(ENSURE, "replace_failed", e);
        }
    } else if own_chan::someone_listening(&relay_route_core::listen_socket_for(&dh)) {
        // 已有人在听 ⇒ 不再起（起了也拿不到锁）。
        let pid = read_owner(&dh).map(|(p, _)| p);
        println!("{}", serde_json::json!({ "pid": pid }));
        return 0;
    }
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            return fail(
                ENSURE,
                "spawn_failed",
                copy_core::said::Said::with_raw(copy_text("beResident.spawn.noSelf", &[]), &e),
            )
        }
    };
    match spawn_detached(&exe, &child_env(&home, hosted)) {
        Ok(pid) => {
            println!("{}", serde_json::json!({ "pid": pid }));
            0
        }
        Err((code, e)) => fail(ENSURE, code, e),
    }
}

/// 本平台有没有常驻后端（脱离当前进程单独跑 · 听这台家里的套接字）：有 ⇒ `None`；没有 ⇒ 那一句话。
/// `--resident-ensure`（回 `unsupported`）与带常驻开关的流模式（`main.rs::claim_then_log`，退 [`crate::stream::listen::EXIT_BAD_LISTEN_CONFIG`]）
/// 都在建任何目录之前先问它 —— 同一句话、同一处判（判本身住平台层 `platform::child::cannot_detach`）。
pub fn unsupported_here() -> Option<String> {
    crate::platform::child::cannot_detach()
}

/// `--resident-attach`：小中继。连这台的套接字，两向原样对拷，任一边断就退（退出码 0）；
/// 连不上 ⇒ stdout 一行拒绝（理由闭集，`listen::refusal_line`）、退出 2。
pub async fn attach() -> i32 {
    let Some(dh) = data_home() else {
        return fail(ATTACH, "no_home", copy_text("beResident.home.missing", &[]));
    };
    let sock = match own_chan::connect(&relay_route_core::listen_socket_for(&dh)).await {
        Ok(s) => s,
        Err(e) => {
            let reason = relay_refusal(e.kind());
            eprintln!("resident-attach: {e}");
            print!("{}", crate::stream::listen::refusal_line(reason));
            return 2;
        }
    };
    let (mut down, mut up) = tokio::io::split(sock);
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    tokio::select! {
        _ = tokio::io::copy(&mut stdin, &mut up) => {}
        _ = async {
            let _ = tokio::io::copy(&mut down, &mut stdout).await;
            let _ = tokio::io::AsyncWriteExt::flush(&mut stdout).await;
        } => {}
    }
    0
}

/// 中继连不上的那一格（纯函数）：没人在听（文件不在 / 拒连）⇒ `absent`（刚起的还没绑上，monitor 隔一会儿再接）；
/// 别的（不归本人 · 路径坏了）⇒ `unreachable`（再接也一样）。
pub(crate) fn relay_refusal(kind: std::io::ErrorKind) -> &'static str {
    match kind {
        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused => {
            crate::stream::listen::REFUSE_ABSENT
        }
        _ => crate::stream::listen::REFUSE_UNREACHABLE,
    }
}

/// `--resident-ensure` 的入口：宿主层环境 = 中转口 · stderr 诊断文件（凭据与历史注解住家里，那台后端按家自己推）。
pub fn ensure(args: &[String]) -> i32 {
    // 本平台没有常驻 ⇒ 先说，一个目录都不建（诊断文件那层目录也不建）。
    if let Some(why) = unsupported_here() {
        return fail("unsupported", why);
    }
    let hosted = vec![
        // 中转口与这台 `ccm` 起会话时找的是同一个口（同一个函数：这台环境里交了就用交的，否则默认口）。
        (
            crate::stream::listen::RELAY_PORT_ENV,
            crate::accounts::upstream_select::endpoint::relay_port(&|k| std::env::var(k).ok())
                .to_string(),
        ),
        (crate::stderr_log::ENV, format!("~/{STDERR_LOG_REL}")),
    ];
    if let Some(h) = home() {
        // 诊断文件那层目录先建好（`stderr_log` 只 `O_EXCL` 建文件、不建目录）；建不了 ⇒ 子进程装不上、stderr 照旧 null，不拖垮起。
        let _ = log_dir_chain(&h);
    }
    run_ensure(args, &hosted)
}

/// `--resident-stop [--grace <秒>]`。
pub fn run_stop(args: &[String]) -> i32 {
    let Some(dh) = data_home() else {
        return fail(STOP, "no_home", copy_text("beResident.home.missing", &[]));
    };
    let grace_ms = match parse_grace(args) {
        Ok(g) => g,
        Err(e) => return fail(STOP, "bad_args", e),
    };
    match stop_owner(&dh, grace_ms) {
        Ok(end) => {
            println!(
                "{}",
                serde_json::json!({ "stopped": end.word(), "pid": end.pid() })
            );
            0
        }
        Err(e) => fail(STOP, "stop_failed", e),
    }
}

/// `--grace <秒>` ⇒ 毫秒；不给 ⇒ [`STOP_GRACE_MS`]。不大于排空上限的拒：那样后端还没说完就被强杀（纯函数）。
pub(crate) fn parse_grace(args: &[String]) -> Result<u32, String> {
    let Some(i) = args.iter().position(|a| a == "--grace") else {
        return Ok(STOP_GRACE_MS);
    };
    let drain_secs =
        u32::try_from(crate::stream::inbound::DRAIN_DEADLINE.as_secs()).unwrap_or(u32::MAX);
    let bad = || {
        copy_text(
            "beResident.stop.badGrace",
            &[
                ("min", &(drain_secs + 1).to_string()),
                ("max", &GRACE_MAX_SECS.to_string()),
            ],
        )
    };
    let secs: u32 = args
        .get(i + 1)
        .and_then(|v| v.parse().ok())
        .ok_or_else(bad)?;
    if secs <= drain_secs || secs > GRACE_MAX_SECS {
        return Err(bad());
    }
    Ok(secs * 1000)
}

/// 「停」的结局（线上三个词）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stopped {
    /// 收到 SIGTERM 之后在宽限期内自己退了。
    Graceful(u32),
    /// 宽限期满还在 ⇒ 强杀了，而且它确实没了。
    Killed(u32),
    /// 没有记录，或记下的那个已经不在。
    NotRunning,
}

impl Stopped {
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Graceful(_) => "graceful",
            Self::Killed(_) => "killed",
            Self::NotRunning => "not_running",
        }
    }
    pub(crate) fn pid(self) -> Option<u32> {
        match self {
            Self::Graceful(p) | Self::Killed(p) => Some(p),
            Self::NotRunning => None,
        }
    }
}

/// `~/.cc-monitor` → `logs` → `backend` 逐层建（每层 0700，已在的不动）。
fn log_dir_chain(home: &Path) -> Result<(), crate::common::said::Said> {
    let Some(dir) = home.join(STDERR_LOG_REL).parent().map(Path::to_path_buf) else {
        return Ok(());
    };
    let mut chain: Vec<&Path> = dir.ancestors().take_while(|a| *a != home).collect();
    chain.reverse();
    chain.into_iter().try_for_each(ensure_dir)
}

fn ensure_dir(dir: &Path) -> Result<(), crate::common::said::Said> {
    crate::common::own_dir::ensure_private_dir(dir).map_err(|e| {
        crate::common::said::Said::with_raw(
            copy_text(
                "beResident.fs.mkdirFailed",
                &[
                    ("dir", &dir.display().to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })
}

/// 子进程的环境：常驻开关 · 宿主层交的那几格（中转口 · stderr 诊断文件，值里的 `~` 换成家目录）。纯函数，判据钉它。
pub(crate) fn child_env(home: &Path, hosted: &[(&str, String)]) -> Vec<(String, String)> {
    let mut env = vec![(crate::stream::listen::ENV_RESIDENT.into(), "1".into())];
    env.extend(hosted.iter().map(|(k, v)| {
        let v = match v.strip_prefix("~/") {
            Some(rel) => home.join(rel).display().to_string(),
            None => v.clone(),
        };
        (k.to_string(), v)
    }));
    env
}

/// 起一个脱离的自己（常驻载体）：stdio 全空（SSH 断了它不跟着收 SIGPIPE）、自成进程组、不继承 `TMUX`。
/// 自有那几格（常驻开关 · 诊断文件）是**交给它自己用的**，经 `pass_own` 交；别的经 `env`。
fn spawn_detached(
    exe: &Path,
    env: &[(String, String)],
) -> Result<u32, (&'static str, copy_core::said::Said)> {
    let mut cmd = Child::new(exe).args(DEFAULT_STREAM_ARGS).env_remove("TMUX");
    for (k, v) in env {
        cmd = if OWN_ENVS.contains(&k.as_str()) {
            cmd.pass_own(k, v)
        } else {
            cmd.env(k, v)
        };
    }
    cmd.detach().map_err(|e| match e {
        // 不支持：那一句由读的那一方说（「远端只支持 Unix」），`message` 这一格交的就是原话（读的那一方放进复制详情）。
        ChildFail::Io(io) if io.kind() == std::io::ErrorKind::Unsupported => {
            ("unsupported", io.to_string().into())
        }
        e => e.into_cmd_said("spawn_failed", |why| {
            copy_text(
                "beResident.spawn.failed",
                &[("exe", &exe.display().to_string()), ("why", why)],
            )
        }),
    })
}

/// 常驻后端拿到锁、绑上之后记下「谁在听」（`pid\n二进制\n`，`<家>/run/backend.pid`）—— 本机远端都由它自己记。
pub fn record_owner(data_home: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    crate::common::own_state::write(
        &relay_route_core::listen_pid_for(data_home),
        format!("{}\n{}\n", std::process::id(), exe.display()).as_bytes(),
    )
    .map_err(|e| e.logged())
}

/// 读「谁在听」那一份（读不到 / 残缺 ⇒ `None`）。
fn read_owner(data_home: &Path) -> Option<(u32, PathBuf)> {
    std::fs::read_to_string(relay_route_core::listen_pid_for(data_home))
        .ok()
        .and_then(|b| parse_owner(&b))
}

/// 解 pid 文件那两行（纯函数）。
pub(crate) fn parse_owner(body: &str) -> Option<(u32, PathBuf)> {
    let mut lines = body.lines();
    let pid: u32 = lines.next()?.trim().parse().ok()?;
    let bin = lines.next()?.trim();
    (pid != 0 && !bin.is_empty()).then(|| (pid, PathBuf::from(bin)))
}

/// `/proc/<pid>/exe` 读出来的路径是不是那个二进制：部署换过文件之后旧进程读出来带 ` (deleted)`，照样算（纯函数）。
pub(crate) fn exe_matches(seen: &str, recorded: &Path) -> bool {
    let seen = seen.strip_suffix(" (deleted)").unwrap_or(seen);
    Path::new(seen) == recorded
}

/// 按 pid 文件停在听的那一位。没有记录 ⇒ `NotRunning`。
fn stop_owner(data_home: &Path, grace_ms: u32) -> Result<Stopped, String> {
    let path = relay_route_core::listen_pid_for(data_home);
    let Some((pid, bin)) = read_owner(data_home) else {
        return Ok(Stopped::NotRunning);
    };
    let end = stop_pid(pid, &bin, grace_ms, KILL_WAIT_MS)?;
    // 它已不在 ⇒ 那份记录是陈的：还指着它就收掉（下一个起来的会自己再记；指着别人的不动）。
    if read_owner(data_home).is_some_and(|(p, _)| p == pid) {
        let _ = std::fs::remove_file(&path);
    }
    Ok(end)
}

/// 同机监督者本体：先拿进程把手、再核身份（pid 会被复用；拿到把手之后信号只打得到它）→ SIGTERM →
/// 至多等 `grace_ms` → 还在 ⇒ 强杀 → 至多再等 `kill_wait_ms`；还在 ⇒ `Err`（不说「停了」）。
pub(crate) fn stop_pid(
    pid: u32,
    bin: &Path,
    grace_ms: u32,
    kill_wait_ms: u32,
) -> Result<Stopped, String> {
    let Some(target) = crate::platform::signal::stoppable(pid)? else {
        return Ok(Stopped::NotRunning);
    };
    let seen = crate::platform::proc::exe_of(pid)
        .ok_or_else(|| copy_text("beResident.stop.cannotVerify", &[("pid", &pid.to_string())]))?;
    if !exe_matches(&seen, bin) {
        return Err(copy_text(
            "beResident.stop.notOurs",
            &[("pid", &pid.to_string()), ("exe", &seen)],
        ));
    }
    match target.ask_to_finish() {
        Ok(()) => {
            if target.exited_within(grace_ms)? {
                return Ok(Stopped::Graceful(pid));
            }
        }
        // 发不出「请你收尾」（Windows 没有这一格）⇒ 等也等不来它自己退，直接强杀、如实报「强杀」。
        Err(e) => tracing::warn!("{e}"),
    }
    target.kill()?;
    if target.exited_within(kill_wait_ms)? {
        Ok(Stopped::Killed(pid))
    } else {
        Err(copy_text(
            "beResident.stop.stuck",
            &[("pid", &pid.to_string())],
        ))
    }
}

/// 常驻后端抢门牌的结局。
pub enum Claim {
    /// 抢到了：在听的套接字 ＋ 攥着的目录锁（随进程活着）。
    Listening(own_chan::Listener, own_chan::Held),
    /// 别人攥着锁（已有一个常驻后端在听）。
    Held,
    /// 起不来（那一句，已带路径与原因）。
    Failed(String),
}

/// 套接字路径的字节上限（`sockaddr_un.sun_path` 108 字节含结尾的 0；留几格余量）。超了就不绑：内核会截断或拒，截断了就是另一个路径。
pub(crate) const SOCKET_PATH_MAX: usize = 100;

/// 常驻后端抢门牌：建 `<家>/run/`（只给本人）→ 抢它的独占锁 → 记「谁在听」→ 删陈旧的套接字文件 → 绑。没有钥匙。
/// 要在 tokio 运行时里调（绑的是异步套接字）。
pub fn claim(data_home: &Path) -> Claim {
    let dir = relay_route_core::listen_dir_for(data_home);
    let sock = relay_route_core::listen_socket_for(data_home);
    if let Err(e) = ensure_dir(data_home).and_then(|()| ensure_dir(&dir)) {
        return Claim::Failed(e.logged());
    }
    let held = match own_chan::try_hold(&dir) {
        Ok(Some(h)) => h,
        Ok(None) => return Claim::Held,
        Err(e) => return Claim::Failed(bind_failed(&dir, &e)),
    };
    // 「谁在听」先记、再开门：套接字一出现，记录就已经在盘上（`--resident-ensure` 见到有人在听就读它）。
    if let Err(e) = record_owner(data_home) {
        tracing::warn!("记不下「谁在听」（{e}）⇒ 「停」会停不了它");
    }
    let len = sock.as_os_str().len();
    if len > SOCKET_PATH_MAX {
        return Claim::Failed(copy_text(
            "beResident.claim.pathTooLong",
            &[
                ("path", &sock.display().to_string()),
                ("len", &len.to_string()),
                ("max", &SOCKET_PATH_MAX.to_string()),
            ],
        ));
    }
    // 攥着锁 ⇒ 没有别人在听：留下的那个文件是上一个被强杀的常驻后端的。
    match std::fs::remove_file(&sock) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Claim::Failed(bind_failed(&sock, &e)),
    }
    match own_chan::bind(&sock) {
        Ok(l) => Claim::Listening(l, held),
        Err(e) => Claim::Failed(bind_failed(&sock, &e)),
    }
}

fn bind_failed(path: &Path, e: &std::io::Error) -> String {
    copy_text(
        "beResident.claim.bindFailed",
        &[
            ("path", &path.display().to_string()),
            ("why", &copy_core::io_reason(e.kind())),
        ],
    )
}

/// 〔台架防真家〕这一趟是不是沙箱跑、而要占的门牌落在本账号**真家目录**的 `.cc-monitor` 下（纯函数，判据钉它）。
///
/// - 沙箱跑 ＝ 带了 `CCM_SANDBOX=1`，或 `$HOME` 与账号数据库里的家目录不一样（测试与台架改 `$HOME`）。
/// - 门牌 ＝ 这台的家（套接字 · 锁 · 进程记录都在 `<家>/run/`）＋ 交来的诊断文件路径。
///
/// 是 ⇒ 回那一句（拒绝起）。`account_home` 查不到 ⇒ 不判（`None`）：没有对照物，判了也只是拿一个不存在的值去拒人。
pub fn sandbox_refusal(
    get: &dyn Fn(&str) -> Option<String>,
    data_home: &Path,
    account_home: Option<&Path>,
) -> Option<String> {
    let account_home = account_home?;
    let real = account_home.join(".cc-monitor");
    let marked = get(relay_route_core::SANDBOX_ENV).is_some_and(|v| v.trim() == "1");
    let home_moved = crate::platform::paths::home_dir_from(&|k| get(k).map(Into::into))
        .is_some_and(|h| h != account_home);
    if !(marked || home_moved) {
        return None;
    }
    let log = get(crate::stderr_log::ENV).map(PathBuf::from);
    let hit = std::iter::once(data_home.to_path_buf())
        .chain(log)
        .find(|p| p.starts_with(&real))?;
    Some(copy_text(
        "beResident.sandbox.realHome",
        &[("path", &hit.display().to_string())],
    ))
}

/// 〔升级那一跳，跨过 4.1.x 之后删〕4.1.x 及以前的常驻后端听回环 TCP，「谁在听」记在 `<家>/listen-<口>.pid`。
/// 新的常驻后端拿到锁之后、接中转之前调它：那一位还在跑 ⇒ 按旧记录核身份、请它收尾、到点强杀（它占着中转口）；
/// 然后删掉旧记录与旧钥匙文件。回给日志的那一句（没有旧的 ⇒ `None`）。
/// 等待在这里：只在常驻后端起来那一刻走一次（`no_timer_guard` 那张表登记了这一处入口）。
pub fn retire_legacy(data_home: &Path) -> Option<String> {
    let rec = relay_route_core::legacy_listen_pid_for(data_home);
    let (pid, bin) = std::fs::read_to_string(&rec)
        .ok()
        .and_then(|b| parse_owner(&b))?;
    let said = match stop_pid(pid, &bin, STOP_GRACE_MS, KILL_WAIT_MS) {
        Ok(end) => copy_text(
            "beResident.legacy.stopped",
            &[("pid", &pid.to_string()), ("how", end.word())],
        ),
        Err(e) => copy_text(
            "beResident.legacy.stuck",
            &[("pid", &pid.to_string()), ("why", &e)],
        ),
    };
    let _ = std::fs::remove_file(&rec);
    let _ = std::fs::remove_file(data_home.join(relay_route_core::LEGACY_LISTEN_TOKEN_NAME));
    Some(said)
}

#[cfg(test)]
#[path = "../../../tests/backend/control/resident_tests.rs"]
mod tests;
