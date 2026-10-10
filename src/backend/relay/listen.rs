//! 中转的**宿主那一半**：绑回环 · accept · 在途上界 · 两个期限值 · 起监听之前那点接线（读 / 铸钥匙、问上游选择）。
//!
//! 中转本身（一条连接的一来一回）是另一个 crate（`comms_outward`）：本文件接下一条连接就交给 `comms_outward::serve_one`，
//! 端口、钥匙、期限、上游选择那只手、tap 口都由这里交进去（`Relay::new`）——
//! 中转自己不绑口、不读盘、不读环境（边界判据 `C4` / `C5`）。

use super::key;
use crate::common::net::LOOPBACK;
use comms_outward::{Relay, Startup, TeeSink};
use copy_core::copy_text;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::Arc;

/// 同时在途的下游连接数上限。住宿主不住中转：上界与绑口同是策略值。
/// 是条数不是体量（名字里刻意不带 `MAX`/`CAP`/`LIMIT`/`BYTES`：那是 `byte_cap_registry` 的钩子）。超了回 `503` 并关连接，不是静默 FIN。
pub(super) const INFLIGHT_CONNECTIONS: usize = 256;

pub(crate) const ENV_PORT: &str = "CCM_RELAY_PORT";

//
// ══════════════════════════════════════════════════════════════════════════
// 期限的值住这一层
// ══════════════════════════════════════════════════════════════════════════
//
// 值归后端 · 执行归通信层 · 说法归调用方：期限与端口号同是策略值，通信层自己没有任何期限常量。
// 装它们的那两手在中转（`server::apply_downstream_deadline` 与 `upstream::connect`），收入参。
// 两个值在 `no_timer_guard::REGISTERED_DURATION_USES` 里各占一行（恰好相等的断言）；不许搬回中转 crate（那边生产段零期限字面量）。

/// 下游那条 socket 的读写期限。`INFLIGHT_CONNECTIONS` 买「顶不满、拒绝有声」，这一条买「顶住的那些会自己散」：
/// 没有它，256 条半开连接能把中转永久钉死，而它会礼貌地回 503，读起来像正常限流。
///
/// 它不是定时器：`SO_RCVTIMEO` / `SO_SNDTIMEO` 只管这一次阻塞的读写最多等多久，不让任何线程自己醒来。
/// 配套硬约束：期限到了就把这条连接结掉，任何一层都不许重试（`pump` 与 `http1` 的读循环只对 `Interrupted` `continue`）——
/// 一重试就成了轮询。
///
/// 30 秒：对端就在本机（`listen()` 绑 `LOOPBACK`），回环上搬完一条最大请求体（`BODY_CAP` = 64 MiB）是零点零几秒的量级
/// （按量级推，没实测）。设长则半开连接要很久才散、读起来像挂死；设短则负载高时合法的大请求体被掐掉、还不好查。
/// 不跟上游用同一个数：下游请求在内存里已经拼好，慢是异常；上游是「模型在想」，慢是正常（见 [`UPSTREAM_DEADLINE`]）。
pub(super) const DOWNSTREAM_DEADLINE: std::time::Duration =
    std::time::Duration::from_millis(30_000);

/// 上游那条 socket 的读写期限。同样不是定时器（只限这一次阻塞的读写），期限到了把连接结掉、不许任何一层重试
/// （`pump` 与 `http1` 只重试 `Interrupted`；`rustls` 的 `complete_io` 同形）。
///
/// 600 秒：这一跳等的是模型在想，上游几十秒不发一个字节是 SSE 长流的正常形态（参考实现给上游也是 600 秒）。
/// 改小会把一条正在正常吐字、只是中间想了 40 秒的长流从中间提断 —— 比不设期限更坏；改大则一条死掉但没发 FIN 的上游
/// 会多钉住一条线程那么久。不拿「上游会周期发 `ping`」当依据：这个数要在上游合法地整段沉默时也站得住。
///
/// `TcpStream::connect` 本身没有连接期限：那一步有内核 SYN 重试上限与解析器自己的上限兜着（具体多少没量，只能说不是无界）。
pub(super) const UPSTREAM_DEADLINE: std::time::Duration = std::time::Duration::from_millis(600_000);

/// 起监听。返回真实绑定的地址（端口给 0 时由内核选，测试用）。
pub(crate) fn listen(port: u16) -> std::io::Result<TcpListener> {
    let listener = TcpListener::bind(SocketAddr::new(LOOPBACK, port))?;
    Ok(listener)
}

/// 接受循环。阻塞在 `accept()` 上 —— 那是内核事件，不是定时器。
///
/// 在途连接数有上界，且拒绝是出声的：超过 `INFLIGHT_CONNECTIONS` 回 503 并关连接；spawn 失败同样回 503 并出声
/// （不然下游拿到一个没有任何 HTTP 响应的 FIN）。顶住的那些会自己散：`apply_downstream_deadline` 在两处装 `DOWNSTREAM_DEADLINE`
/// —— 这里 `accept` 出来那一刻（覆盖 503 那条支）· `handle()` 开头；上游那条 socket 由 `upstream::connect` 装 [`UPSTREAM_DEADLINE`]。
///
/// `inflight` 是在途计数（本函数进出各动一次），由调用方交进来 —— 与上界同住宿主这一侧。
pub(crate) fn serve(listener: TcpListener, relay: Arc<Relay>, inflight: Arc<AtomicUsize>) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else {
            continue;
        };
        // 期限先装上、在分流之前：每条 accept 出来的 socket 从第一刻起就带期限，不管它接下来走哪个分支（纵深；503 那条支
        // 只写 ~90 字节，没构造出它阻塞的形状）。装不上仍然关连接并出声：宁可拒绝，也不放一条来路不明的进来。
        if let Err(e) = comms_outward::apply_downstream_deadline(&stream, DOWNSTREAM_DEADLINE) {
            eprintln!("[relay] cannot set connection deadline: {e}");
            continue;
        }
        if inflight.load(SeqCst) >= INFLIGHT_CONNECTIONS {
            // 只印数字与上限，永不印请求头（这一支连一个字节都还没读）。
            eprintln!("[relay] refusing: {INFLIGHT_CONNECTIONS} connections already in flight");
            let _ = comms_outward::refuse_busy(&mut stream);
            continue;
        }
        // 先留一份 fd 副本：`spawn` 失败时 `stream` 已经被 move 进闭包，没有副本就只能看着它 drop 成一个没有任何 HTTP 响应的 FIN。
        // `try_clone` 是一次 `dup`，成功那条路上它立刻 drop（关 dup 出来的 fd 不关 socket）。
        let spare = stream.try_clone().ok();
        let relay = Arc::clone(&relay);
        inflight.fetch_add(1, SeqCst);
        let mine = Arc::clone(&inflight);
        // 每连接一个线程。与参考实现同形（它用 ThreadingHTTPServer）。
        let spawned = std::thread::Builder::new()
            .name("ccm-relay-conn".to_string())
            .spawn(move || {
                if let Err(e) = comms_outward::serve_one(stream, &relay) {
                    // 只印错误本身，永不印请求头。
                    eprintln!("[relay] connection ended: {e}");
                }
                mine.fetch_sub(1, SeqCst);
            });
        if let Err(e) = spawned {
            inflight.fetch_sub(1, SeqCst);
            eprintln!("[relay] cannot spawn connection thread: {e}");
            if let Some(mut s) = spare {
                let _ = comms_outward::refuse_busy(&mut s);
            }
        }
    }
}

/// 起中转那一段：上游选择认启动配置 → 绑回环 → 报地址 → 装表 → 造 `Relay`。失败时**该说的那一句已经说了**。
/// 先前与独立 `--relay` 那一形共用；那一形删了，只剩 [`host`] 一个调用方。
fn prepare(
    port: u16,
    get: &dyn Fn(&str) -> Option<String>,
    startup: &dyn Startup,
    tee: TeeSink,
) -> Result<(TcpListener, Arc<Relay>), String> {
    // 上游选择认不出时这里拿不到「为什么」：这一支只印一句 `[relay] bad upstream base url`（改那句要同拍改
    // `creds_guard::LOG_SITES`）；每一行账号的 `base_url` 为什么认不出，装表时已经印出去了。
    let Some(ready) = startup.check(get) else {
        eprintln!("[relay] bad upstream base url");
        return Err(copy_text("beRelayListen.prepare.badUpstream", &[]));
    };
    let listener = match listen(port) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[relay] cannot bind loopback port {port}: {e}");
            // 这一句只进日志（`Hosted::Failed`）：原因词 ＋ 原话一起记。
            return Err(crate::common::said::Said::with_raw(
                copy_text(
                    "beRelayListen.prepare.bindFailed",
                    &[
                        ("port", &port.to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                &e,
            )
            .logged());
        }
    };
    // 绑上口之后、说「在听」之前拿钥匙（读回，或铸一把落盘；`INVARIANTS §48.1a`）：
    // ① 只有绑上了口的那一个会写 ⇒ 两个中转抢着铸构造上不存在；
    // ② 「在听」那句话说出去的时候钥匙文件已经在盘上。
    // 拿不到 ⇒ 不起（有口没钥匙 = 不设防的口；`listener` 在这里 drop，口当场放掉）。报错里只有路径与原因，没有钥匙值（`door::Key` 不派生 `Debug`）。
    let ensure = |kind| {
        key::key_path(get, kind)
            .ok_or_else(|| {
                crate::common::said::Said::from(copy_text("beRelayListen.key.noHome", &[]))
            })
            .and_then(|p| key::ensure_key(&p))
    };
    let door = match ensure(key::KeyKind::Full)
        .and_then(|full| ensure(key::KeyKind::Pass).map(|pass| comms_outward::Keys { full, pass }))
    {
        Ok(k) => k,
        Err(e) => {
            let said = e.said.clone();
            let e = e.logged();
            eprintln!("[relay] refusing to listen without a relay key: {e}");
            return Err(copy_text(
                "beRelayListen.key.unavailable",
                &[("why", &said)],
            ));
        }
    };
    match listener.local_addr() {
        Ok(a) => eprintln!("[relay] listening on {a}"),
        Err(e) => eprintln!("[relay] listening (addr unknown: {e})"),
    }
    // 顺序：起监听之后、进接受循环之前 —— 放在前面的话，端口起不来那条支会先把还不相干的凭据路径印出来。
    let dest = ready.into_destinations(get, &mut std::io::stderr());
    let relay = Relay::new(dest, door, tee, DOWNSTREAM_DEADLINE, UPSTREAM_DEADLINE);
    Ok((listener, Arc::new(relay)))
}

/// 常驻后端里**进程内**起中转的结局。
///
/// 中转只是常驻后端的一个面 ⇒ 起不来**出声、不拖垮后端**（持有全部 SSH 的那个进程
/// 不许因为「端口被占」或「上游配置认不出」而倒下）。
#[derive(Debug)]
pub(crate) enum Hosted {
    /// 没被交端口 ⇒ 这个进程**不开**中转（测试连接探针 exec 的那一趟就是这一格）。
    NotAsked,
    /// 在听：回环 ＋ 这个地址。
    Listening(SocketAddr),
    /// 交了端口却起不来。串是给人看的那句「为什么」（`[relay]` 前缀那一句已印过）。
    Failed(String),
}

impl std::fmt::Display for Hosted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Hosted::NotAsked => f.write_str("[relay] 没被交中转端口 ⇒ 本进程不开中转"),
            Hosted::Listening(a) => write!(f, "[relay] 中转住本进程，听 {a}"),
            Hosted::Failed(reason) => write!(
                f,
                "[relay] 被交了中转端口却起不来（后端照常服务）：{reason}"
            ),
        }
    }
}

/// 本进程里**绑上了、接受线程也起来了**的中转口。唯一写者是 [`host`]（生产里一个进程至多一个；
/// 判据在同一个测试进程里各起各的口 ⇒ 按口记，互不干扰）。接受线程随进程生死、不中途退 ⇒ 记下就不摘。
static HOSTED_PORTS: std::sync::Mutex<std::collections::BTreeSet<u16>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

fn note_listening(port: u16) {
    HOSTED_PORTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(port);
}

/// **这个进程里我们的中转在不在听这个口** —— 读宿主自己那份监听状态，不从外面探自己
/// （中转就住在这个进程里，；一个事实一个家）。读者：上游选择出的两份成品
/// （`apikey-routing` 的 `running` · `relay-optin` 的 `listening` · 别名预览）。没起中转的进程（一次性 exec · 测试连接探针）恒答 `false`。
pub(crate) fn our_relay_listening(port: u16) -> bool {
    HOSTED_PORTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&port)
}

/// 在本进程里起中转：交了端口（[`ENV_PORT`]）才起，接受循环跑在一条专属线程上。
///
/// 起常驻后端的宿主交端口（本机 monitor · 远端 `--resident-ensure`）；测试连接探针 exec 的那一趟流模式没人交 ⇒ 不起（它读完 hello 就退）。
/// ① 端口没有缺省值（交了一个认不出的串 ⇒ `Failed`，不悄悄退回默认值：注入侧拼的是它交出来的那个数）；
/// ② tee 落宿主交下来的 tap 口（[`TeeSink::to_port`]）：本进程的 stdout 在 stdio 载体上就是 wire，宿主把事件转成 `tap` 帧走它自己的有界通道；
/// ③ 起不来不退出（见 [`Hosted`]）。
pub(crate) fn host(
    get: &dyn Fn(&str) -> Option<String>,
    startup: &dyn Startup,
    tap: std::sync::Arc<dyn comms_outward::TapPort>,
) -> Hosted {
    let Some(raw) = get(ENV_PORT).filter(|s| !s.trim().is_empty()) else {
        return Hosted::NotAsked;
    };
    let port = match raw.trim().parse::<u16>() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[relay] not hosted: CCM_RELAY_PORT is not a port number: {e}");
            return Hosted::Failed(format!("{ENV_PORT}={raw:?} 不是端口号（{e}）"));
        }
    };
    let (listener, relay) = match prepare(port, get, startup, TeeSink::to_port(tap)) {
        Ok(x) => x,
        Err(why) => return Hosted::Failed(why),
    };
    let addr = match listener.local_addr() {
        Ok(a) => a,
        Err(e) => return Hosted::Failed(format!("读不出绑到的地址：{e}")),
    };
    // 接受循环**阻塞在 `accept()` 上**（内核事件，不是定时器）；线程随进程生、随进程死 ——
    // 常驻后端按「退出行为」退的那一刻，中转一起走。
    match std::thread::Builder::new()
        .name("ccm-relay-accept".to_string())
        .spawn(move || serve(listener, relay, Arc::default()))
    {
        Ok(_) => {
            note_listening(addr.port());
            Hosted::Listening(addr)
        }
        Err(e) => {
            eprintln!("[relay] cannot spawn accept thread: {e}");
            Hosted::Failed(format!("起不来接受线程：{e}"))
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/relay/host_tests.rs"]
mod host_tests; // 进程内中转：`host` 的四种结局 ＋ 生产接线在真子进程里 stdout 零 tee
