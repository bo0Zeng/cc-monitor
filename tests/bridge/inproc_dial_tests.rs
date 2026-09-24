//! 〔C2 · `设计/05 §13.5`〕**界面进程里最后一份 russh 拨号**（`inproc_dial.rs`，只剩 SFTP 用）的判据。
//!
//! 原样搬自 `ssh_source_tier1_tests.rs`（host key 校验三分支 · 竞速编排 · 阶段事件）——
//! 被测的代码从 `ssh_source.rs` 原样搬进了 `inproc_dial.rs`，判据跟着搬，一个断言没改。
//! 拨号代理（后端 `dial/`）那一份的判据住 `tests/backend/dial_tests.rs` ＋ 读数 `tests/evidence/C2-dial-loopback.py`。
//! 🔴 `F7c` 把 SFTP 换走那天，本文件随 `inproc_dial.rs` 一起删。

use super::*;
use crate::ssh_source::Endpoint;

fn ep(host: &str, port: u16) -> Endpoint {
    Endpoint {
        host: host.to_string(),
        port,
    }
}
// F43：check_server_key 三分支——匹配接受 / 失配拒绝 / 无期望指纹 TOFU 接受，
// 且无论哪支都把实际指纹写回 observed cell（测试连接据此展示 + 固化）。
use russh::client::Handler as _; // check_server_key 是 trait 方法

fn handler_with(expected: Option<&str>) -> (ClientHandler, Arc<Mutex<Option<String>>>) {
    let observed = Arc::new(Mutex::new(None));
    let h = ClientHandler {
        expected_fingerprint: expected.map(String::from),
        observed_fingerprint: Arc::clone(&observed),
        stage_emitter: None,
        endpoint: None,
    };
    (h, observed)
}

/// 固定 ed25519 公钥 + 其 SHA256 指纹（ssh-keygen 一次性生成后固化进测试，
/// SAMPLE_FP 可用 `ssh-keygen -lf` 对 SAMPLE_PUB 独立复算核对）。
const SAMPLE_PUB: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEwdsNpXeLF3bjmkjNIpFsbGCxLntS8RsfA6BPOv/Ykv f43";
const SAMPLE_FP: &str = "SHA256:fPFSH7moeRu2I96lFjdo8lO2iB7KgVLtL4LXvHVZWDk";

fn sample_key() -> PublicKey {
    PublicKey::from_openssh(SAMPLE_PUB).expect("parse sample pubkey")
}

#[tokio::test]
async fn check_server_key_matching_fingerprint_accepts() {
    let (mut h, observed) = handler_with(Some(SAMPLE_FP));
    assert!(h.check_server_key(&sample_key()).await.unwrap());
    assert_eq!(observed.lock().unwrap().as_deref(), Some(SAMPLE_FP));
}

#[tokio::test]
async fn check_server_key_matching_tolerates_trailing_whitespace() {
    let padded = format!("{SAMPLE_FP}\n  ");
    let (mut h, _observed) = handler_with(Some(&padded));
    assert!(
        h.check_server_key(&sample_key()).await.unwrap(),
        "尾随空白不应误判 MITM"
    );
}

#[tokio::test]
async fn check_server_key_mismatch_rejects_but_records() {
    let (mut h, observed) = handler_with(Some("SHA256:deadbeefwrongfingerprintvalueAAAAAAAAAAAA"));
    assert!(
        !h.check_server_key(&sample_key()).await.unwrap(),
        "失配必须拒绝"
    );
    // 失配也把实际指纹写回 cell（供「重置为 TOFU / 重新固化」）。
    assert_eq!(observed.lock().unwrap().as_deref(), Some(SAMPLE_FP));
}

#[tokio::test]
async fn check_server_key_no_expected_tofu_accepts() {
    let (mut h, observed) = handler_with(None);
    assert!(
        h.check_server_key(&sample_key()).await.unwrap(),
        "TOFU 首连接受"
    );
    assert_eq!(observed.lock().unwrap().as_deref(), Some(SAMPLE_FP));
}

// === F45：race_connect 编排（可控 mock，不依赖真 SSH）===
// 用一个内存 TCP listener 模拟「快地址」（accept 即断=握手必失败但 TCP 连得上），
// 及不存在端口模拟「立即拒绝」；**本地静默对端**（`spawn_silent_peer`）模拟握手挂起。
// 断言编排语义：首个可用者决定结果、全失败聚合、看门狗生效。注：这些测走 race_connect
// 的错误路径（无真 SSH server 故握手都失败），验证的是编排（顺序/聚合/超时/取消），
// 非握手成功路径。
//
// 🔴 **本段的前提纪律**〔`K-R24` 09-04〕：这几条判据要的对端一律**自己起在 loopback 上**。
// 不许再靠「这台机器到某个外部地址是什么反应」—— 那是一条**没人建立、也没人检查**的
// 环境前提，前提不成立时它吐的红与「被测的东西真坏了」**长得一模一样**。
// ⚠ 判据不是靠一张「禁用哪些 IP」的词表守的（那种表迟早腐）：守它的是**门禁自己的
// 默认口径 `--network none`** —— 断网下还能绿，才说明这条判据的前提是它自己建立的。

use tokio::net::TcpListener;

async fn dead_port() -> u16 {
    // 绑后立即释放 → 该端口大概率无监听 → connect 立即 RST（快速失败）。
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

/// 一个「TCP 接了，但**永不吐 SSH 版本串**」的本地对端 —— 看门狗那条判据的前提。
///
/// # 为什么不是黑洞 IP〔`K-R24`〕
///
/// 旧写法拨 `10.255.255.1:22`，靠「这台机器到那个地址**有路由**、且包被静默丢弃」
/// 让 connect 挂起。那条前提是**环境性的，而它既不建立、也不检查**：沙箱默认
/// `--network none` 里没有那条路由 ⇒ connect 立刻 `ENETUNREACH` ⇒ 内层瞬间跑完
/// ⇒ 走 `race_connect` 的**聚合失败**支而不是 deadline 支
/// ⇒ 「看门狗坏了」与「本机没有到黑洞的路由」共用了同一个红。
///
/// # 它挂在哪一段（这一格是本修的要害）
///
/// 本对端 `accept()` 之后**一个字节都不回**。russh `client::connect_stream` 的次序是
/// **先写出自己的 `SSH-2.0-…` 标识，再停在「读对端标识」那一步等着**，而
/// `client::Config::default()` 的 `inactivity_timeout` 是 `None`（无客户端侧超时）
/// ⇒ 卡住的是**握手**，不是 TCP 连接。
/// （那一步的上游函数名此处刻意不点：它是仓外符号，点了就要进 `structural_scan` 的
/// 仓外名字登记表，而那张表不在本件写区 —— 机制上面已经说全，不靠那个名字承重。）
/// ★ 这才对得上断言原文那句「握手超时」—— 黑洞地址连 TCP connect 都没完成过，
///   它驱动的其实是「连接挂起」那条路，措辞却是「握手」那条路的。
///
/// 返回 `(地址, 痕迹)`。痕迹让**前提本身可被断言**，见 `PeerTrace`。
async fn spawn_silent_peer() -> (Endpoint, Arc<Mutex<PeerTrace>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let trace: Arc<Mutex<PeerTrace>> = Arc::new(Mutex::new(PeerTrace::default()));
    let trace_w = Arc::clone(&trace);
    tokio::spawn(async move {
        // 只接一条。收下之后读一次（记下对端的 SSH 标识），然后**攥着不放** ——
        // 既不回字节、也不主动关，让对端一直卡在「读对端标识」那一步。
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = [0u8; 128];
            if let Ok(n) = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await {
                let mut t = trace_w.lock().unwrap();
                t.banner = Some(String::from_utf8_lossy(&buf[..n]).into_owned());
                t.banner_at = Some(std::time::Instant::now());
            }
            // 继续读，但**永远不回一个字节** —— 客户端挂在「读对端标识」那一步时这一读
            // 一直 pend；等 `race_connect` 胜出/到点后 `abort_all` 把在飞那一路 drop 掉、
            // socket 随之关闭，这一读才以 EOF（或 reset）返回。
            // ⇒ 记下这一刻 = 记下「那条连接一直挂到被整批收走为止」。
            // ⚠ 循环而不是只读一次：客户端可能在标识之后紧跟着又写了 KEXINIT，
            //   只读一次会把「又来了几个字节」误读成「客户端还没走」。
            let mut sink = [0u8; 256];
            loop {
                match tokio::io::AsyncReadExt::read(&mut stream, &mut sink).await {
                    Ok(0) | Err(_) => {
                        trace_w.lock().unwrap().client_went_away_at =
                            Some(std::time::Instant::now());
                        break;
                    }
                    Ok(_) => continue,
                }
            }
            std::future::pending::<()>().await;
        }
    });
    (ep("127.0.0.1", addr.port()), trace)
}

/// 静默对端**被走到了什么程度**的痕迹〔`K-R24` 下一拍〕。
///
/// # 为什么判据不能只断言结果
///
/// `race_live_server_wins_when_a_hung_peer_is_first` 要证的是「首地址挂起也不吊死整批」。
/// 但它的**结果**（live 胜出）**从来不随环境翻转** —— 首地址不管是挂住、还是瞬间
/// `ENETUNREACH`、还是被 RST 拒了，live 都照样赢。⇒ **只断言结果的判据，在
/// 「那半根本没被驱动」时也是绿的** —— 那不是「红说不清是什么红」，是**「绿说不清测没测到」**。
///
/// ⇒ 出路不是把断言写得更狠，是**让夹具记下它被走到了什么程度**，判据去断言那个痕迹。
#[derive(Default, Clone)]
struct PeerTrace {
    /// 对端读到的首批字节（正常即客户端的 `SSH-2.0-…` 标识）。
    /// 有它 ⇒ TCP 早已连上、且卡的是**握手**那一段，不是 TCP 连接那一段。
    banner: Option<String>,
    /// 读到那批字节的时刻 —— 用来断言这事发生在竞速**进行中**，而不是事后。
    banner_at: Option<std::time::Instant>,
    /// 客户端那一侧先撒手了（socket 关掉 ⇒ 这边读到 EOF / reset）的时刻。
    /// 🔴 本夹具**自己从不主动关、也从不回一个字节** ⇒ 这一格有值 **⇔**
    /// 那条连接从收到标识起一直挂着，直到被 `race_connect` 的 `abort_all` 收走。
    /// **这才是「首地址真的挂住了」那件事本身**，而不是它的后果。
    client_went_away_at: Option<std::time::Instant>,
}

/// 等一个条件成立，最多 ~2s（立刻成立就立刻返回，不花这 2s）。
/// 用在断言「前提确已建立」之前 —— 免得把**调度抖动**读成「前提不成立」。
async fn settled(mut cond: impl FnMut() -> bool) -> bool {
    for _ in 0..200 {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

fn test_config() -> Arc<client::Config> {
    Arc::new(client::Config::default())
}

/// race_connect 的 Ok 分支持有不实现 Debug 的 Handle,不能直接 unwrap_err；
/// 这个 helper 压成错误串便于断言错误路径。
fn race_err(
    r: Result<
        (
            client::Handle<ClientHandler>,
            Arc<Mutex<Option<String>>>,
            Endpoint,
        ),
        String,
    >,
) -> String {
    match r {
        Ok(_) => panic!("expected Err, got a live connection"),
        Err(e) => e,
    }
}

#[tokio::test]
async fn race_all_dead_aggregates_errors() {
    let p1 = dead_port().await;
    let p2 = dead_port().await;
    let order = vec![ep("127.0.0.1", p1), ep("127.0.0.1", p2)];
    let err =
        race_err(race_connect(test_config(), None, order, Duration::from_secs(5), None).await);
    // trap #3：聚合报告，含「所有地址连接失败」且提到两个地址（至少首个立即失败）。
    assert!(err.contains("所有地址连接失败"), "应聚合: {err}");
    assert!(err.contains(&p1.to_string()), "应含首地址: {err}");
}

/// 看门狗：到点整批 abort、不吊死。对端是**本地静默 listener**（握手挂起）。
///
/// 🔴 这条判据的前提由它**自己建立**（`spawn_silent_peer`），并且**自己断言**。
/// 从前「红」这一个读数装着三件事，现在三件各有各的话：
///   ① **前提没建立**（对端没收到客户端标识）⇒ 「前提不成立……这条今天判不了」；
///   ② **看门狗吊死**（deadline 根本不兑现）⇒ 「3 秒内没返回」，当场红而不是把测试二进制挂住；
///   ③ **走错了支**（快速失败顺路带出措辞 / 措辞变了）⇒ 「没等到 deadline」或「应超时」。
#[tokio::test]
async fn race_watchdog_times_out_on_a_silent_peer() {
    let (silent, trace) = spawn_silent_peer().await;
    let start = std::time::Instant::now();
    // 外层再兜一道 3s：看门狗真坏时**当场红并说清楚**，而不是把整个测试二进制吊死。
    // （原来那条 `elapsed < 3s` 的上界改由这一道守 —— 上界没丢，换了个说得出话的地方。）
    let raced = tokio::time::timeout(
        Duration::from_secs(3),
        race_connect(
            test_config(),
            None,
            vec![silent],
            Duration::from_millis(400),
            None,
        ),
    )
    .await;
    let elapsed = start.elapsed();
    let Ok(inner) = raced else {
        panic!("看门狗没兑现：deadline 给的是 400ms，3 秒内 race_connect 没返回 —— 它吊死了");
    };
    let err = race_err(inner);

    // ① 前提这一半：对端确实收到了客户端的 SSH 标识 ⇒ TCP 连上了、卡的是**握手**那一段。
    assert!(
        settled(|| trace.lock().unwrap().banner.is_some()).await,
        "前提不成立：该卡住的那个静默对端一个字节都没收到 —— 拨的根本不是它？\
             本机 loopback 不通？总之这条今天判不了，**它不是「看门狗坏了」**"
    );
    let banner = trace.lock().unwrap().banner.clone().unwrap_or_default();
    assert!(
        banner.starts_with("SSH-2.0-"),
        "前提不成立：对端收到的不是 SSH 标识而是 {banner:?} —— 卡住的不是握手，\
             这条今天判不了，**它不是「看门狗坏了」**"
    );

    // ② 被测性质这一半：到点走 deadline 支（那句「握手超时」只在那一支出现）。
    assert!(err.contains("握手超时"), "应超时: {err}");
    // ③ 它是**等到 deadline 才**返回的 —— 不靠措辞一条腿站着：快速失败那一支
    //    （`ENETUNREACH` / RST）会在几毫秒内返回，这一条把那种情形直接判红。
    assert!(
        elapsed >= Duration::from_millis(400),
        "只花了 {elapsed:?} 就返回 —— 没等到 400ms deadline，走的是快速失败那一支，\
             不是看门狗那一支"
    );
}

#[tokio::test]
async fn race_single_endpoint_dead_reports_that_endpoint() {
    // 单地址退化路径：错误里报该地址（保留老实现的可诊断性）。
    let p = dead_port().await;
    let order = vec![ep("127.0.0.1", p)];
    let err =
        race_err(race_connect(test_config(), None, order, Duration::from_secs(5), None).await);
    assert!(err.contains(&p.to_string()), "单地址错误应含该地址: {err}");
}

#[tokio::test]
async fn race_empty_order_errors_cleanly() {
    let err =
        race_err(race_connect(test_config(), None, vec![], Duration::from_secs(1), None).await);
    assert!(err.contains("无可用地址"), "空 order: {err}");
}

// === F45 / D 审计 R-1：胜者 happy-path（live server 胜、慢地址被弃）===
// 起一个 mock russh server（run_stream 自动完成 KEX/握手,握手成功即客户端 Ok——race
// 只到握手,不需要真鉴权）。expected_fp=None 走 TOFU 接受该 mock key。

/// mock server 用的固定 ed25519 host key（ssh-keygen 生成）。
const MOCK_SERVER_KEY: &str = "\
-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACABVcXnVsSgWL3RAZE1r7ebLdEi510GsSqfaTYYzM26GwAAAJiEWV/KhFlf
ygAAAAtzc2gtZWQyNTUxOQAAACABVcXnVsSgWL3RAZE1r7ebLdEi510GsSqfaTYYzM26Gw
AAAEDRp5kloww4Jpr8K56RETPX0tLdId9XD8a+yNz5Tx0XOQFVxedWxKBYvdEBkTWvt5st
0SLnXQaxKp9pNhjMzbobAAAAD2Y0NS1tb2NrLXNlcnZlcgECAwQFBg==
-----END OPENSSH PRIVATE KEY-----";

struct MockServer;
impl russh::server::Handler for MockServer {
    type Error = russh::Error;
}

fn mock_server_config() -> Arc<russh::server::Config> {
    let key = russh::keys::PrivateKey::from_openssh(MOCK_SERVER_KEY).expect("parse mock key");
    Arc::new(russh::server::Config {
        keys: vec![key],
        ..Default::default()
    })
}

/// 起一个只接一条连接的 mock SSH server,返回其监听地址。
async fn spawn_mock_server() -> Endpoint {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let _ = russh::server::run_stream(mock_server_config(), stream, MockServer).await;
        }
    });
    ep("127.0.0.1", addr.port())
}

/// 从 race_connect 的 Ok 分支取胜者 Endpoint（Handle 不实现 Debug,丢弃即关连接）。
fn race_win(
    r: Result<
        (
            client::Handle<ClientHandler>,
            Arc<Mutex<Option<String>>>,
            Endpoint,
        ),
        String,
    >,
) -> Endpoint {
    match r {
        Ok((_h, _cell, ep)) => ep,
        Err(e) => panic!("expected a winner, got Err: {e}"),
    }
}

#[tokio::test]
async fn race_live_server_wins_when_first() {
    let live = spawn_mock_server().await;
    // live 排 i=0 立即拨、静默对端 i=1 延迟 → live 握手先成功即胜。
    // （i=1 那位通常根本没被拨到就被 abort 了 ⇒ 这里不断言它的前提，见下一条。）
    let (silent, _trace) = spawn_silent_peer().await;
    let order = vec![live.clone(), silent];
    let win =
        race_win(race_connect(test_config(), None, order, Duration::from_secs(5), None).await);
    assert_eq!(win, live, "live server 应胜出");
}

/// 静默对端排首(i=0 立即拨,卡在握手永不完成)、live 排 i=1(250ms 后拨)——慢地址被弃,live 仍胜。
/// 佐证 trap #8:首地址挂起不吊死整批,后位可达地址照样赢。
///
/// # 🔴 这条判据治的是「**绿说不清测没测到**」〔`K-R24` D1 边界 + 下一拍〕
///
/// 它的**判决从来不随环境翻转，翻转的是它有没有测到东西**：旧写法拨黑洞 IP，
/// 断网下那个地址**瞬间报错**而不是挂起 ⇒ live 照样胜 ⇒ **判决仍绿，
/// 而「首地址挂起也不吊死整批」这半再没被驱动过**。
///
/// ⚠⚠ **所以承重的不是那句 `assert_eq!(win, live)`** —— 首地址无论是挂住、
/// 瞬间 `ENETUNREACH`、还是被 RST 拒了，live 都赢。**只断言结果 = 什么都没断言。**
/// 要断言的是**「首地址真的挂住了」这件事本身**，下面两条腿各自独立地证它：
///
/// · **腿①（生产侧，不靠夹具记账）**：阶段事件流里首地址有 `dialing`、
///   **且自始至终没有 `failed`**，而 `won` 落在 live 上 ⇒ 首地址那一路**既没成也没败**，
///   是被 `abort_all` 收走的 —— **整批解决的那一刻它正挂着**。
///   （首地址若是快速失败，`race_connect:576` 会给它 emit 一条 `failed`。）
/// · **腿②（夹具侧痕迹，`PeerTrace`）**：对端记下它收到了客户端的 `SSH-2.0-` 标识
///   （⇒ TCP 早连上、卡的是握手那一段），且那条连接**一直攥到客户端被收走**才断 ——
///   而这个夹具自己从不主动关、从不回一个字节。
///
/// ★ 两条腿**不共用证据**：腿① 读的是被测函数自己吐的事件，腿② 读的是对端看到的字节。
#[tokio::test]
async fn race_live_server_wins_when_a_hung_peer_is_first() {
    let live = spawn_mock_server().await;
    let (hung, trace) = spawn_silent_peer().await;
    let hung_label = format!("{}:{}", hung.host, hung.port);
    let live_label = format!("{}:{}", live.host, live.port);
    let (ch, events) = collecting_channel();
    let order = vec![hung, live.clone()];
    let win =
        race_win(race_connect(test_config(), None, order, Duration::from_secs(5), Some(ch)).await);
    let race_returned_at = std::time::Instant::now();

    // 结果那半 —— 它不随环境翻转，所以它**不是**承重的那条腿。
    assert_eq!(win, live, "首地址挂起时后位 live 仍应胜出");

    // ── 腿①：首地址那一路「既没成也没败」，是被整批 abort 收走的 ──
    let ev = events.lock().unwrap().clone();
    let (Some(i_dial), Some(i_won)) = (
        ev.iter()
            .position(|(k, e)| k == "dialing" && *e == hung_label),
        ev.iter().position(|(k, e)| k == "won" && *e == live_label),
    ) else {
        panic!(
            "前提不成立：首地址压根没被拨、或 live 没胜出 —— 「首地址挂起也不吊死整批」\
                 这半没被驱动，这一条此刻是**绿得没有意义**的。事件流：{ev:?}"
        );
    };
    assert!(
        i_dial < i_won,
        "首地址是在 live 胜出之后才被拨的 —— 竞速期间它并没挂在那儿：{ev:?}"
    );
    assert!(
        !ev.iter().any(|(k, e)| k == "failed" && *e == hung_label),
        "首地址那一路**自己失败了**（不是挂住）—— live 照样胜、这条照样绿，\
             但「首地址挂起也不吊死整批」这半没被驱动。事件流：{ev:?}"
    );

    // ── 腿②：夹具痕迹 —— 走到了握手，并且一直挂到客户端被收走 ──
    assert!(
        settled(|| trace.lock().unwrap().banner.is_some()).await,
        "前提不成立：首地址那个对端一个字节都没收到 —— 「首地址挂起也不吊死整批」\
             这半没被驱动，这一条此刻是**绿得没有意义**的"
    );
    let t = trace.lock().unwrap().clone();
    let banner = t.banner.clone().unwrap_or_default();
    assert!(
        banner.starts_with("SSH-2.0-"),
        "首地址那个对端收到的不是 SSH 标识而是 {banner:?} —— 卡住的不是握手那一段"
    );
    assert!(
        t.banner_at.is_some_and(|at| at < race_returned_at),
        "首地址那一路是在竞速**结束之后**才走到握手的 —— 竞速进行时它并没挂在那儿"
    );
    assert!(
        settled(|| trace.lock().unwrap().client_went_away_at.is_some()).await,
        "首地址那条连接**不是被整批 abort 收走的** —— 它没挂住（自己先断了 / 从没真连上）。\
             ⚠ 注意这一条与上面那句 `assert_eq!(win, live)` 的区别：live 照样胜、结果照样对，\
             但「首地址挂起也不吊死整批」这半没被驱动"
    );
}

// === F46：连接分阶段事件 ===

#[test]
fn classify_stage_buckets() {
    assert_eq!(classify_stage("Connection refused (os error 111)"), "tcp");
    assert_eq!(classify_stage("No route to host"), "tcp");
    assert_eq!(classify_stage("operation timed out"), "timeout");
    assert_eq!(classify_stage("握手超时"), "timeout");
    assert_eq!(classify_stage("host key mismatch"), "hostkey");
    assert_eq!(classify_stage("Unknown server key"), "hostkey");
    assert_eq!(classify_stage("something else entirely"), "other");
}

/// 收集 Channel emit 的阶段事件（send→on_message(InvokeResponseBody::Json)），
/// 逐条记 `(kind, endpoint)`、**保序**。
///
/// ⚠ 从前这里只记 `kind`。带上 `endpoint` 是 `K-R24` 下一拍要的：
/// 「首地址那一路怎么了」与「后位那一路怎么了」在只有 `kind` 的流上**分不开**，
/// 而那正是 `race_live_server_wins_when_a_hung_peer_is_first` 要断言的东西。
/// `endpoint` 那一格对 `auth` / `established` 这类不带地址的阶段是空串。
fn collecting_channel() -> (
    tauri::ipc::Channel<ConnectStage>,
    Arc<Mutex<Vec<(String, String)>>>,
) {
    let sink = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let s2 = Arc::clone(&sink);
    let ch = tauri::ipc::Channel::new(move |body: tauri::ipc::InvokeResponseBody| {
        if let tauri::ipc::InvokeResponseBody::Json(json) = body {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                if let Some(k) = v.get("kind").and_then(|k| k.as_str()) {
                    let endpoint = v
                        .get("endpoint")
                        .and_then(|e| e.as_str())
                        .unwrap_or_default()
                        .to_string();
                    s2.lock().unwrap().push((k.to_string(), endpoint));
                }
            }
        }
        Ok(())
    });
    (ch, sink)
}

/// 只要 kind 那一维（给那两条不关心是哪个地址的判据用）。
fn stage_kinds(sink: &Arc<Mutex<Vec<(String, String)>>>) -> Vec<String> {
    sink.lock()
        .unwrap()
        .iter()
        .map(|(k, _)| k.clone())
        .collect()
}

#[tokio::test]
async fn race_emits_dialing_hostkey_won_for_live_server() {
    let live = spawn_mock_server().await;
    let (ch, sink) = collecting_channel();
    let _ = race_win(
        race_connect(
            test_config(),
            None,
            vec![live],
            Duration::from_secs(5),
            Some(ch),
        )
        .await,
    );
    let kinds = stage_kinds(&sink);
    assert!(
        kinds.contains(&"dialing".to_string()),
        "缺 dialing: {kinds:?}"
    );
    assert!(
        kinds.contains(&"hostKey".to_string()),
        "缺 hostKey: {kinds:?}"
    );
    assert!(kinds.contains(&"won".to_string()), "缺 won: {kinds:?}");
}

#[tokio::test]
async fn race_emits_dialing_and_failed_for_dead_address() {
    let p = dead_port().await;
    let (ch, sink) = collecting_channel();
    let _ = race_err(
        race_connect(
            test_config(),
            None,
            vec![ep("127.0.0.1", p)],
            Duration::from_secs(3),
            Some(ch),
        )
        .await,
    );
    let kinds = stage_kinds(&sink);
    assert!(
        kinds.contains(&"dialing".to_string()),
        "缺 dialing: {kinds:?}"
    );
    assert!(
        kinds.contains(&"failed".to_string()),
        "缺 failed: {kinds:?}"
    );
    assert!(
        !kinds.contains(&"won".to_string()),
        "死地址不应 won: {kinds:?}"
    );
}

#[tokio::test]
async fn race_emitter_none_still_works() {
    // emitter=None 路径不 panic、与 F45 行为等价（此处验死地址聚合）。
    let p = dead_port().await;
    let err = race_err(
        race_connect(
            test_config(),
            None,
            vec![ep("127.0.0.1", p)],
            Duration::from_secs(3),
            None,
        )
        .await,
    );
    assert!(err.contains(&p.to_string()));
}
