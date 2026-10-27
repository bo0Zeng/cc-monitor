//! 「测试连接走真连接那条路」—— `dial/probe.rs` 拿表单那份配置跑一趟连常驻那一层（`resident-client` 的常驻流，真连接同一串）：
//! attach → 没接上 ensure → 再 attach；接上了问一次 `ping`。拨号那一侧用替身（[`Fake`]：开长 exec 照排好的那一形回 · 收全照排好的回），
//! 替身只认 `--resident-attach` / `--resident-ensure` 两个词 ⇒ 测试连接走的不是那条路就红。
//! 〔「进度不许倒退」〕每条判据同时钉**推了哪几格、什么顺序、每格那一句与四段**（结局是最后一格）。
use super::*;
use crate::dial::Stage;
use crate::platform::child::{Deadline, Until};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, DuplexStream};

fn args() -> Value {
    json!({"ticket": "t-1", "machine": {"host": "192.0.2.2", "label": "devbox", "user": "u"}})
}

/// 开一条长 exec 时那台怎么回。
enum Open {
    /// 拨号核心报这几步，然后没开成。
    Refused(Vec<Stage>, ssh_dial::DialFail),
    /// 拨号核心报这几步，然后一直挂着（开不出来、也不回话）。
    Hang(Vec<Stage>),
    /// 开成了：那台那一侧照 [`Peer`] 回。
    Up(Peer),
}

/// 开成之后那台怎么回 attach。
#[derive(Clone)]
enum Peer {
    /// 一个字节都不回，也不断。
    Silent,
    /// 小中继说常驻还没绑上口。
    Absent,
    /// 第一行不是 hello。
    NotOurs,
    /// 回 hello（这几条命令 · 做不到的那几项）、回「可以」；之后 `ping` 照 `ping` 答（`None` ⇒ 答应）。
    Serve {
        hello: String,
        ping: Option<&'static str>,
    },
}

/// 拨号替身：按序取排好的那几形；记下每一趟 exec 的是哪个词。
struct Fake {
    opens: Mutex<VecDeque<Open>>,
    ensure: Mutex<Option<Captured>>,
    words: Mutex<Vec<String>>,
}

impl Fake {
    fn new(opens: Vec<Open>) -> Arc<Fake> {
        Arc::new(Fake {
            opens: Mutex::new(opens.into()),
            ensure: Mutex::new(None),
            words: Mutex::new(Vec::new()),
        })
    }

    fn ensure_says(self: Arc<Self>, c: Captured) -> Arc<Self> {
        *self.ensure.lock().unwrap() = Some(c);
        self
    }

    fn words(&self) -> Vec<String> {
        self.words.lock().unwrap().clone()
    }

    /// 那台那一侧的后端词（`… -- --resident-attach` ⇒ `--resident-attach`）。只认真连接那两个词。
    fn word_of(&self, req: &crate::dial::DialRequest) -> String {
        let w = req
            .command
            .split(" -- ")
            .nth(1)
            .unwrap_or_else(|| panic!("不是后端落点上的一条子命令：{}", req.command))
            .to_string();
        assert!(
            w == "--resident-attach" || w == "--resident-ensure",
            "测试连接走的不是真连接那条路：exec 了 `{w}`"
        );
        self.words.lock().unwrap().push(w.clone());
        w
    }
}

impl Dial for Fake {
    fn open(
        &self,
        req: crate::dial::DialRequest,
        stages: StageTx,
        _progress: Progress,
    ) -> resident_client::feed::Boxed<'_, Result<Pipe, ssh_dial::DialFail>> {
        assert!(req.probe && req.stages, "测试连接是短命探活、要阶段");
        let w = self.word_of(&req);
        assert_eq!(w, "--resident-attach", "长 exec 只开 attach");
        let plan = self
            .opens
            .lock()
            .unwrap()
            .pop_front()
            .expect("多开了一条 attach");
        Box::pin(async move {
            match plan {
                Open::Refused(st, f) => {
                    st.into_iter().for_each(|s| stages.send(s).unwrap());
                    Err(f)
                }
                Open::Hang(st) => {
                    st.into_iter().for_each(|s| stages.send(s).unwrap());
                    std::future::pending().await
                }
                Open::Up(peer) => {
                    let (up_w, up_r) = tokio::io::duplex(64 * 1024);
                    let (down_w, down_r) = tokio::io::duplex(256 * 1024);
                    tokio::spawn(serve(peer, up_r, down_w));
                    Ok(tokio::io::join(down_r, up_w))
                }
            }
        })
    }

    fn capture(
        &self,
        req: crate::dial::DialRequest,
    ) -> resident_client::feed::Boxed<'_, Result<Captured, ssh_dial::DialFail>> {
        assert!(req.probe, "测试连接是短命探活");
        assert_eq!(self.word_of(&req), "--resident-ensure", "收全只跑 ensure");
        let said = self
            .ensure
            .lock()
            .unwrap()
            .clone()
            .expect("没排 ensure 的回话");
        Box::pin(async move { Ok(said) })
    }
}

/// 那台那一侧：读 attach 行，照 `peer` 回。
async fn serve(peer: Peer, up: DuplexStream, mut down: DuplexStream) {
    let mut lines = tokio::io::BufReader::new(up).lines();
    let Ok(Some(attach)) = lines.next_line().await else {
        return;
    };
    let attach: Value = serde_json::from_str(&attach).unwrap();
    assert_eq!(attach["attach"], true, "第一行是 attach 行：{attach}");
    assert_eq!(attach["mine"], crate::BUILD_ID, "attach 行带手上这一版");
    match peer {
        Peer::Silent => {
            let _held = (lines, down);
            std::future::pending::<()>().await;
        }
        Peer::Absent => {
            let _ = down
                .write_all(b"{\"attach\":\"refused\",\"reason\":\"absent\"}\n")
                .await;
        }
        Peer::NotOurs => {
            let _ = down.write_all(b"not json\n").await;
        }
        Peer::Serve { hello, ping } => {
            let _ = down.write_all(format!("{hello}\n").as_bytes()).await;
            let _ = down
                .write_all(b"{\"attach\":\"ok\",\"relation\":\"same\"}\n")
                .await;
            while let Ok(Some(l)) = lines.next_line().await {
                let asked: Value = serde_json::from_str(&l).unwrap();
                assert_eq!(asked["cmd"], "ping", "测试连接只问 ping：{asked}");
                let reply = match ping {
                    None => json!({"kind": "reply", "id": asked["id"], "ok": true}),
                    Some(m) => {
                        json!({"kind": "reply", "id": asked["id"], "ok": false, "code": "failed", "message": m})
                    }
                };
                let _ = down.write_all(format!("{reply}\n").as_bytes()).await;
            }
        }
    }
}

/// 一行 hello：版本 · 认的命令（连形状指纹）· 做不到的那几项。
fn hello(build: &str, cmds: &[&str], unavailable: Value) -> String {
    let shapes: serde_json::Map<String, Value> = cmds
        .iter()
        .filter_map(|c| Some((c.to_string(), json!(cmd_wire::shapes::shape_of(c)?))))
        .collect();
    let frames: serde_json::Map<String, Value> = cmd_wire::frame_shapes::FRAMES
        .iter()
        .map(|(k, h)| (k.to_string(), json!(h)))
        .collect();
    json!({
        "kind": "hello", "build_id": build, "host_arch": "x86_64",
        "commands": cmds, "unavailable": unavailable, "shapes": shapes, "frames": frames,
    })
    .to_string()
}

fn serving(ping: Option<&'static str>) -> Open {
    Open::Up(Peer::Serve {
        hello: hello("b1", &["ping"], json!([])),
        ping,
    })
}

/// 跑一趟测试连接（发起方没带期限），收下它推的每一格（票都得是交来的那张）；回 `(结局那一格, 结局之前那几格)`。
async fn cells_of(args: &Value, dial: &Arc<Fake>) -> (Value, Vec<Value>) {
    cells_until(args, None, dial).await
}

/// 同 [`cells_of`]，带分派那一层交来的截止时刻。
async fn cells_until(args: &Value, until: Option<Until>, dial: &Arc<Fake>) -> (Value, Vec<Value>) {
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let d: Arc<dyn Dial> = Arc::clone(dial) as Arc<dyn Dial>;
    probe_with(&crate::wire::typed::take(args).unwrap(), until, d, &tx)
        .await
        .unwrap();
    drop(tx);
    let mut cells = Vec::new();
    while let Some(f) = rx.recv().await {
        match f {
            Frame::Probe(crate::wire::Probe { ticket, cell }) => {
                assert_eq!(ticket, "t-1", "进度格要带交来的那张票");
                assert!(
                    cell["said"].as_str().is_some_and(|s| !s.is_empty()),
                    "每一格都带写好的那一句：{cell}"
                );
                cells.push(cell);
            }
            other => panic!("测试连接只该推进度格：{other:?}"),
        }
    }
    let end = cells.pop().expect("至少有结局那一格");
    assert!(end["end"].is_object(), "最后一格带结局：{end}");
    assert!(
        cells.iter().all(|c| c["end"].is_null()),
        "结局只在最后一格：{cells:?}"
    );
    (end, cells)
}

/// 没开成：拨号核心写好的那一句 · 复制详情 · 看到的指纹。
fn refused(error: &str, detail: Option<&str>, fingerprint: Option<&str>) -> ssh_dial::DialFail {
    ssh_dial::DialFail {
        error: error.into(),
        detail: detail.map(str::to_string),
        fingerprint: fingerprint.map(str::to_string),
        reason: None,
        key_type: None,
        expected_fingerprint: None,
    }
}

/// 一格进度（没有结局）：那一句 · 语气 · 四段（解析 · 连接 · 认证 · 就绪）。
fn step(said: &str, tone: &str, segs: [&str; 4]) -> Value {
    json!({"said": said, "tone": tone, "segs": segs_of(segs), "end": null})
}

fn segs_of([resolve, connect, auth, ready]: [&str; 4]) -> Value {
    json!({"resolve": resolve, "connect": connect, "auth": auth, "ready": ready})
}

const SSH_UP: [&str; 4] = ["ok", "ok", "ok", "wait"];

fn reached_ssh() -> Value {
    step(&copy_text("beProbe.reached.ssh", &[]), "now", SSH_UP)
}

fn reached_hello() -> Value {
    step(&copy_text("beProbe.reached.hello", &[]), "now", SSH_UP)
}

#[tokio::test(start_paused = true)]
async fn a_refused_handshake_reports_ssh_down_with_its_stages_and_no_fingerprint() {
    let fake = Fake::new(vec![Open::Refused(
        vec![Stage::Dialing {
            endpoint: "192.0.2.2:22".into(),
        }],
        refused(
            "host key mismatch",
            Some("cmd: dial\nraw: fixture-raw-1"),
            Some("SHA256:x"),
        ),
    )]);
    let (r, cells) = cells_of(&args(), &fake).await;
    assert_eq!(r["tone"], "fail");
    assert_eq!(
        r["segs"],
        segs_of(["fail", "wait", "wait", "wait"]),
        "第一处没过的那一段打 ✗"
    );
    assert!(r["said"].as_str().unwrap().contains("host key mismatch"));
    assert_eq!(
        r["end"]["detail"], "cmd: dial\nraw: fixture-raw-1",
        "拨号核心带的复制详情原样进结局（句子里不接原话，原话在这里）"
    );
    assert!(
        r["end"].get("fingerprint").is_none(),
        "失败时不回指纹（免得被固化）"
    );
    assert_eq!(
        cells,
        vec![step(
            &copy_text("beProbe.stage.dial", &[("endpoint", "192.0.2.2:22")]),
            "now",
            ["wait"; 4]
        )],
        "握手那几步逐条推成一格、带写好的那一句；没过握手 ⇒ 没有「已连上」那一格"
    );
    assert_eq!(
        fake.words(),
        vec!["--resident-attach"],
        "链路开不出来 ⇒ 不 ensure"
    );
}

/// 握手的每一种阶段都有那一句与四段（阶段码 ⇒ 字只在这里，界面照画）。
#[tokio::test(start_paused = true)]
async fn every_handshake_stage_is_a_written_line() {
    let fake = Fake::new(vec![Open::Refused(
        vec![
            Stage::Dialing {
                endpoint: "h:22".into(),
            },
            Stage::Failed {
                endpoint: "h:22".into(),
                reason: "超时-甲".into(),
            },
            Stage::HostKey {
                endpoint: "h2:22".into(),
                fingerprint: "SHA256:x".into(),
            },
            Stage::Won {
                endpoint: "h2:22".into(),
            },
            Stage::Auth {
                ok: false,
                detail: Some("拒绝-乙".into()),
            },
        ],
        refused("e", None, None),
    )]);
    let (r, cells) = cells_of(&args(), &fake).await;
    let up = ["ok", "ok", "wait", "wait"];
    assert_eq!(
        cells,
        vec![
            step(
                &copy_text("beProbe.stage.dial", &[("endpoint", "h:22")]),
                "now",
                ["wait"; 4]
            ),
            step(
                &copy_text(
                    "beProbe.stage.failed",
                    &[("endpoint", "h:22"), ("reason", "超时-甲")]
                ),
                "warn",
                ["wait"; 4]
            ),
            step(
                &copy_text(
                    "beProbe.stage.fingerprint",
                    &[("endpoint", "h2:22"), ("fingerprint", "SHA256:x")]
                ),
                "now",
                up
            ),
            step(
                &copy_text("beProbe.stage.won", &[("endpoint", "h2:22")]),
                "now",
                up
            ),
            step(
                &copy_text("beProbe.stage.authFailed", &[("said", "拒绝-乙")]),
                "fail",
                ["ok", "ok", "fail", "wait"]
            ),
        ]
    );
    assert_eq!(
        r["segs"],
        segs_of(["ok", "ok", "fail", "wait"]),
        "已有 ✗ 的不再多打一处"
    );
}

#[tokio::test(start_paused = true)]
async fn a_passed_login_lights_three_segments() {
    let fake = Fake::new(vec![Open::Refused(
        vec![
            Stage::Auth {
                ok: true,
                detail: None,
            },
            Stage::Established,
        ],
        refused("e", None, None),
    )]);
    let (_, cells) = cells_of(&args(), &fake).await;
    assert_eq!(
        cells,
        vec![
            step(&copy_text("beProbe.stage.authOk", &[]), "now", SSH_UP),
            step(&copy_text("beProbe.stage.ready", &[]), "now", SSH_UP),
        ]
    );
}

/// 发起方带了期限 ⇒ 按它执行，到点那一格说的是带来的那个数。
#[tokio::test(start_paused = true)]
async fn a_stall_within_the_callers_deadline_says_that_deadline() {
    let fake = Fake::new(vec![Open::Hang(vec![])]);
    let until = Until::after(Deadline::millis(4_000));
    let (r, _) = cells_until(&args(), until, &fake).await;
    let last = copy_text("beProbe.stage.notStarted", &[]);
    assert_eq!(
        r["said"],
        copy_text(
            "beProbe.test.stalled",
            &[("dur", &copy_core::format_duration(4_000)), ("last", &last)]
        )
    );
    assert_ne!(
        copy_core::format_duration(PROBE_CAP.shown_secs() * 1000),
        copy_core::format_duration(4_000),
        "两个数一样就分不出照的是哪个"
    );
}

/// 拨出去一直没人回话：与真连接同一个判法（拨号期限里一步进展都没有 ⇒ 没够着那一台），那一句照真连接那一句。
#[tokio::test(start_paused = true)]
async fn a_dial_nobody_answers_says_the_machine_is_unreached() {
    let fake = Fake::new(vec![Open::Hang(vec![Stage::Dialing {
        endpoint: "192.0.2.2:22".into(),
    }])]);
    let (r, cells) = cells_of(&args(), &fake).await;
    let last = copy_text("beProbe.stage.dial", &[("endpoint", "192.0.2.2:22")]);
    assert_eq!(cells, vec![step(&last, "now", ["wait"; 4])]);
    let offline = comms_inward::chan::said::offline("devbox");
    assert_eq!(
        r["said"],
        copy_text("beProbe.test.sshFailed", &[("why", &offline)])
    );
    assert_eq!(
        (r["tone"].as_str(), r["segs"].clone()),
        (Some("fail"), segs_of(["fail", "wait", "wait", "wait"]))
    );
    assert_eq!(r["end"]["backendHello"], Value::Null);
}

/// 链路开成了、那台一个字节都不回：到点那一格说最后一步是「SSH 已连上」，就绪那一段打 ✗。
#[tokio::test(start_paused = true)]
async fn a_probe_stalled_after_ssh_says_it_waits_for_the_backend() {
    let fake = Fake::new(vec![Open::Up(Peer::Silent)]);
    let (r, cells) = cells_of(&args(), &fake).await;
    assert_eq!(cells, vec![reached_ssh()]);
    let dur = copy_core::format_duration(PROBE_CAP.shown_secs() * 1000);
    let last = copy_text("beProbe.reached.ssh", &[]);
    assert_eq!(
        r["said"],
        copy_text("beProbe.test.stalled", &[("dur", &dur), ("last", &last)])
    );
    assert_eq!(r["segs"], segs_of(["ok", "ok", "ok", "fail"]));
}

/// 第一行不是 hello：与真连接一样先 ensure；ensure 说那台没有后端 ⇒ 结局就是那一句，就绪那一段打 ✗。
#[tokio::test(start_paused = true)]
async fn not_our_backend_ensures_like_the_real_connection_then_says_why() {
    let fake = Fake::new(vec![Open::Up(Peer::NotOurs)]).ensure_says(Captured {
        stdout: String::new(),
        stderr: "sh: ccm: not found".into(),
        exit_status: Some(127),
    });
    let (r, cells) = cells_of(&args(), &fake).await;
    assert_eq!(
        fake.words(),
        vec!["--resident-attach", "--resident-ensure"],
        "走真连接那一串：attach → ensure"
    );
    assert_eq!(cells, vec![reached_ssh()]);
    assert_eq!(r["said"], copy_text("remoteHealth.head.notDeployed", &[]));
    assert_eq!(
        (r["tone"].as_str(), r["segs"].clone()),
        (Some("fail"), segs_of(["ok", "ok", "ok", "fail"]))
    );
    assert!(
        r["end"]["detail"].as_str().unwrap().contains("not found"),
        "原话进复制详情：{r}"
    );
}

/// 认形状 ＋ 机器页「测试连接」那一行不露 `v=… build=… caps=[…]` 日志行，
/// 露三格（版本 · 能用几项 / 做不到几项 · 往返毫秒），做不到的那几类按码分类交给界面；键值对那一形只进日志。
fn is_not_a_log_line(s: &str) -> bool {
    !s.contains('=') && !s.contains('[')
}

#[tokio::test(start_paused = true)]
async fn a_hello_that_answers_ping_is_all_green() {
    let fake = Fake::new(vec![Open::Up(Peer::Serve {
        hello: hello(
            "b1",
            &["ping", "kill", "files-chmod"],
            json!([{"command": "kill", "code": "no_tmux"}]),
        ),
        ping: None,
    })]);
    let (r, cells) = cells_of(&args(), &fake).await;
    assert_eq!(
        cells,
        vec![reached_ssh(), reached_hello()],
        "两段按序各一格，结局最后"
    );
    assert_eq!(
        (r["tone"].as_str(), r["segs"].clone()),
        (Some("plain"), segs_of(["ok"; 4]))
    );
    assert_eq!(r["said"], copy_text("beProbe.test.ok", &[]));
    assert_eq!(r["end"]["detail"], Value::Null, "都过了 ⇒ 没有复制详情");
    let r = &r["end"];
    let hello = r["backendHello"].as_str().unwrap();
    let want = copy_text(
        "beProbe.hello.okGaps",
        &[("build", "b1"), ("gaps", "1"), ("dur", "\u{0}")],
    );
    let (pre, post) = want.split_once('\u{0}').unwrap();
    let dur = hello
        .strip_prefix(pre)
        .and_then(|t| t.strip_suffix(post))
        .unwrap_or_else(|| panic!("不是那三格：{hello}"));
    assert!(dur.starts_with(|c: char| c.is_ascii_digit()), "{hello}");
    assert_eq!(
        r["backendGaps"],
        json!([{"code": "no_tmux", "count": 1, "said": copy_core::unavailable("no_tmux", "devbox")}])
    );
    assert!(is_not_a_log_line(hello), "机器页那一行又是日志行：{hello}");
    assert_eq!(fake.words(), vec!["--resident-attach"]);
}

/// 常驻还没起（小中继说没人在听）：与真连接一样 ensure 起它、再 attach，接上了照样全绿。
#[tokio::test(start_paused = true)]
async fn an_absent_resident_is_ensured_then_attached() {
    let fake = Fake::new(vec![Open::Up(Peer::Absent), serving(None)]).ensure_says(Captured {
        stdout: "{\"pid\":7,\"replaced\":false}".into(),
        stderr: String::new(),
        exit_status: Some(0),
    });
    let (r, cells) = cells_of(&args(), &fake).await;
    assert_eq!(
        fake.words(),
        vec![
            "--resident-attach",
            "--resident-ensure",
            "--resident-attach"
        ]
    );
    assert_eq!(cells, vec![reached_ssh(), reached_hello()]);
    assert_eq!(r["said"], copy_text("beProbe.test.ok", &[]));
}

#[tokio::test(start_paused = true)]
async fn an_old_backend_without_commands_says_too_old() {
    let fake = Fake::new(vec![Open::Up(Peer::Serve {
        hello: hello("b0", &[], json!([])),
        ping: None,
    })]);
    let (r, cells) = cells_of(&args(), &fake).await;
    assert_eq!(cells, vec![reached_ssh(), reached_hello()]);
    let said = copy_core::backend_old("devbox");
    assert_eq!(r["said"], json!(said));
    assert_eq!(
        (r["tone"].as_str(), r["segs"].clone()),
        (Some("warn"), segs_of(["ok"; 4])),
        "hello 到了 ⇒ 就绪那一段算过"
    );
    let r = &r["end"];
    assert_eq!(
        r["backendHello"],
        json!(copy_text(
            "beProbe.hello.tooOld",
            &[("build", "b0"), ("said", &said)]
        ))
    );
    assert!(is_not_a_log_line(r["backendHello"].as_str().unwrap()));
    assert_eq!(r["backendGaps"], json!([]));
}

#[tokio::test]
async fn a_probe_without_a_valid_ticket_is_refused_before_dialling() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    for bad in [json!(null), json!(""), json!("a b"), json!("x".repeat(65))] {
        let mut a = args();
        a["ticket"] = bad.clone();
        // 不是串的那一形在收入参那一步就拒（同生产：typed 那一处）。
        let fake = Fake::new(vec![]);
        let r = match crate::wire::typed::take::<ProbeReq>(&a) {
            Err(f) => Err(f.code),
            Ok(req) => probe_with(&req, None, Arc::clone(&fake) as Arc<dyn Dial>, &tx)
                .await
                .map_err(|(c, _)| c.to_string()),
        };
        assert_eq!(r, Err("bad_args".to_string()), "票 {bad} 该拒");
        assert!(fake.words().is_empty(), "票不对就不该拨");
    }
    drop(tx);
    assert!(rx.recv().await.is_none(), "拒了就一格都不推");
}

/// 通了、hello 也到了，ping 那一问回了「不行」：那台的原话进复制详情，不进那一句（`backendHello` 只留版本与「无应答」）。
#[tokio::test(start_paused = true)]
async fn a_ping_refusal_keeps_its_raw_words_out_of_the_sentence() {
    let fake = Fake::new(vec![serving(Some("fixture-raw-2"))]);
    let (r, _) = cells_of(&args(), &fake).await;
    assert_eq!(
        (r["said"].clone(), r["tone"].clone()),
        (
            json!(copy_text("beProbe.test.controlDown", &[])),
            json!("warn")
        )
    );
    let r = &r["end"];
    let hello = r["backendHello"].as_str().unwrap();
    assert!(!hello.contains("fixture-raw-2"), "原话进了那一句：{hello}");
    assert!(
        r["detail"].as_str().unwrap().contains("fixture-raw-2"),
        "{r}"
    );
}
