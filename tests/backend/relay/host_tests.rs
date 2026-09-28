//! 〔RL1 · V107〕常驻后端**进程内**的中转：`relay::listen::host` 与它在 `main.rs` 里的那一处接线。
//!
//! 判据（记录住 `调研/第四波记录/RL1.md §3` H1–H4）：
//! - H1 交了端口 ⇒ 真在听、真转发一段 SSE（假上游是本文件自己的 socket —— 异源）；
//! - H2 没交端口 ⇒ `NotAsked`、零监听；交了但起不来（端口被占 / 端口认不出 / 上游配置认不出）⇒ `Failed`，不退出；
//! - H3 生产接线（`accounts::upstream::host_relay`，`main.rs` 调的就是它）在**真子进程**里：
//!   转发一整段 SSE 之后，子进程 stdout 上**一行 tee 都没有**（stdio 载体上 stdout 就是 wire）；
//! - `main.rs` 那一处：流模式里恰好一处、排在一次性分派之后、选载体之前。
//!
//! ⚠ 走得到 `serve()` 的判据一律带读期限（风险 `5x`：把挂住换成红，同 `server_tests::send_request`）。

use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::Duration;

/// 上游那一段 SSE（**原样字节**；下游必须逐字节收到它）。
const SSE_BODY: &str = "event: message_start\ndata: {\"type\":\"message_start\",\"n\":1}\n\ndata: {\"type\":\"ping\"}\n\n";

/// 一个一次性的假上游：收一条请求（读到头结束），回一段 SSE、关连接。回地址。
fn fake_upstream() -> SocketAddr {
    let l = TcpListener::bind("127.0.0.1:0").expect("假上游 bind");
    let addr = l.local_addr().expect("假上游地址");
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
            // 读到请求头结束（本文件的请求带一个短体，读头就够判「来了一条」）。
            let mut head = Vec::new();
            let mut one = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                match s.read(&mut one) {
                    Ok(1) => head.push(one[0]),
                    _ => break,
                }
            }
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{SSE_BODY}",
                SSE_BODY.len()
            );
            let _ = s.write_all(resp.as_bytes());
            let _ = s.flush();
        }
    });
    addr
}

/// 〔TAP〕这几条只量「起没起来 / 转没转发」，不看 tee 抄了什么 ⇒ 给一个自己的 hub（不碰进程级那一个）。
fn no_tap() -> std::sync::Arc<dyn super::super::TapPort> {
    std::sync::Arc::new(crate::tap::TapHub::default())
}

/// 一次性目录（判据绝不碰用户真实的凭据文件）。
fn tmpdir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "ccm-rl1-host-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&d).expect("建临时目录");
    d
}

/// 一份只有一条 keyless 账号的凭据文件（走默认上游 —— 与 `server_tests::spawn_relay_child` 同一形）。
fn creds_fixture(tag: &str) -> std::path::PathBuf {
    let p = tmpdir(tag).join("apikey-credentials.json");
    std::fs::write(&p, b"{\n  \"accounts\": {\n    \"acctA\": {}\n  }\n}\n").expect("写凭据夹具");
    p
}

/// 喂给 `host` 的取值器：端口 ＋ 默认上游 ＋ 凭据路径 ＋〔RK1〕夹具家目录（钥匙文件在它底下，
/// 预先放好 `door::door_tests::TEST_KEY` —— 不给的话中转会去用户真实的家目录里铸钥匙）。
fn env_of(
    port: Option<&str>,
    upstream: Option<String>,
    creds: &std::path::Path,
) -> impl Fn(&str) -> Option<String> {
    let port = port.map(str::to_string);
    let home = door::door_tests::seed_test_home(creds.parent().expect("夹具目录"))
        .display()
        .to_string();
    let creds = creds.display().to_string();
    move |k: &str| match k {
        ENV_PORT => port.clone(),
        "CCM_AGENT_UPSTREAM_CLAUDE_CODE" => upstream.clone(),
        "CCM_APIKEY_CREDENTIALS" => Some(creds.clone()),
        "HOME" => Some(home.clone()),
        _ => None,
    }
}

/// 经中转发一条请求，读完整个应答（带读期限）。〔V141〕带 claude 那个会话标识头（值 `k-rl1`）。
fn through(addr: SocketAddr) -> String {
    through_with(addr, "x-claude-code-session-id: k-rl1\r\n")
}

/// 同上，多出来的请求头由调用方给（整行、带 `\r\n`）。
fn through_with(addr: SocketAddr, extra: &str) -> String {
    let mut c = TcpStream::connect(addr).expect("connect 中转");
    c.set_read_timeout(Some(Duration::from_secs(10)))
        .expect("读期限（风险 5x）");
    let body = "{}";
    let req = format!(
        // 〔RK1〕过门：钥匙段挂在最前、`Host` 用回环字面量。
        "POST /{}/s/claude-code/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        door::door_tests::TEST_KEY,
        body.len()
    );
    c.write_all(req.as_bytes()).expect("写请求");
    let mut out = String::new();
    let _ = c.read_to_string(&mut out);
    out
}

/// H2：没交端口 ⇒ 这个进程不开中转（远端经 SSH exec 起的流模式后端就是这一格）；空串 == 没交。
#[test]
fn a_process_that_was_not_handed_a_port_hosts_no_relay() {
    let creds = creds_fixture("none");
    for port in [None, Some(""), Some("   ")] {
        let got = host(
            &env_of(port, None, &creds),
            std::path::Path::new("/nonexistent"),
            &crate::accounts::upstream::Boot,
            no_tap(),
        );
        assert!(
            matches!(got, Hosted::NotAsked),
            "端口 {port:?} ⇒ 应是 NotAsked，得 {got:?}"
        );
    }
}

/// H1：交了端口 ⇒ 本进程里真有中转在听，且真转发 —— 下游收到的应答体逐字节 == 假上游发的那段 SSE。
#[test]
fn a_handed_port_really_listens_and_forwards_the_upstream_sse_byte_for_byte() {
    let up = fake_upstream();
    let creds = creds_fixture("fwd");
    let got = host(
        &env_of(
            Some("0"),
            Some(format!("http://127.0.0.1:{}", up.port())),
            &creds,
        ),
        std::path::Path::new("/nonexistent"),
        &crate::accounts::upstream::Boot,
        no_tap(),
    );
    let Hosted::Listening(addr) = got else {
        panic!("交了端口 0 ⇒ 应在听，得 {got:?}");
    };
    assert!(addr.ip().is_loopback(), "进程内中转必须只听回环：{addr}");
    let resp = through(addr);
    let (head, body) = resp
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("中转的应答不成形：{resp:?}"));
    assert!(head.starts_with("HTTP/1.1 200"), "应答头：{head:?}");
    assert_eq!(body, SSE_BODY, "下游收到的 SSE 与上游发的逐字节不等");
}

/// H2：交了端口、那个口被占着 ⇒ `Failed`（点名那个口），**不退出进程**（本条能跑到断言就是证据）。
#[test]
fn a_handed_port_that_is_taken_fails_loudly_without_taking_the_process_down() {
    let squatter = TcpListener::bind("127.0.0.1:0").expect("占口");
    let port = squatter.local_addr().expect("地址").port();
    let creds = creds_fixture("taken");
    let got = host(
        &env_of(Some(&port.to_string()), None, &creds),
        std::path::Path::new("/nonexistent"),
        &crate::accounts::upstream::Boot,
        no_tap(),
    );
    match got {
        Hosted::Failed(why) => {
            assert!(why.contains(&port.to_string()), "理由里要点名那个口：{why}")
        }
        other => panic!("口被占 ⇒ 应是 Failed，得 {other:?}"),
    }
}

/// H2：端口串认不出 ⇒ `Failed`，**不悄悄退回缺省端口**（注入侧拼的是交出来的那个数）。
#[test]
fn an_unreadable_port_is_refused_rather_than_defaulted() {
    let creds = creds_fixture("junk");
    for junk in ["abc", "70000", "-1"] {
        let got = host(
            &env_of(Some(junk), None, &creds),
            std::path::Path::new("/nonexistent"),
            &crate::accounts::upstream::Boot,
            no_tap(),
        );
        match got {
            Hosted::Failed(why) => assert!(why.contains(junk), "理由里要带那个原串：{why}"),
            other => panic!("端口 {junk:?} ⇒ 应是 Failed，得 {other:?}"),
        }
    }
}

/// H2：上游选择认不出启动配置（默认上游那个变量是坏的）⇒ `Failed`，而且**没有去绑那个口**（顺序：先认配置、再绑）。
#[test]
fn a_bad_upstream_config_fails_before_any_port_is_bound() {
    // 先拿一个此刻空着的口，再交给它 —— 失败之后我们还绑得上它 ⇒ 它没被占。
    let probe = TcpListener::bind("127.0.0.1:0").expect("探口");
    let port = probe.local_addr().expect("地址").port();
    drop(probe);
    let creds = creds_fixture("badup");
    let got = host(
        &env_of(
            Some(&port.to_string()),
            Some("not a url".to_string()),
            &creds,
        ),
        std::path::Path::new("/nonexistent"),
        &crate::accounts::upstream::Boot,
        no_tap(),
    );
    assert!(
        matches!(got, Hosted::Failed(_)),
        "坏上游 ⇒ 应是 Failed，得 {got:?}"
    );
    TcpListener::bind(("127.0.0.1", port)).expect("配置认不出时不该已经把口绑走了");
}

// ─────────────────────────────────────────────────────────────────────────────
//  H3：生产接线在真子进程里 —— stdout 上零 tee
// ─────────────────────────────────────────────────────────────────────────────

const CHILD_MARK: &str = "CCM_RL1_HOSTED_CHILD";
const CHILD_TEST_NAME: &str = "relay::listen::host_tests::hosted_relay_child_entry_point";

/// 子进程入口（不是判据 ⇒ `#[ignore]`）：走 `main.rs` 流模式**真调的那一个**
/// （`accounts::upstream::host_relay`），然后停在这里当一个「活着的宿主」，等父进程收掉它。
#[test]
#[ignore = "子进程入口：只在被父判据用 CCM_RL1_HOSTED_CHILD 拉起时才跑"]
fn hosted_relay_child_entry_point() {
    if std::env::var(CHILD_MARK).is_err() {
        return;
    }
    let said =
        crate::accounts::upstream::host_relay(&crate::agents::claudecode::paths::resolve_home());
    eprintln!("[rl1-child] {said}");
    loop {
        std::thread::park();
    }
}

struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// H3：生产接线 ⇒ 真在听、真转发；转发之后子进程 **stdout 上零 tee 行**（`__meta__` / `"event"` 一行都没有），
/// 而 stdout 的采集面是活的（libtest 自己那句 `running 1 test` 在上面 —— 非空对照）。
#[test]
fn the_production_wiring_hosts_the_relay_and_never_writes_tee_lines_to_stdout() {
    let up = fake_upstream();
    let creds = creds_fixture("child");
    let home = creds.parent().expect("夹具目录").to_path_buf();
    let exe = std::env::current_exe().expect("测试二进制自己的路径");
    let mut child = std::process::Command::new(exe)
        .args([
            CHILD_TEST_NAME,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_MARK, "1")
        .env(ENV_PORT, "0")
        .env(
            "CCM_AGENT_UPSTREAM_CLAUDE_CODE",
            format!("http://127.0.0.1:{}", up.port()),
        )
        .env("CCM_APIKEY_CREDENTIALS", &creds)
        .env("CLAUDE_CONFIG_DIR", &home)
        // 〔RK1〕钥匙文件落在夹具家目录里（预先放好夹具那一把），不碰用户真实的家目录。
        .env("HOME", door::door_tests::seed_test_home(&home))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("起子进程");
    let so = child.stdout.take().expect("stdout");
    let se = child.stderr.take().expect("stderr");
    let child = Child(child);

    let out = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let out_c = std::sync::Arc::clone(&out);
    std::thread::spawn(move || {
        for line in BufReader::new(so).lines().map_while(Result::ok) {
            let mut g = out_c.lock().expect("lock");
            g.push_str(&line);
            g.push('\n');
        }
    });
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(se).lines().map_while(Result::ok) {
            if line.contains("[relay] listening on") || line.contains("[rl1-child]") {
                let _ = tx.send(line);
            }
        }
    });
    let mut addr: Option<SocketAddr> = None;
    let mut said = String::new();
    while addr.is_none() || said.is_empty() {
        let line = rx
            .recv_timeout(Duration::from_secs(20))
            .expect("子进程 20s 内没说出「在听」与宿主那句话 —— 它没跑到进程内中转那一支");
        if let Some(rest) = line.strip_prefix("[relay] listening on ") {
            addr = Some(rest.trim().parse().expect("listening 行末段是地址"));
        } else {
            said = line;
        }
    }
    let addr = addr.expect("上面循环保证");
    assert!(said.contains("中转住本进程"), "宿主那句话：{said:?}");

    let resp = through(addr);
    assert!(
        resp.ends_with(SSE_BODY),
        "经子进程里的中转没收全上游那段 SSE：{resp:?}"
    );
    // 给子进程的 tee 写线程留出把行写出去的机会（若落点是 stdout 的话）：再经一趟往返，
    // 它的前一条的 tee 行早就排进了写线程 —— 这不是等「没有东西」，是再做一次同样的事。
    let _ = through(addr);
    drop(child);
    let stdout = out.lock().expect("lock").clone();
    assert!(
        stdout.contains("running 1 test"),
        "stdout 采集面是死的（连 libtest 那句都没有）⇒ 下面那条零命中是空真：{stdout:?}"
    );
    let tee: Vec<&str> = stdout
        .lines()
        .filter(|l| l.contains("__meta__") || l.contains("\"event\""))
        .collect();
    assert!(
        tee.is_empty(),
        "进程内中转往 stdout 写了 tee 行 —— stdio 载体上那就是 wire，一行一帧会被污染：{tee:?}"
    );
}

/// `main.rs` 那一处：生产段里 `accounts::upstream::host_relay(` **恰好一处**，
/// 排在一次性分派（`is_query_mode`）之后、选载体（`listen::mode_from`）之前 ——
/// 前者保证一次性子命令不会多开一个中转，后者保证两条载体都有它。
#[test]
fn main_hosts_the_relay_exactly_once_between_the_one_shot_dispatch_and_the_carrier_choice() {
    let raw = std::fs::read_to_string(crate::guard_support::src_root().join("main.rs"))
        .expect("读 main.rs");
    let prod = crate::guard_support::production_code(&raw);
    let call = guard_core::find_pinned(&prod, "accounts::upstream::host_relay(&agent_home)")
        .unwrap_or_else(|e| panic!("main.rs 生产段里那一处接线：{e}"));
    let dispatch = guard_core::find_pinned(&prod, "if is_query_mode(&args) {")
        .unwrap_or_else(|e| panic!("一次性分派那一行：{e}"));
    let carrier = guard_core::find_pinned(&prod, "listen::mode_from(")
        .unwrap_or_else(|e| panic!("选载体那一行：{e}"));
    assert!(
        dispatch < call && call < carrier,
        "顺序应是 一次性分派({dispatch}) < 起中转({call}) < 选载体({carrier})"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
//  〔TAP · V124〕tee 的第二个落点：进程内中转抄出来的 SSE 事件交到 tap 口
//  （设计住仓外 `调研/第四波记录/TAP.md §1.1 · §3`；出处 `设计/20 §8` · `设计/05 §4.5.3` ③）
// ─────────────────────────────────────────────────────────────────────────────

/// 手写的一轮上游 SSE：`event:` 行、`data:` 行、空行、`ping`、`[DONE]` 都有（期望值只从这张表来，异源）。
const TAP_EVENTS: &[&str] = &[
    r#"{"type":"message_start","message":{"id":"msg_tap_1","model":"m"}}"#,
    r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
    r#"{"type":"ping"}"#,
    r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"你好，"}}"#,
    r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"tap"}}"#,
    r#"{"type":"content_block_stop","index":0}"#,
    r#"{"type":"message_stop"}"#,
];

fn tap_body() -> String {
    let mut b = String::new();
    for e in TAP_EVENTS {
        // 每个事件前带一行 `event:`（切行器只认 `data:`），事件之间空行。
        b.push_str("event: x\ndata: ");
        b.push_str(e);
        b.push_str("\n\n");
    }
    b
}

/// 假上游：读完整条请求（头 ＋ 定长体 —— 不读体就关 ⇒ 内核发 RST，中转那一侧会把收尾读成「断了」），
/// 回给定的那一整段（定长），关连接。
fn fake_upstream_with(body: String) -> SocketAddr {
    let l = TcpListener::bind("127.0.0.1:0").expect("假上游 bind");
    let addr = l.local_addr().expect("假上游地址");
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
            let mut head = Vec::new();
            let mut one = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                match s.read(&mut one) {
                    Ok(1) => head.push(one[0]),
                    _ => break,
                }
            }
            let text = String::from_utf8_lossy(&head).to_ascii_lowercase();
            let len = text
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            let mut req_body = vec![0u8; len];
            let _ = s.read_exact(&mut req_body);
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            let _ = s.write_all(resp.as_bytes());
            let _ = s.flush();
        }
    });
    addr
}

/// 起一个进程内中转，tee 落给定的 tap 口；回在听的地址。
fn hosted_with_tap(
    tag: &str,
    up: SocketAddr,
    tap: std::sync::Arc<dyn super::super::TapPort>,
) -> SocketAddr {
    let creds = creds_fixture(tag);
    match host(
        &env_of(
            Some("0"),
            Some(format!("http://127.0.0.1:{}", up.port())),
            &creds,
        ),
        std::path::Path::new("/nonexistent"),
        &crate::accounts::upstream::Boot,
        tap,
    ) {
        Hosted::Listening(a) => a,
        other => panic!("交了端口 0 ⇒ 应在听，得 {other:?}"),
    }
}

/// 把 tap 接收端里此刻已有的全部取出来（`through` 返回时中转那条线程已经收尾：下游 EOF 在 `close` 之后）。
fn drain(
    rx: &mut tokio::sync::mpsc::Receiver<super::super::TapEvent>,
) -> Vec<super::super::TapEvent> {
    let mut v = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        v.push(ev);
    }
    v
}

/// T1：真中转 ＋ 假上游 ⇒ tap 口收到的事件 **==** 上游那一串 `data:`（逐字节、同序；`[DONE]` 与空行不算事件），
/// 位置号 `n` == 0..k 连续，最后一件是 `End{broken:false}` 且 `n == k`；`stream` == 请求自带的会话标识头（〔V141〕）；
/// 下游收到的字节照旧 == 上游发的（抄一份不动主路）。
#[test]
fn tap_gets_every_sse_data_payload_in_order_with_contiguous_positions_and_a_clean_end() {
    let body = tap_body();
    let up = fake_upstream_with(body.clone());
    let hub = std::sync::Arc::new(crate::tap::TapHub::default());
    let mut rx = hub.attach();
    let addr = hosted_with_tap("tap1", up, hub.clone());

    let resp = through(addr);
    let (_, down) = resp.split_once("\r\n\r\n").expect("应答成形");
    assert_eq!(down, body, "抄 tap 不许动下游字节");

    let got = drain(&mut rx);
    let k = TAP_EVENTS.len() as u64;
    let mut want: Vec<super::super::TapEvent> = TAP_EVENTS
        .iter()
        .enumerate()
        .map(|(i, e)| super::super::TapEvent {
            stream: "k-rl1".into(),
            resp: got.first().map(|g| g.resp).unwrap_or(u64::MAX),
            n: i as u64,
            body: super::super::TapBody::Data((*e).to_string()),
        })
        .collect();
    want.push(super::super::TapEvent {
        stream: "k-rl1".into(),
        resp: got.first().map(|g| g.resp).unwrap_or(u64::MAX),
        n: k,
        body: super::super::TapBody::End { broken: false },
    });
    assert_eq!(got, want, "tap 收到的事件序列与上游发的不等");
}

/// 〔V141 · R1〕**流标签 == claude 请求头里自带的会话标识**，与路径无关（路径里没有会话段）。
/// 守的要求：用户裁决 V141「中转从 claude 自己发的请求里认出这是哪个会话 …… 启动器不往中转地址里塞任何会话身份」。
/// 名单走生产接线（`host` → 上游选择 → 适配层 `session_header`）；缺头 / 值过不了段闸 ⇒ 空标签（前端当匿名流）。
#[test]
fn the_stream_label_is_the_session_id_the_agent_sends_in_its_own_request_header() {
    let body = tap_body();
    let up = fake_upstream_with(body);
    let hub = std::sync::Arc::new(crate::tap::TapHub::default());
    let mut rx = hub.attach();
    let addr = hosted_with_tap("tapv141", up, hub.clone());
    let sid = "3f2a9c1e-7d44-4c3b-9a55-0e6b2f1d8c77";
    let label_of = |extra: &str, rx: &mut tokio::sync::mpsc::Receiver<super::super::TapEvent>| {
        through_with(addr, extra);
        let got = drain(rx);
        assert!(!got.is_empty(), "正控：这一发该有 tap 件");
        let labels: std::collections::BTreeSet<String> =
            got.into_iter().map(|e| e.stream).collect();
        labels.into_iter().collect::<Vec<_>>()
    };
    assert_eq!(
        label_of(&format!("X-Claude-Code-Session-Id: {sid}\r\n"), &mut rx),
        vec![sid.to_string()],
        "流标签不是请求头里那个会话标识"
    );
    assert_eq!(
        label_of("", &mut rx),
        vec![String::new()],
        "没带头 ⇒ 空标签"
    );
    assert_eq!(
        label_of("x-claude-code-session-id: a/b\r\n", &mut rx),
        vec![String::new()],
        "过不了段闸的值不许进 tap"
    );
}

/// T2：tap 那一侧跟不上（通道只容 1 件、接收端不读）⇒ **下游字节一个不少**；收到的那几件的位置号
/// 是 0..k 的**真子集**、`End.n == k` ⇒ 缺在哪两号之间，接收侧纯算术算得出（`05 §3.3.4` 的 `Gap` 形）。
/// 同一趟里「没人连着」（发送端不在）⇒ 转发照常、一件都收不到。
#[test]
fn a_tap_that_cannot_keep_up_loses_positions_visibly_and_never_touches_the_forwarded_bytes() {
    let body = tap_body();
    let k = TAP_EVENTS.len() as u64;

    // ① 容量 1、不读：第一件进去之后全满 ⇒ 只剩第 0 号；收尾那一件也投不进（尾巴也会丢 ——
    //    接收侧拿「0 号之后再没有东西」判不出断没断，那一格由前端的撤卡规则兜，见 TAP.md §4）。
    let up = fake_upstream_with(body.clone());
    let hub = std::sync::Arc::new(crate::tap::TapHub::default());
    let mut rx = hub.attach_bounded(1);
    let addr = hosted_with_tap("tap2", up, hub.clone());
    let resp = through(addr);
    assert_eq!(
        resp.split_once("\r\n\r\n").expect("成形").1,
        body,
        "tap 满了也不许动下游字节"
    );
    let got = drain(&mut rx);
    let ns: Vec<u64> = got.iter().map(|e| e.n).collect();
    assert_eq!(
        ns,
        vec![0],
        "容量 1、不读 ⇒ 恰好只剩第 0 号（其余全丢、号照占）：{got:?}"
    );

    // ② 没人连着：hub 里没有发送端 ⇒ 转发照常。
    let up = fake_upstream_with(body.clone());
    let lonely = std::sync::Arc::new(crate::tap::TapHub::default());
    let addr = hosted_with_tap("tap3", up, lonely);
    let resp = through(addr);
    assert_eq!(
        resp.split_once("\r\n\r\n").expect("成形").1,
        body,
        "没人连着也不许动下游字节"
    );

    // ③ 容量够但中途丢一件（第 2 号超单件上限）⇒ 收到的号 == 0..k 去掉 2，`End.n == k`。
    let mut fat: Vec<String> = TAP_EVENTS.iter().map(|s| s.to_string()).collect();
    fat[2] = format!(
        r#"{{"type":"ping","pad":"{}"}}"#,
        "x".repeat(super::super::tee::TAP_DATA_CAP)
    );
    let mut b = String::new();
    for e in &fat {
        b.push_str("data: ");
        b.push_str(e);
        b.push_str("\n\n");
    }
    let up = fake_upstream_with(b.clone());
    let hub = std::sync::Arc::new(crate::tap::TapHub::default());
    let mut rx = hub.attach();
    let addr = hosted_with_tap("tap4", up, hub.clone());
    let resp = through(addr);
    assert_eq!(
        resp.split_once("\r\n\r\n").expect("成形").1,
        b,
        "超上限的那件不许动下游字节"
    );
    let got = drain(&mut rx);
    let ns: Vec<u64> = got.iter().map(|e| e.n).collect();
    let mut want: Vec<u64> = (0..k).filter(|&i| i != 2).collect();
    want.push(k);
    assert_eq!(
        ns, want,
        "超上限那一件的号要空着（缺口原位可见），收尾那件带总数"
    );
    assert_eq!(
        got.last().map(|e| e.body.clone()),
        Some(super::super::TapBody::End { broken: false })
    );
}

/// T2b：非 SSE 的应答（一个事件都没有）⇒ tap 一件都不交（连 `End` 都不交：接收侧本来也认不出它）。
#[test]
fn a_response_without_sse_events_hands_nothing_to_the_tap() {
    let up = fake_upstream_with("{\"type\":\"error\"}".to_string());
    let hub = std::sync::Arc::new(crate::tap::TapHub::default());
    let mut rx = hub.attach();
    let addr = hosted_with_tap("tap5", up, hub.clone());
    let resp = through(addr);
    assert!(resp.ends_with("{\"type\":\"error\"}"), "{resp:?}");
    assert_eq!(drain(&mut rx), Vec::new());
}
