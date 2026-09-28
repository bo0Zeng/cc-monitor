//! 中转 · **监听面**：绑回环 · accept · 在途上界 · 起监听之前那点接线。
//!
//! # 它从哪儿来（`设计/20 §4`：`server.rs` 4506 行按职责拆）
//!
//! 规格那张表把 `server.rs` 拆成三份，本文件是其中一份，逐字：
//! 「`relay/listen.rs`  🔴 **要劈两半**：listen / accept / `INFLIGHT_CONNECTIONS` → 后端侧
//! （`01 C5`）；serve / `apply_downstream_deadline` → 中转」。
//! 〔NET2〕在途上界 `INFLIGHT_CONNECTIONS` 与在途计数今天住本文件（`设计/20 §10` 第 7 条）。
//!
//! ⚠⚠ **那「两半」本拍只劈了一半，另一半劈不动，理由现打**：
//! 把 bind/accept 挪到**后端侧**要在 `relay/` 之外新开一个模块，而本拍的写区
//! 逐字是「`src/backend/relay/` 及它下面新建的目录」。⇒ 本文件今天**两半都在**，
//! 边界用注释标着；真正的搬家归 `99 §4` 的步 **13c**（「后端 `bind`/`listen`，
//! 把 `accept` 到的连接交给面 B」）—— 那一步自己就写着「它不挡 14」。
//!
//! # ⚠ 有两样东西**职责在这里、代码还在 `server.rs`** —— 逐条给现打的理由
//!
//! 它们**不是**按职责留在那儿的，是被**写区外的住址登记**钉住的：搬一步就当场红，
//! 而那几处登记都不在本拍的写区里。如实列，别读成设计要它们分开：
//!
//! | 留在 `server.rs` 的 | 钉住它的登记（都在写区外） |
//! |---|---|
//! | `DOWNSTREAM_DEADLINE` ＋ `apply_downstream_deadline` | 〔`P16` 订正〕那个**值**今天住本文件，`REGISTERED_DURATION_USES` 那两行的住址栏逐字 `"listen.rs"`；装它的那一手仍在 `server.rs`（改成收入参） |
//! | `LOOPBACK` | 同上，钉它的是 **`src/backend/listen.rs`**（K-P1 那个常驻监听口，与本文件同名但是另一棵）那句「理由与 `…/relay/server.rs::LOOPBACK` 逐字同源」 |
//!
//! ⇒ 本文件 `use` 它们，注释里点符号（不点文件）。真要把它们挪过来，得与
//! `src/bridge/` 那两句散文 ＋ `no_timer_guard` 那张表**同拍**改。

use super::server::{self, Relay, LOOPBACK};
use super::{door, tee::TeeSink, Startup};
use copy_core::copy_text;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::Arc;

/// 同时在途的下游连接数上限〔回修轮之五 08-25，D3 `阻-3(D3)` 的**做得到的那一半**；NET2 从 `server.rs` 挪来〕。
///
/// 住宿主不住中转：上界与绑口同是策略值（`设计/01 §2.1 C5` · `20 §4`）；中转只剩 `handle`。
/// ⚠ **是条数不是体量**，名字里刻意不带 `MAX`/`CAP`/`LIMIT`/`BYTES`（`byte_cap_registry` 的钩子）。
/// 超了 **回 `503` 并关连接**，不是静默 FIN。
pub(super) const INFLIGHT_CONNECTIONS: usize = 256;

pub(crate) const ENV_PORT: &str = "CCM_RELAY_PORT";

// ══════════════════════════════════════════════════════════════════════════
//  期限的**值**住这一层〔`设计/99 §4 P16`，2026-09-22〕
// ══════════════════════════════════════════════════════════════════════════
//
// `设计/01 §2.1` 的 `C4` 逐字：「凭据、配置、路由表、**期限值**全部由后端**交给它**
// ⇒ 期限（超时）与端口号同是**策略值**……通信层自己**没有任何期限常量**」。
// `设计/05 §3.3.2` 把分工写成三段：**值归后端 · 执行归通信层 · 说法归调用方**。
//
// 🔴 **这两个值先前住中转**（`server.rs` 与 `upstream.rs`）—— 那正是 `P16` 完成判据里
// 点名的「两个期限常量（`X2`）」。搬来这里的理由不是品味：本文件自己就是**监听面**，
// 按 `C5` 括号里那条「端口号是策略值，从配置来 ⇒ 按 `C4` 本就归后端」它**语义上就在
// 边界外**（`comm_boundary_registry` 那张表里它逐字登记为「这一份**语义上就该在外面**，
// 不是等它变干净」）。⇒ 期限值与端口号是同一类东西，它们该住同一层。
//
// ⚠ **搬走的只有值**：装它们的那两手还在中转（`server::apply_downstream_deadline`
// 与 `upstream::connect`），只是改成收入参 —— **执行仍归通信层**，这一条没变。
// ⚠ 这两条在 `no_timer_guard::REGISTERED_DURATION_USES` 里各占一行，住址栏跟着改成
// 本文件。那张表是**恰好相等**的断言 ⇒ 住址改错/漏改当场红。
// ⚠ 本段刻意**不写出通信层那枚成员标记的字面**：它是**裸子串扫描、不剥注释**
// （与 `relay/mod.rs ㈠` 那一节记的只读护栏同一族坑）—— 在散文里提它一次，
// 这份文件当场就「自称成员」了。本拍第一版正是这么红的，逐字
// 「这几份文件**自称**通信层成员，却不在登记表里」。**要改就改措辞。**
// ⚠ **不许把值搬回中转**：`upstream.rs` 今天盖着那枚成员标记，`X2`（生产段
// 零期限字面量）对它当场成立；搬回去它立刻掉出边界。

/// **下游**那条 socket 的读写期限〔回修轮之六 08-25，D3 `阻-3(D3)` 的**后半段**〕。
///
/// # 它治的是什么（别读成「限流已经够了」）
///
/// `INFLIGHT_CONNECTIONS` 买的是「**顶不满、拒绝有声**」；这一条买的是
/// 「**顶住的那些会自己散**」。没有它，256 条半开连接能把中转**永久**钉死，
/// 而它会礼貌地回 503 —— **那一屏读起来像正常限流**。
/// （D3 实测：64 条半开 ⇒ 线程 4 → 68，全卡在 `read_head` 的 `r.read(&mut one)?` 上永不返回。
///  **这个数我没重打，住址 `audits/K-H1-D3.md` §2.3**。）
///
/// # 它不是定时器 —— 这句话就是 `no_timer_guard::REGISTERED_DURATION_USES` 里登记的那一行
///
/// ⚠ 〔`P16` 2026-09-22〕那张表里这一行的住址栏从 `"server.rs"` 改成了 `"listen.rs"` ——
/// **值搬了、性质没变**。装它的那一手仍在 `server::apply_downstream_deadline`。
///
/// `SO_RCVTIMEO` / `SO_SNDTIMEO` 说的是「**这一次**阻塞的读/写最多等多久」：
/// 有字节就**立刻**返回，没字节就**报错**返回。它不让任何线程**自己醒来**、不产生任何节拍。
/// ⭐ 配套硬约束：**期限到了就把这条连接结掉，任何一层都不许重试** ——
/// 一重试它就从「阻塞有上限」变成「轮询」，而轮询正是零定时器护栏要防的东西。
/// 今天靠的是：`pump` 与 `http1` 的读循环**只**对 `Interrupted`（EINTR）`continue`，其余一律 `return Err`。
///
/// # 值为什么是 30 秒（分母写在这里）
///
/// 对端**就在本机** —— `listen()` 绑的是 `LOOPBACK` 常量，`DoD-4㈡` 那条判据钉着它。
/// 分母是「回环上搬完一条**最大**请求体要多久」：`BODY_CAP` = 64 MiB，回环带宽是 GB/s 量级
/// ⇒ 零点零几秒。30 秒比它高**两到三个量级**。
/// ⚙ 那个「零点零几秒」是**按量级推的，我没实测本机回环吞吐** ⇒ 数量级论证，不是读数。
///
/// ⚙ **设错会怎样**（两个方向都坏，坏法不同）：
/// - **设长**（比如照抄上游那 600 秒）：半开连接确实会自己散，但要散 10 分钟
///   ⇒ 上界从 ∞ 降到 600 秒是真收益，但那个数**读起来仍像挂死**。
/// - **设短**（比如 1 秒）：一台负载高的机器上，一条**合法**的大请求体会被中转自己掐掉，
///   客户端看到「网络错误」而中转日志上是一条正常的连接结束 ⇒ **打断正常流量、且不好查**。
///
/// # 为什么**不**跟上游用同一个数
///
/// 「这个对端合法地可以多久不吭声」是**对端的属性**，不是方向的属性。
/// 下游是本机、请求在它内存里已经拼好了 ⇒ 慢是**异常**；
/// 上游是「模型在想」⇒ 慢是**正常**（见 [`UPSTREAM_DEADLINE`]）。
pub(super) const DOWNSTREAM_DEADLINE: std::time::Duration =
    std::time::Duration::from_millis(30_000);

/// 上游那条 socket 的**读写期限**〔回修轮之六 08-25，D3 `阻-3(D3)` 的**后半段**〕。
///
/// # 它不是定时器（这句话就是 `no_timer_guard` 那张表里登记的那一行）
///
/// ⚠ 〔`P16` 2026-09-22〕那张表里这一行的住址栏从 `"upstream.rs"` 改成了 `"listen.rs"`。
/// 装它的那一手仍在 `upstream::connect`（收入参）。
///
/// `SO_RCVTIMEO` / `SO_SNDTIMEO` 说的是「**这一次**阻塞的读/写最多等多久」：
/// 有字节就**立刻**返回，没字节就**报错**返回。它不会让任何线程**自己醒来**，
/// 也不产生任何节拍 —— 这正是零定时器护栏禁的那一类与它的分界。
/// ⭐ 配套的硬约束：**期限到了就把连接结掉，不允许任何一层重试** ——
/// 一重试它就从「阻塞有上限」变成「轮询」，而轮询正是护栏要防的东西。
/// 今天这一条靠的是：`pump` 与 `http1` 里的读循环**只**对 `Interrupted`（EINTR）`continue`，
/// 其余错误一律 `return Err`；`rustls` 的 `complete_io` 同形（只重试 `Interrupted`）。
///
/// # 值为什么是 600 秒，而不是下游那个数
///
/// 这一跳等的是**模型在想** —— 上游几十秒不发一个字节是 **SSE 长流的正常形态**，
/// 不是卡死。600 秒这个数**不是我拍的**：`super` 头注逐字记着「参考实现给上游 600 秒」，
/// 说的正是同一跳。
///
/// ⚙ **设错会怎样**：把它改小（比如照抄下游那 30 秒）会把一条**正在正常吐字、
/// 只是中间想了 40 秒**的长流从中间提断，客户端拿到半条回答
/// ⇒ **比不设期限更坏**（不设的话那条流是能走完的）。这是本格最贵的一种错。
/// 改大则是：一条死掉但没发 FIN 的上游（NAT/conntrack 丢连接）会多钉住一条线程那么久。
///
/// ⚙ **我刻意不拿「Anthropic 的 SSE 会周期发 `ping`」当依据**：那要打真 API 才量得到，
/// 而 `C7` 逐字禁「绝不起真 claude」⇒ 这个数必须在「上游合法地整段沉默」的前提下也站得住。
///
/// # 它**没**盖住的那一步：`connect` 本身
///
/// 下面 `TcpStream::connect` **没有**连接期限。本轮故意不做：读写是**真无界**
/// （对端不发就永远不返回），而 connect 那一步有内核 SYN 重试上限与解析器自己的上限兜着。
/// ⚙ **那个上限具体多少我没量** ⇒ 只敢说「不是无界」，不敢说「够小」。
pub(super) const UPSTREAM_DEADLINE: std::time::Duration = std::time::Duration::from_millis(600_000);

/// 起监听。返回真实绑定的地址（端口给 0 时由内核选，测试用）。
pub(crate) fn listen(port: u16) -> std::io::Result<TcpListener> {
    let listener = TcpListener::bind(SocketAddr::new(LOOPBACK, port))?;
    Ok(listener)
}

/// 接受循环。**阻塞在 `accept()` 上** —— 那是内核事件，不是定时器。
///
/// # ★ 在途连接数有上界，且拒绝是**出声**的〔回修轮之五 08-25，D3 `阻-3(D3)`〕
///
/// 先前这里是「每连接无条件 spawn」+ `let _ = …spawn(…)`：
/// - **无上界** —— 64 条半开连接（只发半个请求头、永不发结尾空行、不关连接）
///   就钉住 64 条线程（D3 实测 `半开 64 条之前线程 4 · 1.5s 之后线程 68`）；
/// - **spawn 失败被整个吞掉** —— 线程顶满之后 `stream` 当场 drop，下游拿到一个
///   **没有任何 HTTP 响应**的 FIN，而 `serve` 一个字都不印 ⇒ **静默拒绝**，不是 503。
///
/// 今天：超过 `INFLIGHT_CONNECTIONS` 就回 **503** 并关连接；spawn 失败同样回 503 并**出声**。
///
/// # ★★ 另一半也补上了〔回修轮之六 08-26〕：**顶住的那些会自己散**
///
/// ⚠ 订正：这里先前逐字写着「**这只是那条阻塞的一半，另一半我做不到**」——
/// 那句话在回修轮之五是真的（`no_timer_guard.rs` 当时不在写区，交回见件文件 §8.20.4），
/// **PM 收 R5 时扩了写区一格并派了 R6**（§8.21.3），今天它**已经不成立了**。
///
/// 补的是读写期限：`apply_downstream_deadline` 在**两个**调用点装 `DOWNSTREAM_DEADLINE`
/// —— ㈠ 这里，`accept` 出来那一刻（覆盖 503 那条支，它跑在 **accept 线程**上）；
/// ㈡ `handle()` 开头（转发路径真正阻塞的地方，也是 D3 §2.3 逐字点名的住址）。
/// 上游那条 socket 由 `upstream::connect` 装 [`UPSTREAM_DEADLINE`]（本文件交下去的）。
///
/// ⇒ 三样齐了：**顶不满**（上界）· **拒绝有声**（503 而不是静默 FIN）· **顶住的会自己散**（期限）。
///
/// `inflight` 是在途计数（本函数进出各动一次），由调用方交进来 —— 与上界同住宿主这一侧。
pub(crate) fn serve(listener: TcpListener, relay: Arc<Relay>, inflight: Arc<AtomicUsize>) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else {
            continue;
        };
        // 期限**先装上，在分流之前** —— 这一处是**纵深**：不变式是「每条 accept 出来的
        //    socket 从第一刻起就带期限，不管它接下来走哪个分支」。
        //    ⚙ 别把它读成「治了一个实测过的挂死」：下面那条 503 支只写 ~90 字节、
        //    排字节那步又是非阻塞的，我**没构造出**它阻塞的形状（理由全文见
        //    `apply_downstream_deadline` 头注㈡；本轮 `MU6` 实测删掉它**零红**）。
        //    装不上仍然**关连接并出声**：宁可拒绝，也不放一条来路不明的进来。
        if let Err(e) = server::apply_downstream_deadline(&stream, DOWNSTREAM_DEADLINE) {
            eprintln!("[relay] cannot set connection deadline: {e}");
            continue;
        }
        if inflight.load(SeqCst) >= INFLIGHT_CONNECTIONS {
            // ⚠ 只印数字与上限，**永不印请求头**（`K9` 裁定四第 1 条）——
            // 这一支根本还没读过一个字节，连请求头都还不存在。
            eprintln!("[relay] refusing: {INFLIGHT_CONNECTIONS} connections already in flight");
            let _ = server::respond_and_drain(&mut stream, server::BUSY);
            continue;
        }
        // ★ 先留一份 fd 副本：`spawn` 失败时 `stream` 已经被 move 进那个闭包、拿不回来，
        //   没有副本就只能眼看着它 drop 成一个**没有任何 HTTP 响应**的 FIN。
        //   `try_clone` 是一次 `dup`，成功那条路上它立刻 drop（dup 出来的 fd 关掉不关 socket）。
        let spare = stream.try_clone().ok();
        let relay = Arc::clone(&relay);
        inflight.fetch_add(1, SeqCst);
        let mine = Arc::clone(&inflight);
        // 每连接一个线程。与参考实现同形（它用 ThreadingHTTPServer）。
        let spawned = std::thread::Builder::new()
            .name("ccm-relay-conn".to_string())
            .spawn(move || {
                if let Err(e) = server::handle(stream, &relay) {
                    // ⚠ 只印错误本身，**永不印请求头**（`K9` 裁定四第 1 条）。
                    eprintln!("[relay] connection ended: {e}");
                }
                mine.fetch_sub(1, SeqCst);
            });
        if let Err(e) = spawned {
            inflight.fetch_sub(1, SeqCst);
            eprintln!("[relay] cannot spawn connection thread: {e}");
            if let Some(mut s) = spare {
                let _ = server::respond_and_drain(&mut s, server::BUSY);
            }
        }
    }
}

/// 起中转那一段：上游选择认启动配置 → 绑回环 → 报地址 → 装表 → 造 `Relay`。失败时**该说的那一句已经说了**。
/// 〔DEL〕先前与独立 `--relay` 那一形共用；那一形删了，只剩 [`host`] 一个调用方。
fn prepare(
    port: u16,
    get: &dyn Fn(&str) -> Option<String>,
    home: &std::path::Path,
    startup: &dyn Startup,
    tee: TeeSink,
) -> Result<(TcpListener, Arc<Relay>), String> {
    // ⚠ `K-R1`：上游选择认不出时**为什么**认不出，这里拿不到 —— 如实登记为射程外：
    //   这一支只印一句 `[relay] bad upstream base url`，而改那句报文要同拍改
    //   `creds_guard::LOG_SITES` 那张表 ⇒ 另一拍。★ 而**每一行**账号的 `base_url`
    //   那句为什么，今天是真的印出去了（上游选择装表时）。
    let Some(ready) = startup.check(get) else {
        eprintln!("[relay] bad upstream base url");
        return Err(copy_text("beRelayListen.prepare.badUpstream", &[]));
    };
    let listener = match listen(port) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[relay] cannot bind loopback port {port}: {e}");
            return Err(copy_text(
                "beRelayListen.prepare.bindFailed",
                &[("port", &port.to_string()), ("e", &e.to_string())],
            ));
        }
    };
    // 〔RK1 · `INVARIANTS §48.1a`〕**绑上口之后、说「在听」之前**拿钥匙（读回，或铸一把落盘）：
    //   ① 只有绑上了口的那一个会写 ⇒ 两个中转抢着铸构造上不存在；
    //   ② 「在听」那句话说出去的时候钥匙文件已经在盘上 ⇒ 看着那句话去读钥匙的人（判据 · 那台机器的 shell）读得到。
    //   拿不到 ⇒ **不起**（有口没钥匙 = 不设防的口；`listener` 在这里 drop，口当场放掉）。
    //   ⚠ 报错里只有路径与原因，**永远没有钥匙值**（`door::Key` 不派生 `Debug`）。
    let door = match door::key_path(get)
        .ok_or_else(|| copy_text("beRelayListen.key.noHome", &[]))
        .and_then(|p| door::ensure_key(&p))
    {
        Ok(k) => k,
        Err(e) => {
            eprintln!("[relay] refusing to listen without a relay key: {e}");
            return Err(copy_text(
                "beRelayListen.key.unavailable",
                &[("e", &e.to_string())],
            ));
        }
    };
    match listener.local_addr() {
        Ok(a) => eprintln!("[relay] listening on {a}"),
        Err(e) => eprintln!("[relay] listening (addr unknown: {e})"),
    }
    // ⚠ 顺序：**起监听之后、进接受循环之前**。放在起监听之前的话，
    //   端口起不来那条支会先把凭据路径印出来，而那时它还不相干。
    let dest = ready.into_destinations(get, home, &mut std::io::stderr());
    let relay = Relay::new(dest, door, tee, DOWNSTREAM_DEADLINE, UPSTREAM_DEADLINE);
    Ok((listener, Arc::new(relay)))
}

/// 〔RL1 · V107〕常驻后端里**进程内**起中转的结局。
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

/// 〔RL1 · V107 · V139〕**在本进程里起中转**：交了端口（[`ENV_PORT`]）才起，接受循环跑在一条专属线程上。
///
/// # 为什么是「交了端口才起」而不是「流模式一律起」
///
/// 起常驻后端的宿主交端口（本机 monitor · 远端 `--resident-ensure`）；测试连接探针 exec 的那一趟流模式没人交 ⇒ 不起
/// （它读完 hello 就退，不该去抢那个口）。
///
/// # 形状
///
/// ① 端口**没有缺省值**（交了一个认不出的串 ⇒ `Failed`，不悄悄退回一个默认值 ——
/// 注入侧拼的是它交出来的那个数，两边对不上就是一个查不出来的连接失败）；
/// ② tee 落宿主交下来的 tap 口（[`TeeSink::to_port`]，〔TAP · V124〕）：本进程的 stdout 在 stdio 载体上**就是 wire**
/// （一行一帧），在脱离载体上是 null；宿主把事件转成 `tap` 帧走它自己的有界通道；
/// ③ 起不来不退出（见 [`Hosted`]）。
pub(crate) fn host(
    get: &dyn Fn(&str) -> Option<String>,
    home: &std::path::Path,
    startup: &dyn Startup,
    tap: std::sync::Arc<dyn super::tee::TapPort>,
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
    let (listener, relay) = match prepare(port, get, home, startup, TeeSink::to_port(tap)) {
        Ok(x) => x,
        Err(why) => return Hosted::Failed(why),
    };
    let addr = match listener.local_addr() {
        Ok(a) => a,
        Err(e) => return Hosted::Failed(format!("读不出绑到的地址：{e}")),
    };
    // 接受循环**阻塞在 `accept()` 上**（内核事件，不是定时器）；线程随进程生、随进程死 ——
    // 常驻后端按「退出行为」退的那一刻，中转一起走（`设计/01 §3.3b`，V107）。
    match std::thread::Builder::new()
        .name("ccm-relay-accept".to_string())
        .spawn(move || serve(listener, relay, Arc::default()))
    {
        Ok(_) => Hosted::Listening(addr),
        Err(e) => {
            eprintln!("[relay] cannot spawn accept thread: {e}");
            Hosted::Failed(format!("起不来接受线程：{e}"))
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/relay/host_tests.rs"]
mod host_tests; // 〔RL1〕进程内中转：`host` 的四种结局 ＋ 生产接线在真子进程里 stdout 零 tee
