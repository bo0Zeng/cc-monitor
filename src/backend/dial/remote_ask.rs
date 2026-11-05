//! **本机后端问远端后端**的那一跳 —— 全后端**只此一处**。
//!
//! 「观测方沿它本来就拥有的那条连接去拉被观测方」（零新通道）。用它的：资产目录同步（[`command_line`] ＋ [`DialRemote`]）·
//! 两台之间的枢纽 · 公钥（[`ask_json`]）· 那台常驻的清单（[`ask_resident`]）· 公钥与部署计划（[`capture_full`]）· 端口转发与探测（[`REACH`]）。
//!
//! # 形状（乙：池里那条连接上多开一个 capture exec）
//!
//! ```text
//!  调用方 ── ask_json(machine, cmd, args, 那几台, 对面) ──▶ 这一端的机器表按名字组请求 ──▶ DialRemote.run_coded(dial, "<落点> -- '--<cmd>' '--stdin-line'", stdin = args 一行)
//!                                          └─ ssh_dial::uses::capture（池里那条 SSH，多一个 exec 通道，不是新连接）
//! ```
//!
//! # 为什么是 capture 一次性子命令，不起远端流模式
//!
//! 流模式一起来就往 tmux server 装**全局** hook（`control/tmux_hook.rs::install_hooks`，载荷里烤着
//! **那个进程**的 pid）—— 一个用完就退的流会把 monitor 那条真流的 hook 盖成一个死 pid。
//! 老后端不认子命令会进流模式、第一行是 hello ⇒ capture 带 `abort_marker`，见到就收工、报「太旧」。
//!
//! # 那几台（[`Table`]）
//!
//! 怎么够到一台只看这一端的机器表（`~/.cc-monitor/machines.json`，读法由 faces 装进拨号层，[`super::machines_now`]）：
//! 按名字组请求（[`super::request_named`]），线上没有谁交拨号。拨号请求里只有路径（`key_path`），没有私钥本体（凭据面 `K11`）。
//! 另记一样：对面的资产目录 id（按名字，只在本进程里，后端重启就空；`assets-sync` 拉过那台才有）。

use crate::common::said::Said;
use cmd_wire::dial::OLD_BACKEND_MARKER;
use copy_core::copy_text;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

use serde_json::{json, Value};

/// 拉回来那一份（远端 stdout）的上限：与资产目录文件的读上限同一个数（目录本身就这么大封顶；那份文件的名字与上限只住声明表）；
/// 超了整份不收、回错。
pub(crate) const PULL_MAX_BYTES: relay_route_core::Cap = relay_route_core::Cap {
    bytes: 16 * 1024 * 1024,
    over: relay_route_core::Over::Reject,
};

/// 那台不认这条命令 / 子命令：与帧协议里同一个码。
const UNKNOWN_COMMAND: &str = "unknown_command";

/// 对面：在那台上跑一条一次性命令，交回它的 stdout。生产 = [`DialRemote`]（经 `dial` 的 capture）；判据用替身。
/// `stdin`= exec 之后写进那个进程 stdin 的字节（缺席 = 不写）；收的一侧用 CLI 面的「只读一行」入口。
pub trait Remote: Send + Sync {
    fn run<'a>(
        &'a self,
        dial: &'a Value,
        command: String,
        stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>>;

    /// 同 [`Self::run`]，失败时**连那台 CLI 信封里的码一起交回**（[`Said`]）——
    /// 枢纽要把被写那台的 `stale` / `refused` / … 原码转给界面，不许压成一个。缺省实现：码缺席（替身不必各写一份）。
    fn run_coded<'a>(
        &'a self,
        dial: &'a Value,
        command: String,
        stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, Said>> + Send + 'a>> {
        Box::pin(async move { self.run(dial, command, stdin).await.map_err(Said::from) })
    }
}

/// 那几台：怎么够到一台照这一端的机器表（按名字组请求）＋ 对面的资产目录 id（按名字记，只在本进程里，后端重启就空）。
/// 生产只有一张（[`REACH`]，现读机器表）；判据各给一张固定的（`Table::of`，住判据那一份 `remote_ask_peers.rs`）。
pub struct Table {
    /// `None` ⇒ 现读这一端的机器表（[`super::machines_now`]）。
    fixed: Mutex<Option<Vec<super::Host>>>,
    /// 名字 → 对面的资产目录 id（`assets-sync` 拉过那台才有）。
    peers: Mutex<BTreeMap<String, String>>,
}

/// 本进程那一张（现读这一端的机器表）。
pub static REACH: Table = Table {
    fixed: Mutex::new(None),
    peers: Mutex::new(BTreeMap::new()),
};

impl Table {
    /// 此刻的机器表。
    pub fn hosts(&self) -> Vec<super::Host> {
        match &*self.fixed.lock().unwrap_or_else(|e| e.into_inner()) {
            Some(h) => h.clone(),
            None => super::machines_now(),
        }
    }

    /// 机器表里有没有叫这个名字的那一台。
    pub fn has(&self, machine: &str) -> bool {
        self.hosts().iter().any(|h| h.name() == machine)
    }

    /// 机器表里「连这台」的那几台的名字（照表里的先后）：「每一台都做一趟」（`assets-sync`）· 可达那一列说的就是它们。
    pub fn connected(&self) -> Vec<String> {
        self.hosts()
            .iter()
            .filter(|h| h.connect)
            .map(|h| h.name().to_string())
            .collect()
    }

    /// 那台够不够得到：表里有 ⇒ 一份组好的拨号请求（用法 `capture`，[`DialRemote`] 拿去改成那一趟要的）；
    /// 没有这一台 ⇒ 码 `unreachable`；表里那几格组不成请求（跳板成环 · 找不到 · 缺地址）⇒ 那一码那一句。
    pub fn dial(&self, machine: &str) -> Result<Value, Said> {
        let hosts = self.hosts();
        if !hosts.iter().any(|h| h.name() == machine) {
            return Err(Said::new("unreachable", unreachable_message(machine)));
        }
        super::request_named(&hosts, machine, "capture", json!({}))
    }

    /// 对面的资产目录 id（没拉过 ⇒ `None`）。
    pub fn peer(&self, machine: &str) -> Option<String> {
        self.peers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(machine)
            .cloned()
    }

    /// 记下对面的资产目录 id。
    pub fn set_peer(&self, machine: &str, id: &str) {
        self.peers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(machine.to_string(), id.to_string());
    }
}

/// 「连这台」的那几台此刻：`[{origin, machine}]`（线上那一行住 cmd-wire，`assets-sync` 的回包带它）。
pub fn reach_rows(table: &Table) -> Vec<cmd_wire::assets::ReachRow> {
    table
        .connected()
        .into_iter()
        .map(|o| cmd_wire::assets::ReachRow {
            machine: table.peer(&o),
            origin: cmd_wire::Machine::named(o),
        })
        .collect()
}

/// 远端上那条一次性命令的完整字面：`<落点> <argv…>`，argv 每一格都过 POSIX 单引号。
/// 那台后端恒在固定落点（`relay_route_core::BACKEND_LANDING_SHELL`，可填的 `backendPath` 删了）。
pub fn command_line(argv: &[&str]) -> String {
    // `ccm -- <后端子命令…>`：打头的 `--` 让那台的 `ccm` 当后端用。
    let mut s = format!("{} --", relay_route_core::BACKEND_LANDING_SHELL);
    for a in argv {
        s.push(' ');
        s.push_str(&shell_quote_core::posix_quote(a));
    }
    s
}

/// 够不到那台时给的那句话（不猜、不回落）。
pub fn unreachable_message(machine: &str) -> String {
    copy_text(
        "beRemoteAsk.unreachableMessage.say",
        &[("machine", machine)],
    )
}

/// 问可达表里那一台跑一条**帧命令的 CLI 面**（`--<cmd> --stdin-line`），
/// 入参类型序列化的那一份一行写进它的 stdin，它 stdout 那一行按回包类型解。两台之间那几件的枢纽（`assets/hub.rs`）· 公钥推送 · 偏好迁移用它；
/// 命令行里只有落点与两个旗标（同 `assets-sync` 推那一趟），载荷走 stdin（远端登录 shell 是什么都同读）。
/// `tz` ＝ 看的人那一台的时区（引起这一趟的请求信封里那一格）：经 [`crate::wire::cli_words::TZ_FLAG`] 转给那台，它回的「几点」照它写。
pub async fn ask_json<C: crate::wire::typed::Command>(
    machine: &str,
    req: &C::Req,
    tz: &crate::common::time::Tz,
    table: &Table,
    remote: &dyn Remote,
) -> Result<C::Reply, Said>
where
    C::Req: serde::Serialize,
    C::Reply: serde::de::DeserializeOwned,
{
    let dial = table.dial(machine)?;
    ask_via(&dial, machine, C::NAME, req, tz, remote).await
}

/// 同 [`ask_json`]，那一台不按名字查可达表、直接给拨号请求（两台之间复制：请求里带着两台的拨号请求）；
/// 命令由线上入参类型选（名字 · 回包类型都挂在它上面）。
pub async fn ask_dialed<C: cmd_wire::Call>(
    dial: &Value,
    req: &C,
    tz: &crate::common::time::Tz,
    remote: &dyn Remote,
) -> Result<C::Reply, Said> {
    ask_via(dial, &copy_core::peer_machine(), C::NAME, req, tz, remote).await
}

/// 那台的回包按回包类型解；解不出 ⇒ 码 `backend_differs`、说那台后端版本不同，解码器那一句进原话。
pub fn reply_of<R: serde::de::DeserializeOwned>(
    machine: &str,
    cmd: &str,
    v: Value,
) -> Result<R, Said> {
    serde_json::from_value(v).map_err(|e| {
        Said::new(
            "backend_differs",
            crate::common::contract::joined(
                copy_core::backend_differs(machine),
                Some(format!("{cmd}: {e}")),
            ),
        )
    })
}

async fn ask_via<Req: serde::Serialize, Reply: serde::de::DeserializeOwned>(
    dial: &Value,
    machine: &str,
    cmd: &str,
    req: &Req,
    tz: &crate::common::time::Tz,
    remote: &dyn Remote,
) -> Result<Reply, Said> {
    let args = serde_json::to_value(req).map_err(|e| Said::from(e.to_string()))?;
    let flag = crate::wire::cli_words::cli_flag(cmd);
    let zone = tz.iana_name();
    let argv = [
        flag.as_str(),
        crate::wire::cli_words::STDIN_LINE_FLAG,
        crate::wire::cli_words::TZ_FLAG,
        zone.as_str(),
    ];
    let v = exchange(
        dial,
        machine,
        &command_line(&argv),
        format!("{args}\n"),
        remote,
    )
    .await?;
    reply_of(machine, cmd, v)
}

/// 问那一台的**常驻**跑一条帧命令（`--resident-call --stdin-line`，命令名与入参一行进 stdin）：问到的是那台热着的那一个
/// （它的缓存 · 备忘都在），不在那台起一个冷进程；那台常驻没在跑 ⇒ `resident_absent`（那一句说那台的名字）。
/// `total` ＝ 发起方这一问愿意等多久（[`crate::platform::child::Deadline::within`] 算好的），经期限口交给那台、本机也只等这么久。
pub async fn ask_resident<C: crate::wire::typed::Command>(
    machine: &str,
    req: &C::Req,
    tz: &crate::common::time::Tz,
    total: crate::platform::child::Deadline,
    table: &Table,
    remote: &dyn Remote,
) -> Result<C::Reply, Said>
where
    C::Req: serde::Serialize,
    C::Reply: serde::de::DeserializeOwned,
{
    let dial = table.dial(machine)?;
    let call = crate::wire::listen::ResidentCall {
        cmd: C::NAME.to_string(),
        args: serde_json::to_value(req).map_err(|e| Said::from(e.to_string()))?,
    };
    let line = serde_json::to_string(&call).map_err(|e| Said::from(e.to_string()))?;
    let ms = total.as_ms().to_string();
    let zone = tz.iana_name();
    let argv = [
        crate::wire::cli_words::RESIDENT_CALL_FLAG,
        crate::wire::cli_words::STDIN_LINE_FLAG,
        crate::wire::cli_words::WITHIN_MS_FLAG,
        ms.as_str(),
        crate::wire::cli_words::TZ_FLAG,
        zone.as_str(),
    ];
    let command = command_line(&argv);
    let asked = exchange(&dial, machine, &command, format!("{line}\n"), remote);
    let v = match crate::platform::child::bounded(total, asked).await {
        Ok(got) => got,
        Err(after) => Err(Said::new(
            crate::wire::listen::CALL_TIMED_OUT,
            crate::wire::listen::call_timed_out(machine, after),
        )),
    }
    .map_err(|s| match s.code.as_deref() {
        Some(crate::wire::listen::CALL_ABSENT) => {
            s.wrap(|_| crate::wire::listen::call_absent(machine))
        }
        _ => s,
    })?;
    reply_of(machine, C::NAME, v)
}

/// [`hop`] 交给那台最多等多久（发起方给得更短 ⇒ 照发起方）。
const HOP_CAP: crate::platform::child::Deadline = crate::platform::child::Deadline::secs(10);

/// **跨台那一跳**：写只住会话所在那台的东西（会话输入队 · 收件箱），会话在别台 ⇒ 经 [`ask_resident`] 把同一条交给那台的常驻。
/// `here` ＝ 同一份入参、`machine` 已换成那台眼里的本机；`ours` ＝ 这一条码表里的码：那台答了其中一个（那台说「不行」）⇒ 原样交回；
/// 别的（不在机器表 · 链路坏 · 常驻没在跑 · 过了期限）⇒ `offline`、那一句照连接层写的，本机一个字节不写、不攒着回头再发。
/// `machine` 没有名字（本机）⇒ `bad_args`：本机的调用方就地做，不走这里。
pub async fn hop<C: crate::wire::typed::Command>(
    machine: &cmd_wire::Machine,
    here: &C::Req,
    ours: &[&str],
    tz: &crate::common::time::Tz,
    until: Option<crate::platform::child::Until>,
    table: &Table,
    remote: &dyn Remote,
) -> Result<C::Reply, Said>
where
    C::Req: serde::Serialize,
    C::Reply: serde::de::DeserializeOwned,
{
    let Some(host) = machine.host_name() else {
        return Err(crate::wire::spec::fail(
            "bad_args",
            crate::common::contract::malformed("`machine` must name a machine"),
        ));
    };
    ask_resident::<C>(
        host,
        here,
        tz,
        crate::platform::child::Deadline::within(HOP_CAP, until),
        table,
        remote,
    )
    .await
    .map_err(|s| match s.code.as_deref() {
        Some(c) if ours.contains(&c) => crate::wire::spec::fail(c, s.said),
        _ => crate::wire::spec::fail("offline", s.said),
    })
}

/// 一台的结局：答回来的那一份 · 那台的码与那一句。
pub(crate) type Asked<R> = (cmd_wire::Machine, Result<R, Said>);

/// **一问问几台**（跨台合并那几条：本机后端自己扇出、并好再答，界面不当信差）：几台问同一份入参，经 [`ask_apiece`]。
pub async fn ask_each<'a, C, F>(
    machines: &[cmd_wire::Machine],
    req: &'a C::Req,
    local: F,
    tz: &'a crate::common::time::Tz,
    until: Option<crate::platform::child::Until>,
    table: &'a Table,
    remote: &'a dyn Remote,
) -> Result<Vec<Asked<C::Reply>>, String>
where
    C: crate::wire::typed::Command,
    C::Req: serde::Serialize + Sync,
    C::Reply: serde::de::DeserializeOwned + Send + 'a,
    F: Future<Output = Result<C::Reply, Said>> + Send + 'a,
{
    let asks = machines.iter().map(|m| (m.clone(), req)).collect();
    ask_apiece::<C, _, _, _>(asks, |_| local, tz, until, table, remote).await
}

/// **一问问几台、每台各一份入参**：本机那一台的那一份交给 `local` 答（同进程，不经 SSH），
/// 别的每台经 [`ask_resident`] 拿它那一份问它的常驻（几台同时问）。结局照 `asks` 的先后、一台一格（重名的只问头一份）：
/// 那台够不着 · 常驻不在 · 答了失败 ⇒ 那一格是那台的码与那一句，别台照常。
/// 等多久只归发起方：`until` ＝ 发起方给的截止时刻，每台都只等到那一刻；问到别的台而它缺 ⇒ `Err`（`bad_args` 那一句），这里不自己定一个。
pub async fn ask_apiece<'a, C, Q, L, F>(
    asks: Vec<(cmd_wire::Machine, Q)>,
    local: L,
    tz: &'a crate::common::time::Tz,
    until: Option<crate::platform::child::Until>,
    table: &'a Table,
    remote: &'a dyn Remote,
) -> Result<Vec<Asked<C::Reply>>, String>
where
    C: crate::wire::typed::Command,
    C::Req: serde::Serialize + Sync,
    Q: std::borrow::Borrow<C::Req> + Send + 'a,
    C::Reply: serde::de::DeserializeOwned + Send + 'a,
    L: FnOnce(Q) -> F,
    F: Future<Output = Result<C::Reply, Said>> + Send + 'a,
{
    let mut seen: Vec<(cmd_wire::Machine, Q)> = Vec::new();
    for (m, q) in asks {
        if !seen.iter().any(|(s, _)| *s == m) {
            seen.push((m, q));
        }
    }
    let remote_named = seen.iter().any(|(m, _)| !m.is_local());
    let total = match until {
        Some(u) => crate::platform::child::Deadline::millis(u.left_ms()),
        None if remote_named => {
            return Err(crate::common::contract::malformed(
                "`within_ms` is required when asking another machine: how long to wait belongs to the asker",
            ))
        }
        None => crate::platform::child::Deadline::millis(0),
    };
    let mut local = Some(local);
    let mut machines = Vec::with_capacity(seen.len());
    let mut asks: Vec<Asking<'a, C::Reply>> = Vec::new();
    for (m, q) in seen {
        if m.is_local() {
            match local.take() {
                Some(f) => asks.push(Box::pin(f(q))),
                None => unreachable!("本机那一台只列一次（上面去过重）"),
            }
        } else {
            let name = m.host_name().unwrap_or_default().to_string();
            asks.push(Box::pin(async move {
                let req = <Q as std::borrow::Borrow<C::Req>>::borrow(&q);
                ask_resident::<C>(&name, req, tz, total, table, remote).await
            }));
        }
        machines.push(m);
    }
    let got = all(asks).await;
    Ok(machines.into_iter().zip(got).collect())
}

/// 问一台的那一趟（本机那一问 · 别台的常驻那一问同一形，交给 [`all`] 同时等）。
type Asking<'a, R> = Pin<Box<dyn Future<Output = Result<R, Said>> + Send + 'a>>;

/// 几个问法同时等、都回来才交（照交进来的先后）。
async fn all<'a, T>(mut asks: Vec<Pin<Box<dyn Future<Output = T> + Send + 'a>>>) -> Vec<T> {
    let mut out: Vec<Option<T>> = asks.iter().map(|_| None).collect();
    std::future::poll_fn(|cx| {
        let mut waiting = false;
        for (f, slot) in asks.iter_mut().zip(out.iter_mut()) {
            if slot.is_none() {
                match f.as_mut().poll(cx) {
                    std::task::Poll::Ready(v) => *slot = Some(v),
                    std::task::Poll::Pending => waiting = true,
                }
            }
        }
        if waiting {
            std::task::Poll::Pending
        } else {
            std::task::Poll::Ready(())
        }
    })
    .await;
    out.into_iter().flatten().collect()
}

/// 跑那一条命令、读它 stdout 那一行 JSON（那台不认这条子命令 ⇒ 说那台太旧）。
async fn exchange(
    dial: &Value,
    machine: &str,
    line: &str,
    stdin: String,
    remote: &dyn Remote,
) -> Result<Value, Said> {
    let out = remote
        .run_coded(dial, line.to_string(), Some(stdin))
        .await
        .map_err(|s| match s.code.as_deref() {
            Some(UNKNOWN_COMMAND) => s.wrap(|_| copy_core::backend_old(machine)),
            _ => s,
        })?;
    serde_json::from_str(out.trim()).map_err(|e| {
        tracing::warn!("reply from {machine} is not JSON: {e}");
        Said::from(copy_core::reply_unreadable(machine))
    })
}

// ───────────────────────── 生产那一个对面：经 dial 的 capture ─────────────────────────

/// 经本机常驻后端池里那条 SSH 连接跑 capture（`ssh_dial::uses::capture`，与 monitor 开的链路同一个池，零新连接）。
pub struct DialRemote;

/// 一份组好的拨号请求（[`Table::dial`] · [`crate::dial::request_named`] 那一形）⇒「capture 这一条命令」的拨号请求
/// （照类型组；纯，抽出来是为了判据 —— 载荷真进了 `capture.stdin`，不靠真 sshd 也验得动）。
pub(crate) fn for_capture(
    dial: &Value,
    command: String,
    stdin: Option<String>,
) -> Result<crate::dial::DialRequest, String> {
    let mut req = crate::dial::read_request(dial).map_err(|f| f.said)?;
    req.use_ = crate::dial::Use::Capture;
    req.command = command;
    req.capture = Some(crate::dial::CaptureOpts {
        max_bytes: PULL_MAX_BYTES.as_len(),
        abort_marker: Some(OLD_BACKEND_MARKER.to_string()),
        stdin,
    });
    req.stages = false;
    req.probe = false;
    Ok(req)
}

impl Remote for DialRemote {
    fn run<'a>(
        &'a self,
        dial: &'a Value,
        command: String,
        stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async move {
            self.run_coded(dial, command, stdin)
                .await
                .map_err(|s| s.said)
        })
    }

    fn run_coded<'a>(
        &'a self,
        dial: &'a Value,
        command: String,
        stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, Said>> + Send + 'a>> {
        Box::pin(async move {
            let req = for_capture(dial, command, stdin).map_err(Said::from)?;
            pull_over_coded(async move { ssh_dial::uses::capture(&req).await }).await
        })
    }
}

/// 一个 `spawn` 出去的任务，**句柄被丢 ⇒ 任务被收**。
///
/// tokio 的 `JoinHandle` 被丢是**脱钩**（任务照跑），不是收。攥着池里那条 SSH 连接一格通道的内层任务（端口转发 · 测试连接那条链路）
/// 包在它里面：调用方帧期限到点 ⇒ monitor 补发 `cancel` ⇒ 后端打断的是**外层** future —— 句柄若只是被丢，内层那一格就**永远占着**。
pub(crate) struct AbortOnDrop(pub(crate) tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// 跑一趟收全（生产 = `ssh_dial::uses::capture`），把结果判成 stdout 或一句错。收全那一趟就是这个 future 的一部分：
/// 外层被丢（`cancel` 打断）⇒ 它一起被丢、那一格放掉。**抽出来是为了判据**（替身交一个永不答 / 答好了的结果，不需要真 SSH）。
/// 失败时带上那台 CLI 信封里的码（`{code, message}` 读得出来才有）；只要话的调用方取 `.said`。
pub(crate) async fn pull_over_coded<F>(capture: F) -> Result<String, Said>
where
    F: Future<Output = Result<crate::dial::Captured, ssh_dial::DialFail>>,
{
    let got = capture.await.map_err(|f| Said::from(unreachable(&f)))?;
    settle_pulled(&got)
}

/// 没开成 ⇒「那台够不着」那一句，带拨号核心写好的那一句。
fn unreachable(f: &ssh_dial::DialFail) -> String {
    copy_text("beRemoteAsk.run.unreachable", &[("why", &f.said)])
}

/// **原样收全**一条一次性命令（退出码 · stdout · stderr 三样都交回，不替调用方判退出码）：部署计划那两问
/// （`uname` · 扫身份戳）要的正是这三样 —— `grep` 退出 1 是「一个都没有」，不是失败。拨号请求同 [`for_capture`]。
pub async fn capture_full(dial: &Value, command: String) -> Result<crate::dial::Captured, String> {
    let req = for_capture(dial, command, None)?;
    ssh_dial::uses::capture(&req)
        .await
        .map_err(|f| unreachable(&f))
}

/// ssh 自己出错（没连上 · 认证没过 · 中途断开）的退出码（`man ssh`：「exits with 255 if an error occurred」）。
const SSH_OWN_FAILURE: u32 = 255;

/// 一次性子命令的结果那一行 → 它的 stdout（老后端 · 非 0 退出 · 超上限各是一句错）。
/// 非 0 退出时把那台 CLI 信封里的码一起交回（[`Said`]）。
fn settle_pulled(got: &crate::dial::Captured) -> Result<String, Said> {
    let stdout = got.stdout.as_str();
    if stdout.contains(OLD_BACKEND_MARKER) {
        // 那台的后端不认这条子命令（掉进了流模式）：码同「不认这条命令」，话按码取；知道那台名字的调用方换成名字（[`ask`]）。
        return Err(Said::new(
            UNKNOWN_COMMAND,
            copy_core::backend_old(&copy_core::peer_machine()),
        ));
    }
    if got.exit_status != Some(0) {
        let stderr = got.stderr.as_str();
        // 那台 CLI 面的失败信封（`stream::cli::emit_err` 那一份）：读得出来就把码与原话都取出来。
        #[derive(serde::Deserialize)]
        struct Envelope {
            code: Option<String>,
            message: Option<String>,
        }
        // 那台答了一个错误 ⇒ 就说它那一句（码随 `Said.code` 走，不进话里）；不是信封的原样 stderr 才加一个短头。
        let envelope = serde_json::from_str::<Envelope>(stderr.trim()).ok();
        let code = envelope.as_ref().and_then(|e| e.code.clone());
        let message = match envelope.and_then(|e| e.message) {
            Some(m) if !m.trim().is_empty() => m,
            // ssh 自己出错退 255、那台一个字不回 ⇒ 是这一跳的连接断了（没连上 · 中途断开），不是那台的命令失败。
            _ if stderr.trim().is_empty() && got.exit_status == Some(SSH_OWN_FAILURE) => copy_text(
                "beRemoteAsk.run.unreachable",
                &[("why", &copy_text("beRemoteAsk.run.hopDropped", &[]))],
            ),
            _ if stderr.trim().is_empty() => copy_text("beRemoteAsk.run.failedNoReason", &[]),
            // 不是信封的原样 stderr：句子只说「后端报错」，那台的原话记一行日志（这一跳的失败形没有原话位）。
            _ => crate::common::said::IntoNote::into_note(crate::common::said::Said::with_raw(
                copy_text("beRemoteAsk.run.failed", &[]),
                stderr.trim(),
            )),
        };
        let said = Said::from(message);
        return Err(match code {
            Some(c) => said.coded(&c),
            None => said,
        });
    }
    if stdout.len() >= PULL_MAX_BYTES.as_len() {
        return Err(Said::from(copy_text(
            "beRemoteAsk.run.tooLarge",
            &[("max", &PULL_MAX_BYTES.bytes.to_string())],
        )));
    }
    Ok(stdout.to_string())
}

#[cfg(test)]
#[path = "../../../tests/backend/stream/remote_ask_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/dial/remote_ask_tests.rs"]
mod hop_tests;

// 判据用的「那几台」（替身）：跨台合并那几条的判据共用（上层的判据经 `test-support` 够到）。
#[cfg(any(test, feature = "test-support"))]
#[path = "../../../tests/backend/stream/remote_ask_peers.rs"]
pub mod test_peers;
