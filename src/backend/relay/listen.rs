//! 层 1 · **监听面**：绑回环 · accept · 在途上界 · 起监听之前那点接线。
//!
//! # 它从哪儿来（`设计/20 §4`：`server.rs` 4506 行按职责拆）
//!
//! 规格那张表把 `server.rs` 拆成三份，本文件是其中一份，逐字：
//! 「`relay/listen.rs`  🔴 **要劈两半**：listen / accept / `INFLIGHT_CONNECTIONS` → 后端侧
//! （`01 C5`）；serve / `apply_downstream_deadline` → 层 1」。
//!
//! ⚠⚠ **那「两半」本拍只劈了一半，另一半劈不动，理由现打**：
//! 把 bind/accept 挪到**后端侧**要在 `relay/` 之外新开一个模块，而本拍的写区
//! 逐字是「`src/backend/relay/` 及它下面新建的目录」。⇒ 本文件今天**两半都在**，
//! 边界用注释标着；真正的搬家归 `99 §4` 的步 **13c**（「后端 `bind`/`listen`，
//! 把 `accept` 到的连接交给面 B」）—— 那一步自己就写着「它不挡 14」。
//!
//! # ⚠ 有三样东西**职责在这里、代码还在 `server.rs`** —— 逐条给现打的理由
//!
//! 它们**不是**按职责留在那儿的，是被**写区外的住址登记**钉住的：搬一步就当场红，
//! 而那几处登记都不在本拍的写区里。如实列，别读成设计要它们分开：
//!
//! | 留在 `server.rs` 的 | 钉住它的登记（都在写区外） |
//! |---|---|
//! | `DOWNSTREAM_DEADLINE` ＋ `apply_downstream_deadline` | `tests/backend/no_timer_guard.rs::REGISTERED_DURATION_USES` 那一行的住址栏逐字 `"server.rs"`（配 `Duration::from_millis(30_000)`），按文件名后缀匹配 |
//! | `DEFAULT_PORT` | `src/bridge/src/backend/control/payload.rs` 的散文逐字点着 `src/backend/relay/server.rs::DEFAULT_PORT`，而 `structural_scan::every_symbol_address_in_the_sources_still_resolves` **真的判得了那条住址**（现打：搬走之后它当场红，诊断逐字「符号还在，但**搬家了**」） |
//! | `INFLIGHT_CONNECTIONS` | 同上，钉它的是 `src/bridge/src/local_backend_host.rs` 那句散文 |
//! | `LOOPBACK` | 同上，钉它的是 **`src/backend/listen.rs`**（K-P1 那个常驻监听口，与本文件同名但是另一棵）那句「理由与 `…/relay/server.rs::LOOPBACK` 逐字同源」 |
//!
//! ⇒ 本文件 `use` 它们，注释里点符号（不点文件）。真要把它们挪过来，得与
//! `src/bridge/` 那两句散文 ＋ `no_timer_guard` 那张表**同拍**改。

use super::server::{self, Relay, DEFAULT_PORT, INFLIGHT_CONNECTIONS, LOOPBACK};
use super::{accounts, tee::TeeSink, upstream::Base};
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;

pub(super) const ENV_PORT: &str = "CCM_RELAY_PORT";

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
/// 上游那条 socket 由 `upstream::connect` 装 `upstream::UPSTREAM_DEADLINE`。
///
/// ⇒ 三样齐了：**顶不满**（上界）· **拒绝有声**（503 而不是静默 FIN）· **顶住的会自己散**（期限）。
pub(crate) fn serve(listener: TcpListener, relay: Arc<Relay>) {
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
        if let Err(e) = server::apply_downstream_deadline(&stream) {
            eprintln!("[relay] cannot set connection deadline: {e}");
            continue;
        }
        if relay.inflight().load(SeqCst) >= INFLIGHT_CONNECTIONS {
            // ⚠ 只印数字与上限，**永不印请求头**（`K9` 裁定四第 1 条）——
            // 这一支根本还没读过一个字节，连请求头都还不存在。
            eprintln!("[relay] refusing: {INFLIGHT_CONNECTIONS} connections already in flight");
            let _ = server::respond_and_drain(&mut stream, "503 Service Unavailable");
            continue;
        }
        // ★ 先留一份 fd 副本：`spawn` 失败时 `stream` 已经被 move 进那个闭包、拿不回来，
        //   没有副本就只能眼看着它 drop 成一个**没有任何 HTTP 响应**的 FIN。
        //   `try_clone` 是一次 `dup`，成功那条路上它立刻 drop（dup 出来的 fd 关掉不关 socket）。
        let spare = stream.try_clone().ok();
        let relay = Arc::clone(&relay);
        let inflight = Arc::clone(relay.inflight());
        inflight.fetch_add(1, SeqCst);
        // 每连接一个线程。与参考实现同形（它用 ThreadingHTTPServer）。
        let spawned = std::thread::Builder::new()
            .name("ccm-relay-conn".to_string())
            .spawn(move || {
                if let Err(e) = server::handle(stream, &relay) {
                    // ⚠ 只印错误本身，**永不印请求头**（`K9` 裁定四第 1 条）。
                    eprintln!("[relay] connection ended: {e}");
                }
                relay.inflight().fetch_sub(1, SeqCst);
            });
        if let Err(e) = spawned {
            inflight.fetch_sub(1, SeqCst);
            eprintln!("[relay] cannot spawn connection thread: {e}");
            if let Some(mut s) = spare {
                let _ = server::respond_and_drain(&mut s, "503 Service Unavailable");
            }
        }
    }
}

/// `--relay` 的**配置面** —— 纯函数：不读环境、不起监听、不碰网络。
///
/// ★ 它为什么被抽出来（回修轮 08-25，D1 `重要-6`）：先前这一段整个长在 `run()` 里，
/// 而 `run()` 尾巴上是**永不返回**的 `serve()` ⇒ 没有任何判据调得动它。
/// 实测：把 `run()` 的函数体整个换成 `2`，384 条判据**全绿**（审计 `CG1`）——
/// `CCM_RELAY_PORT`/`CCM_RELAY_UPSTREAM` 的解析、两个默认值，**一样都没被量过**。
pub(super) fn resolve_config(
    port_env: Option<&str>,
    upstream_env: Option<&str>,
) -> Option<(u16, Base)> {
    let port = port_env
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    // ⚠ `K-R1`：`Base::parse` 现在带着**一句为什么**回来，而这里把它丢掉了
    //   —— 如实登记为射程外，不是漏掉：这一支的调用方（`run_with`）只印一句
    //   `[relay] bad upstream base url` 就退 2，而那条路上**还没有任何日志出口**能带这句话。
    //   真要带上，改的是 `run_with` 的报文与 `creds_guard::LOG_SITES` 那张表 ⇒ 另一拍。
    //   ★ 而**每一行**账号的 `base_url` 那句为什么，今天是真的印出去了（`table::build`）。
    //
    // ⚠⚠ **上游那一半住层 2 了**（`20 §4`「常量跟着职责走」）：`ENV_UPSTREAM` 与
    //    `DEFAULT_UPSTREAM` 都在 `accounts/`，本函数只是把环境里那个串**递过去**，
    //    自己**认不出**默认值是什么。⇒ 层 1 里没有任何可以回落的默认上游。
    let base = accounts::upstream_default(upstream_env)?;
    Some((port, base))
}

/// `run()` 剥掉「读环境变量」之后的那一半。
///
/// **起监听之前的处置全在这里** ⇒ 判据打得到「基址不认识就退 2」与
/// 「端口起不来就退出并出声」（`:16-17` 头注承诺的那条）两条。
/// 成功那一条尾巴上是永不返回的 `serve()` ⇒ 判据够不到，登记为 `判不了`。
///
/// ⚠ 「读一次凭据、装表、把该说的话说出去」那一段**搬去层 2 了**
/// （`accounts::load_credentials`）—— 那三件事一件都不属于搬字节这一层。
pub(super) fn run_with(
    port_env: Option<&str>,
    upstream_env: Option<&str>,
    creds_get: &dyn Fn(&str) -> Option<String>,
    home: &std::path::Path,
) -> i32 {
    let Some((port, base)) = resolve_config(port_env, upstream_env) else {
        eprintln!("[relay] bad upstream base url");
        return 2;
    };
    let listener = match listen(port) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[relay] cannot bind loopback port {port}: {e}");
            return 2;
        }
    };
    match listener.local_addr() {
        Ok(a) => eprintln!("[relay] listening on {a}"),
        Err(e) => eprintln!("[relay] listening (addr unknown: {e})"),
    }
    // ⚠ 顺序：**起监听之后、进接受循环之前**。放在起监听之前的话，
    //   端口起不来那条支会先把凭据路径印出来，而那时它还不相干。
    let (table, creds_path, stamp) =
        accounts::load_credentials(creds_get, home, &base, &mut std::io::stderr());
    // `D1 阻-2`：把重载源接上 —— 没有这一行，那张表就是一张**启动快照**，
    // 用户在界面上配完 key 必须重启中转才生效（而不重启的症状是一个静默的 404）。
    let dest = accounts::Accounts::new(table).reloading_from(accounts::Reload::new(
        creds_path,
        base.clone(),
        stamp,
    ));
    let relay = Relay::new(Arc::new(dest), TeeSink::to_stdout());
    serve(listener, Arc::new(relay));
    0
}

/// `run()` 的**接线面**：哪个环境变量喂给哪个配置位。取值器与执行体都是**注入的**
/// ⇒ 判据打得到这条接线本身，而**不必去改进程环境**（`std::env::set_var` 与并行跑的
/// 别的判据是竞态 —— 那不是判据该有的形状）。
///
/// ★ 它为什么被抽出来（回修轮之四 08-25，D2 `重要-3(D2)`）：
/// `重要-6` 那一轮把 `resolve_config`（纯函数）与 `run_with`（退 2 两条）抽了出来，
/// **最外面那一层 `run()` 自己仍然零判据**。实测把那两行 `std::env::var(...)` **对调**，
/// **389 条判据全绿**（D2 `D2RUN`），而真机后果是 `--relay` **整个起不来**：
/// 端口读不懂 ⇒ 回默认 8788、上游解析失败 ⇒ 退 2。
/// 判据见 `each_env_var_name_goes_into_its_own_config_slot`。
pub(super) type RelayExec<'a> = dyn Fn(Option<&str>, Option<&str>, &dyn Fn(&str) -> Option<String>, &std::path::Path) -> i32
    + 'a;

pub(super) fn run_reading(
    get: &dyn Fn(&str) -> Option<String>,
    home: &std::path::Path,
    exec: &RelayExec<'_>,
) -> i32 {
    let port = get(ENV_PORT);
    let upstream = get(accounts::ENV_UPSTREAM);
    // ⚠ `get` 原样往下传：凭据那条路的取值器**必须与端口/上游是同一个**，
    //   否则判据喂进去的环境和生产段读的环境是两套（那正是「量具的作用域对不上事实」）。
    exec(port.as_deref(), upstream.as_deref(), get, home)
}

/// `--relay` 的入口。配置面只有环境变量（backend 今天没有配置文件面）。
///
/// 本函数今天**只剩一件事**：把「真取值器」与 `run_with` 接上。接线本身（哪个变量
/// 喂给哪个位）住 `run_reading`，那里有判据钉着。**别往里加逻辑**：加进来的就又没判据了
/// —— 本函数这一行今天是**判不了**的那一格，登记住址件文件 §8.18.3。
pub fn run(home: &std::path::Path, _args: &[String]) -> i32 {
    run_reading(&|k| std::env::var(k).ok(), home, &run_with)
}
