//! **线上字节的金标准** —— 「零行为变化」这句话的量具（`设计/20 §7` 步 1–3）。
//!
//! # 它为什么必须存在
//!
//! `20 §7` 给步 1–3 的「行为变化」那一栏逐字写着 **零**。而既有那几十条判据量的是
//! **性质**（「auth 头被换掉了」「块数对得上」「404 不发上游」），**没有一条量的是
//! 「发出去的那串字节逐字节还是那一串」**。两者差得很远：把 `Connection: close`
//! 挪到别的位置、把 `Host:` 少写一个空格、把 tee 帧的字段换个序 —— 上面那些判据
//! **全绿**，而下游/上游收到的字节已经不是同一串了。
//!
//! ⇒ 本文件把**三条线上的字节**钉成手写字面量：
//!
//! | 线 | 钉的是什么 |
//! |---|---|
//! | 上游那一跳 | 中转**发给上游**的请求头逐字节 ＋ 请求体逐字节 |
//! | 下游那一跳 | 中转**回给客户端**的全部字节（响应头 ＋ 逐块透传的体） |
//! | tee 那一路 | tee 交出的每一件、写成线上那一形 `tap` 帧（`tap::to_frame`）之后**整行**逐字节 |
//!
//! # 期望值是**手写字面量**，不是拿被测代码算的
//!
//! 每一条期望都写在 [`GOLDEN`] 那张表里，是我**读着改动前的源码推出来、再在改动前的
//! 那棵树上跑绿**的（基点 `641e91bc`）。⇒ 搬家那一刀之后它们**一个字节都不许动** ——
//! 动了就说明「零行为变化」这句话是假的。
//!
//! ⚠ **唯一一处归一化**，如实写在这里：假上游的端口是内核给的（`bind(0)`），
//! 每趟都不同 ⇒ 比对前把 `127.0.0.1:<真端口>` 换成 `127.0.0.1:UPSTREAM`。
//! 除这一处之外**没有任何归一化**：大小写、空格、`\r\n`、字段序全是原样比。
//!
//! # 反空真
//!
//! - 格数：跑过的格子数与 [`GOLDEN_CASES`] 这个**显式登记的数**做**相等断言**，
//!   不是「循环跑完就绿」。一条都不跑 ⇒ `0 != 4` ⇒ 红。
//! - 每一格里「上游收到了什么」有两种期望：`Some(...)`（逐字节相等）与 `None`
//!   （**一个字节都没到上游**）。后者是「扫不到就绿」那一形，所以同一张表里
//!   **必须有 `Some(...)` 那几格垫着** —— 由 [`GOLDEN_CASES_REACHING_UPSTREAM`]
//!   再做一次相等断言：真到上游的格数少了，同样红。

use super::listen::{listen, serve, DOWNSTREAM_DEADLINE, UPSTREAM_DEADLINE};
use super::server::Relay;
use super::tee::{TapEvent, TapPort, TeeSink};
use super::upstream::Base;
use crate::accounts::upstream::table::RoutingTable;
use crate::accounts::upstream::Accounts;
use creds_core::store::AuthStyle;
use creds_core::SecretKey;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::{mpsc, Arc, Mutex};

/// 只听回环。**字面量**，不是拼出来的（同 `server.rs::LOOPBACK` 那条理由）。
const STUB_LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// 假上游**一字不差**回这一串。响应体是一条 chunked 包着的 SSE 事件 ＋ 终止块。
///
/// `f` 是那一块的长度（十六进制 15）：`data: ` 6 ＋ `{"n":1}` 7 ＋ 两个换行 2。
const UPSTREAM_RESPONSE: &str = concat!(
    "HTTP/1.1 200 OK\r\n",
    "Content-Type: text/event-stream\r\n",
    "Transfer-Encoding: chunked\r\n",
    "\r\n",
    "f\r\n",
    "data: {\"n\":1}\n\n",
    "\r\n",
    "0\r\n\r\n",
);

/// 下游打过去的请求体。**手写字面量**，7 字节。
const CLIENT_BODY: &str = "{\"m\":1}";

/// 下游自带的那份鉴权头的值。代入模式下它**必须**被换掉，直通模式下**必须**原样到上游。
const CLIENT_TOKEN: &str = "CLIENT-TOKEN";

/// 表里那一行自己的 key。
const ROW_KEY: &str = "KEY-A";

/// 表里配得到的两个账号段：`WITH_KEY` 有自己的 key，`NO_KEY` 没有。
const ACCT_WITH_KEY: &str = "acctA";
const ACCT_NO_KEY: &str = "acctB";

/// 上游那一跳的 `Host:` 里把真端口换成这个词之后再比（见头注「唯一一处归一化」）。
const UPSTREAM_HOST: &str = "127.0.0.1:UPSTREAM";

/// 一格金标准。
struct Golden {
    /// 下游请求行里的那个 target（含路由前缀）。
    target: &'static str,
    /// 中转**发给上游**的请求头逐字节；`None` = 这一格一个字节都不该到上游。
    upstream_head: Option<&'static str>,
    /// 中转**回给下游**的全部字节。
    downstream: &'static str,
    /// tee 交出的全部 `tap` 帧（一件一行；`""` = 这一格 tee 一件都不该交）。
    /// 〔DEL〕先前这一栏是 NDJSON 行（独立 `--relay` 的 stdout 那一形），那一形删了；今天钉的是会上 wire 的那一形。
    tee: &'static str,
}

/// 登记的格数。**手写的数**，与下面真跑过的格数做相等断言（反空真）。
///
/// ⚠ 〔`设计/20 §7` 步 3〕**4 → 6**：加的两格全是 `/t/` 那条新路
/// （⑤⑥）。前四格的期望值**一个字节都没动**，那正是「零行为变化」这句话的量点。
const GOLDEN_CASES: usize = 6;
/// 其中**真的把字节送到上游**的格数。垫住 `upstream_head: None` 那几格的空真。
const GOLDEN_CASES_REACHING_UPSTREAM: usize = 3;

/// 六格金标准。**每一串都是手写的**。〔V141〕路径里没有会话段；这几发客户端不带会话标识头 ⇒ tap 帧的 `stream` 是空串。
const GOLDEN: &[Golden] = &[
    // ① 代入模式 ＋ 表里那一行有 key ⇒ 下游那份 auth 头被**整条丢掉**，换成这一行自己的。
    Golden {
        target: "/s/agentA/acctA/v1/messages",
        upstream_head: Some(concat!(
            "POST /v1/messages HTTP/1.1\r\n",
            "Host: 127.0.0.1:UPSTREAM\r\n",
            "Accept-Encoding: identity\r\n",
            "Connection: close\r\n",
            "Authorization: Bearer KEY-A\r\n",
            "Content-Length: 7\r\n",
            "\r\n",
        )),
        downstream: concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: text/event-stream\r\n",
            "Transfer-Encoding: chunked\r\n",
            "Connection: close\r\n",
            "\r\n",
            "f\r\n",
            "data: {\"n\":1}\n\n",
            "\r\n",
            "0\r\n\r\n",
        ),
        tee: concat!(
            r#"{"kind":"tap","stream":"","resp":0,"n":0,"data":"{\"n\":1}"}"#,
            "\n",
            r#"{"kind":"tap","stream":"","resp":0,"n":1,"end":"done"}"#,
            "\n",
        ),
    },
    // ② 代入模式 ＋ 表里那一行**没有** key ⇒ 下游那份 auth 头**逐字节原样**到上游。
    //    （订阅登录那一档是合法状态：行在表里、没有 key。）
    Golden {
        target: "/s/agentA/acctB/v1/messages",
        upstream_head: Some(concat!(
            "POST /v1/messages HTTP/1.1\r\n",
            "Host: 127.0.0.1:UPSTREAM\r\n",
            "Accept-Encoding: identity\r\n",
            "Connection: close\r\n",
            "Authorization: Bearer CLIENT-TOKEN\r\n",
            "Content-Length: 7\r\n",
            "\r\n",
        )),
        downstream: concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: text/event-stream\r\n",
            "Transfer-Encoding: chunked\r\n",
            "Connection: close\r\n",
            "\r\n",
            "f\r\n",
            "data: {\"n\":1}\n\n",
            "\r\n",
            "0\r\n\r\n",
        ),
        tee: concat!(
            r#"{"kind":"tap","stream":"","resp":0,"n":0,"data":"{\"n\":1}"}"#,
            "\n",
            r#"{"kind":"tap","stream":"","resp":0,"n":1,"end":"done"}"#,
            "\n",
        ),
    },
    // ③ 代入模式 ＋ 表里**没有这一行** ⇒ 404，一个字节都不到上游，tee 上一行都没有。
    Golden {
        target: "/s/agentA/nosuch/v1/messages",
        upstream_head: None,
        downstream: concat!(
            "HTTP/1.1 404 Not Found\r\n",
            "Content-Length: 14\r\n",
            "Connection: close\r\n",
            "\r\n",
            "404 Not Found\n",
        ),
        tee: "",
    },
    // ④ 根本不是路由形状 ⇒ 同样 404，同样一个字节都不到上游。
    Golden {
        target: "/v1/messages",
        upstream_head: None,
        downstream: concat!(
            "HTTP/1.1 404 Not Found\r\n",
            "Content-Length: 14\r\n",
            "Connection: close\r\n",
            "\r\n",
            "404 Not Found\n",
        ),
        tee: "",
    },
    // ⑤ 🔴 **直通模式 ＋ 表里那一行有 key ⇒ 绝不代入**（`20 §3.1` 第 3 行）。
    //    与 ① **同一个账号段**（`acctA`，表里配着 `KEY-A`），只有前缀不同 ⇒
    //    量到的差别只能来自模式那一格。`/t/` 逐字是「中转永不代入 auth」：
    //    上游收到的必须是**客户端那把**（`CLIENT-TOKEN`），`KEY-A` 一个字节都不许出现。
    Golden {
        target: "/t/agentA/acctA/v1/messages",
        upstream_head: Some(concat!(
            "POST /v1/messages HTTP/1.1\r\n",
            "Host: 127.0.0.1:UPSTREAM\r\n",
            "Accept-Encoding: identity\r\n",
            "Connection: close\r\n",
            "Authorization: Bearer CLIENT-TOKEN\r\n",
            "Content-Length: 7\r\n",
            "\r\n",
        )),
        downstream: concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: text/event-stream\r\n",
            "Transfer-Encoding: chunked\r\n",
            "Connection: close\r\n",
            "\r\n",
            "f\r\n",
            "data: {\"n\":1}\n\n",
            "\r\n",
            "0\r\n\r\n",
        ),
        tee: concat!(
            r#"{"kind":"tap","stream":"","resp":0,"n":0,"data":"{\"n\":1}"}"#,
            "\n",
            r#"{"kind":"tap","stream":"","resp":0,"n":1,"end":"done"}"#,
            "\n",
        ),
    },
    // ⑥ 直通模式 ＋ 表里**没有这一行** ⇒ **502**，一个字节都不到上游。
    //    🔴 `20 §3.1` 第 4 行逐字「不许回落到某一个写死的常量」：那一格要「按 `seg1`
    //    取该 agent 的默认上游」。那张每 agent 一行的表（条 59）今天落了
    //    （`agents::Adapter::upstream`，〔NT2 · V25〕跟着适配层），而本格的 `seg1`（`GOLDEN_AGENT`）**不在表里**
    //    ⇒ 未登记 ⇒ 502。理由整段住 `accounts::upstream::decide`；登记过的那一半由
    //    `table_tests` 那条「登记过的走自己那一行、未登记的拒」量（不经网络）。
    //    ⚠ 它与 ③ 的 404 **刻意不同码**：404 答的是「代入模式要求表里有这一行」，
    //    502 答的是「这个 agent 没有登记上游」——两件事，两个码。
    Golden {
        target: "/t/agentA/nosuch/v1/messages",
        upstream_head: None,
        downstream: concat!(
            "HTTP/1.1 502 Bad Gateway\r\n",
            "Content-Length: 16\r\n",
            "Connection: close\r\n",
            "\r\n",
            "502 Bad Gateway\n",
        ),
        tee: "",
    },
];

/// 假上游：把**收到的请求头原样**记下来，然后回 [`UPSTREAM_RESPONSE`]。
struct Stub {
    addr: SocketAddr,
    heads: Arc<Mutex<Vec<String>>>,
    bodies: Arc<Mutex<Vec<String>>>,
}

fn spawn_stub() -> Stub {
    let listener = TcpListener::bind(SocketAddr::new(STUB_LOOPBACK, 0)).expect("bind 假上游");
    let addr = listener.local_addr().expect("假上游地址");
    let heads = Arc::new(Mutex::new(Vec::new()));
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let (h, b) = (Arc::clone(&heads), Arc::clone(&bodies));
    std::thread::spawn(move || {
        for s in listener.incoming() {
            let Ok(mut s) = s else { continue };
            // 读期限是兜底：中转少发字节时要在对账那条断言上红，**不许挂住**。
            let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(4000)));
            let Ok(clone) = s.try_clone() else { continue };
            let mut r = BufReader::new(clone);
            let mut head = String::new();
            let mut clen = 0usize;
            loop {
                let mut line = String::new();
                match r.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                head.push_str(&line);
                if line == "\r\n" {
                    break;
                }
                let low = line.to_ascii_lowercase();
                if let Some(v) = low.strip_prefix("content-length:") {
                    clen = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; clen];
            let _ = r.read_exact(&mut body);
            h.lock().expect("lock").push(head);
            b.lock()
                .expect("lock")
                .push(String::from_utf8_lossy(&body).into_owned());
            let _ = s.set_nodelay(true);
            let _ = s.write_all(UPSTREAM_RESPONSE.as_bytes());
            let _ = s.flush();
            // 这里 drop `s` ⇒ 下游读到干净 EOF。
        }
    });
    Stub {
        addr,
        heads,
        bodies,
    }
}

/// tee 的落点：一个测试侧的 tap 口 —— 每收一件就写成线上那一形 `tap` 帧（一行）攒起来，并敲一次钟（判据靠它等，不靠睡）。
struct TeeTap {
    buf: Arc<Mutex<Vec<u8>>>,
    rx: mpsc::Receiver<()>,
}

struct FramingTap(Arc<Mutex<Vec<u8>>>, Mutex<mpsc::Sender<()>>);
impl TapPort for FramingTap {
    fn offer(&self, ev: TapEvent) -> bool {
        let mut line = serde_json::to_string(&crate::stream::tap::to_frame(ev)).expect("tap 帧");
        line.push('\n');
        self.0
            .lock()
            .expect("lock")
            .extend_from_slice(line.as_bytes());
        let _ = self.1.lock().expect("lock").send(());
        true
    }
}

impl TeeTap {
    fn wait_lines(&self, n: usize) {
        for i in 0..n {
            self.rx
                .recv_timeout(std::time::Duration::from_secs(4))
                .unwrap_or_else(|e| panic!("等 tee 的第 {} 件没等到：{e}", i + 1));
        }
    }

    fn text(&self) -> String {
        String::from_utf8(self.buf.lock().expect("lock").clone()).expect("tee 不是 utf8")
    }
}

/// 这张表里那两行挂在谁名下（条 49：键是 agent ＋ 账号）。
///
/// ⚠ **它必须与下面那些请求行的首段逐字相同**（`/s/agentA/…`），否则 `/s/` 那几格全变 404。
/// ⚠ 它**刻意不是**任何一家登记过的 agent：`/t/` ＋ 表里无行那一格（本文件第 ⑤′ 格，`/t/agentA/nosuch`）
///   钉的是「未登记 ⇒ 502」—— 换成一家登记过的，那一格就会真的连出去。
/// ⚠ 这是**夹具**的改动，不是期望字节的改动：每一格手写的期望串一个字节都没动。
const GOLDEN_AGENT: &str = "agentA";

/// 起一个中转：表里两行（一行有 key、一行没有），都指着那个假上游。
fn spawn_relay(up: SocketAddr) -> (SocketAddr, TeeTap) {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let (tick, rx) = mpsc::channel();
    let base = Base::parse(&format!("http://127.0.0.1:{}", up.port())).expect("base");
    // ⚠ 走的是**生产段那条真实的装表路** `RoutingTable::build`，不是另造一个同构的表。
    let table = RoutingTable::build(
        [
            (
                GOLDEN_AGENT.to_string(),
                ACCT_WITH_KEY.to_string(),
                base.clone(),
                Some(SecretKey::new(ROW_KEY)),
                AuthStyle::DEFAULT,
            ),
            (
                GOLDEN_AGENT.to_string(),
                ACCT_NO_KEY.to_string(),
                base.clone(),
                None,
                AuthStyle::DEFAULT,
            ),
        ]
        .into_iter(),
    );
    // ⚠ 走的是**生产段那条真实的上游选择**（`Accounts`），不是判据自己造的一个假 `Destinations`。
    let relay = Arc::new(Relay::new(
        Arc::new(Accounts::new(
            table,
            crate::accounts::upstream::Upstreams::from_env(&|_| None).expect("内置默认"),
        )),
        super::door::Key::for_tests(),
        TeeSink::to_port(Arc::new(FramingTap(Arc::clone(&buf), Mutex::new(tick)))),
        DOWNSTREAM_DEADLINE,
        UPSTREAM_DEADLINE,
    ));
    let listener = listen(0).expect("listen");
    let addr = listener.local_addr().expect("中转地址");
    std::thread::spawn(move || serve(listener, relay, Default::default()));
    (addr, TeeTap { buf, rx })
}

/// 打一发，把下游收到的**全部字节**读回来。
fn one_shot(relay: SocketAddr, target: &str) -> String {
    let mut c = TcpStream::connect(relay).expect("连中转");
    c.set_nodelay(true).expect("nodelay");
    // 期限：把「挂住」换成「红」。回环上这几发都在毫秒量级。
    c.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .expect("读期限");
    let req = format!(
        // 〔RK1〕过门：钥匙段挂在最前、`Host` 用回环字面量。金标准比的是**上游那一侧**收到的字节
        //   与下游拿回的字节 —— 钥匙段在门里就被剥掉，所以期望一个字节都不用动（这正是「钥匙不上游」的一格）。
        "POST /{}{target} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {CLIENT_TOKEN}\r\nContent-Length: {}\r\n\r\n{CLIENT_BODY}",
        super::door::door_tests::TEST_KEY,
        CLIENT_BODY.len()
    );
    c.write_all(req.as_bytes()).expect("写请求");
    c.flush().expect("flush");
    let mut got = Vec::new();
    c.read_to_end(&mut got).expect("读下游响应");
    String::from_utf8(got).expect("下游响应不是 utf8")
}

/// ★★★ **零行为变化的正主**：三条线上的字节与 [`GOLDEN`] 那张手写表逐字节相等。
#[test]
fn the_bytes_on_all_three_wires_match_the_golden_table_verbatim() {
    let mut checked = 0usize;
    let mut reached_upstream = 0usize;

    for g in GOLDEN {
        // 每一格**各起一套**：tee 的 `resp` 从 0 开始、上游那本账也干净 ⇒ 期望值写得死。
        let stub = spawn_stub();
        let (relay, tee) = spawn_relay(stub.addr);
        let port = stub.addr.port();
        let norm = |s: String| s.replace(&format!("127.0.0.1:{port}"), UPSTREAM_HOST);

        let down = one_shot(relay, g.target);
        assert_eq!(
            down, g.downstream,
            "【{}】下游收到的字节与金标准不同 —— 「零行为变化」这句话在这一格上是假的",
            g.target
        );

        match g.upstream_head {
            Some(want) => {
                // tee 那两件（事件 ＋ 收尾）在转发收工时才交齐 ⇒ 先等它交够，再读，不许拿「还没交齐」当「空」。
                tee.wait_lines(2);
                let heads = stub.heads.lock().expect("lock").clone();
                assert_eq!(heads.len(), 1, "【{}】上游必须**恰好**收到一发", g.target);
                assert_eq!(
                    norm(heads[0].clone()),
                    want,
                    "【{}】中转发给上游的请求头与金标准不同",
                    g.target
                );
                let bodies = stub.bodies.lock().expect("lock").clone();
                assert_eq!(
                    bodies[0], CLIENT_BODY,
                    "【{}】请求体没有逐字节到上游",
                    g.target
                );
                reached_upstream += 1;
            }
            None => {
                // ⚠ 这一支是「零出现」，天生有空真风险 ——
                //   垫它的是同一张表里 `Some(...)` 那两格（下面那条相等断言数着它们）。
                let heads = stub.heads.lock().expect("lock").clone();
                assert!(
                    heads.is_empty(),
                    "【{}】这一发不该有任何字节到上游，实得：{heads:?}",
                    g.target
                );
            }
        }

        assert_eq!(
            tee.text(),
            g.tee,
            "【{}】tee 落点上的字节与金标准不同",
            g.target
        );
        checked += 1;
    }

    // ★ 反空真：跑过的格数 / 真到上游的格数，都与**显式登记的数**相等。
    assert_eq!(
        checked, GOLDEN_CASES,
        "跑过的格数与登记的条数对不上 —— 这把尺子的人群被改动过"
    );
    assert_eq!(
        reached_upstream, GOLDEN_CASES_REACHING_UPSTREAM,
        "真把字节送到上游的格数变了 —— `upstream_head: None` 那几格就成了空真"
    );
}
