//! 「测试连接：后端组拨号请求、拨一次、回结局（后端持有全部 SSH）」——
//! `dial/probe.rs` 的三步各自的结局（链路那一侧用替身）：握手不成 · 通了没 hello · 通了有 hello、控制通道往返 · 到点没等到。
//! 〔「进度不许倒退」〕进度边拨边推：每条判据同时钉**推了哪几格、什么顺序、每格那一句与四段**（结局是最后一格）。
use super::*;
use crate::platform::child::{Deadline, Until};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

fn args() -> Value {
    json!({"ticket": "t-1", "machine": {"host": "192.0.2.2", "label": "devbox", "user": "u"}})
}

/// 跑一趟探针（发起方没带期限），收下它推的每一格（票都得是交来的那张）；回 `(结局那一格, 结局之前那几格)`。
async fn cells_of<F, Fut>(args: &Value, serve: F) -> (Value, Vec<Value>)
where
    F: FnOnce(crate::dial::DialRequest, tokio::io::DuplexStream, tokio::io::DuplexStream) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    cells_until(args, None, serve).await
}

/// 同 [`cells_of`]，带分派那一层交来的截止时刻。
async fn cells_until<F, Fut>(args: &Value, until: Option<Until>, serve: F) -> (Value, Vec<Value>)
where
    F: FnOnce(crate::dial::DialRequest, tokio::io::DuplexStream, tokio::io::DuplexStream) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    probe_with(&crate::wire::typed::take(args).unwrap(), until, serve, &tx)
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
            other => panic!("探针只该推进度格：{other:?}"),
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

#[tokio::test]
async fn a_refused_handshake_reports_ssh_down_with_its_stages_and_no_fingerprint() {
    let (r, cells) = cells_of(&args(), |req, _up, mut down| async move {
        assert!(req.probe && req.stages, "测试连接是短命探活、要阶段行");
        let _ = down.write_all(b"{\"stage\":{\"kind\":\"dialing\",\"endpoint\":\"192.0.2.2:22\"}}\n").await;
        let _ = down
            .write_all(b"{\"v\":2,\"ok\":false,\"error\":\"host key mismatch\",\"detail\":\"cmd: dial\\nraw: fixture-raw-1\",\"fingerprint\":\"SHA256:x\",\"uses\":[]}\n")
            .await;
    })
    .await;
    assert_eq!(r["tone"], "fail");
    assert_eq!(
        r["segs"],
        segs_of(["fail", "wait", "wait", "wait"]),
        "第一处没过的那一段打 ✗"
    );
    assert!(r["said"].as_str().unwrap().contains("host key mismatch"));
    assert_eq!(
        r["end"]["detail"], "cmd: dial\nraw: fixture-raw-1",
        "拨号 ack 带的复制详情原样进结局（句子里不接原话，原话在这里）"
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
        "握手那几行逐条推成一格、带写好的那一句；没过握手 ⇒ 没有「已连上」那一格"
    );
}

/// 握手的每一种阶段行都有那一句与四段（阶段码 ⇒ 字只在这里，界面照画）。
#[tokio::test]
async fn every_handshake_stage_is_a_written_line() {
    let (r, cells) = cells_of(&args(), |_req, _up, mut down| async move {
        for st in [
            json!({"kind": "dialing", "endpoint": "h:22"}),
            json!({"kind": "failed", "endpoint": "h:22", "reason": "超时-甲"}),
            json!({"kind": "hostKey", "endpoint": "h2:22", "fingerprint": "SHA256:x"}),
            json!({"kind": "won", "endpoint": "h2:22"}),
            json!({"kind": "auth", "ok": false, "detail": "拒绝-乙"}),
        ] {
            let _ = down
                .write_all(format!("{}\n", json!({"stage": st})).as_bytes())
                .await;
        }
        let _ = down
            .write_all(b"{\"v\":2,\"ok\":false,\"error\":\"e\",\"uses\":[]}\n")
            .await;
    })
    .await;
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

#[tokio::test]
async fn a_passed_login_lights_three_segments() {
    let (_, cells) = cells_of(&args(), |_req, _up, mut down| async move {
        let _ = down
            .write_all(b"{\"stage\":{\"kind\":\"auth\",\"ok\":true,\"detail\":null}}\n{\"stage\":{\"kind\":\"established\"}}\n")
            .await;
    })
    .await;
    assert_eq!(
        cells,
        vec![
            step(&copy_text("beProbe.stage.authOk", &[]), "now", SSH_UP),
            step(&copy_text("beProbe.stage.ready", &[]), "now", SSH_UP),
        ]
    );
}

/// 没带期限 ⇒ 按登记的上限说。
fn cap_said() -> String {
    copy_core::format_duration(PROBE_CAP.shown_secs() * 1000)
}

/// 发起方带了期限 ⇒ 按它执行，到点那一格说的是带来的那个数。
#[tokio::test(start_paused = true)]
async fn a_stall_within_the_callers_deadline_says_that_deadline() {
    let until = Until::after(Deadline::millis(4_000));
    let (r, _) = cells_until(&args(), until, |_req, up, down| async move {
        let _held = (up, down);
        std::future::pending::<()>().await
    })
    .await;
    let last = copy_text("beProbe.stage.notStarted", &[]);
    assert_eq!(
        r["said"],
        copy_text(
            "beProbe.test.stalled",
            &[("dur", &copy_core::format_duration(4_000)), ("last", &last)]
        )
    );
    assert_ne!(
        cap_said(),
        copy_core::format_duration(4_000),
        "两个数一样就分不出照的是哪个"
    );
}

/// 到点没等到结局：后端自己收掉那一趟，结局那一格说等了多久、最后一步是哪一句；没亮的第一段打 ✗。
#[tokio::test(start_paused = true)]
async fn a_stalled_probe_says_where_it_stopped() {
    let last = copy_text("beProbe.stage.dial", &[("endpoint", "192.0.2.2:22")]);
    let (r, cells) = cells_of(&args(), |_req, _up, mut down| async move {
        let _ = down
            .write_all(b"{\"stage\":{\"kind\":\"dialing\",\"endpoint\":\"192.0.2.2:22\"}}\n")
            .await;
        std::future::pending::<()>().await;
    })
    .await;
    assert_eq!(cells, vec![step(&last, "now", ["wait"; 4])]);
    let dur = cap_said();
    assert_eq!(
        r["said"],
        copy_text("beProbe.test.stalled", &[("dur", &dur), ("last", &last)])
    );
    assert_eq!(
        (r["tone"].as_str(), r["segs"].clone()),
        (Some("fail"), segs_of(["fail", "wait", "wait", "wait"]))
    );
    assert_eq!(
        r["end"],
        json!({"backendHello": null, "backendGaps": [], "detail": null})
    );
}

#[tokio::test(start_paused = true)]
async fn a_probe_with_no_progress_at_all_says_it_never_started() {
    let (r, cells) = cells_of(&args(), |_req, up, down| async move {
        let _held = (up, down);
        std::future::pending::<()>().await
    })
    .await;
    assert_eq!(cells, Vec::<Value>::new());
    let dur = cap_said();
    let last = copy_text("beProbe.stage.notStarted", &[]);
    assert_eq!(
        r["said"],
        copy_text("beProbe.test.stalled", &[("dur", &dur), ("last", &last)])
    );
}

#[tokio::test(start_paused = true)]
async fn a_probe_stalled_after_ssh_says_it_waits_for_the_backend() {
    let (r, cells) = cells_of(&args(), |_req, _up, mut down| async move {
        let _ = down
            .write_all(b"{\"v\":2,\"ok\":true,\"uses\":[\"stream\"]}\n")
            .await;
        std::future::pending::<()>().await;
    })
    .await;
    assert_eq!(cells, vec![reached_ssh()]);
    let dur = cap_said();
    let last = copy_text("beProbe.reached.ssh", &[]);
    assert_eq!(
        r["said"],
        copy_text("beProbe.test.stalled", &[("dur", &dur), ("last", &last)])
    );
    assert_eq!(r["segs"], segs_of(["ok", "ok", "ok", "fail"]));
}

#[tokio::test]
async fn ssh_up_but_no_hello_is_its_own_verdict() {
    let (r, cells) = cells_of(&args(), |_req, _up, mut down| async move {
        let _ = down
            .write_all(b"{\"v\":2,\"ok\":true,\"fingerprint\":\"SHA256:k\",\"endpoint\":\"192.0.2.2:22\",\"uses\":[\"stream\"]}\nnot json\n")
            .await;
    })
    .await;
    assert_eq!(
        cells,
        vec![reached_ssh()],
        "握手过了、hello 没到 ⇒ 只走完 ssh 那一段"
    );
    assert_eq!(r["segs"], segs_of(["ok", "ok", "ok", "fail"]));
    assert_eq!(r["tone"], "fail");
    assert_eq!(r["said"], copy_text("beProbe.test.noHello", &[]));
}

/// 认形状 ＋ 机器页「测试连接」那一行不露 `v=… build=… caps=[…]` 日志行，
/// 露三格（版本 · 能用几项 / 做不到几项 · 往返毫秒），做不到的那几类按码分类交给界面；键值对那一形只进日志。
fn is_not_a_log_line(s: &str) -> bool {
    !s.contains('=') && !s.contains('[')
}

#[tokio::test]
async fn a_hello_that_answers_ping_is_all_green() {
    let (r, cells) = cells_of(&args(), |_req, up, mut down| async move {
        let _ = down.write_all(b"{\"v\":2,\"ok\":true,\"uses\":[\"stream\"]}\n").await;
        let _ = down
            .write_all(b"{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b1\",\"host_arch\":\"x86_64\",\"claude_dir\":\"/h/.claude\",\"capabilities\":[\"stream\"],\"commands\":[\"ping\",\"kill\",\"files-chmod\"],\"unavailable\":[{\"command\":\"kill\",\"code\":\"no_tmux\"}]}\n")
            .await;
        // 真去读那一问、按它的 id 回（证明往返真走了这条流）。
        let mut lines = tokio::io::BufReader::new(up).lines();
        let asked: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(asked["cmd"], "ping");
        let reply = json!({"kind": "reply", "id": asked["id"], "ok": true});
        let _ = down.write_all(format!("{reply}\n").as_bytes()).await;
    })
    .await;
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
    let r = &r["end"];
    let hello = r["backendHello"].as_str().unwrap();
    // 往返时长 · 版本 b1 · 做不到 1 项（kill）（时长是现量的，只核以数起头）。
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
    // 做不到的按码分类交出去，每类带那一句（与置灰那一句同一个家 `copy_core::unavailable`，称呼那台用表单里的名字）。
    assert_eq!(
        r["backendGaps"],
        json!([{"code": "no_tmux", "count": 1, "said": copy_core::unavailable("no_tmux", "devbox")}])
    );
    assert!(is_not_a_log_line(hello), "机器页那一行又是日志行：{hello}");
}

#[tokio::test]
async fn an_old_backend_without_commands_says_too_old() {
    let (tx, rx) = tokio::sync::oneshot::channel::<tokio::io::DuplexStream>();
    let (r, cells) = cells_of(&args(), |_req, up, mut down| async move {
        let _ = down
            .write_all(
                b"{\"v\":2,\"ok\":true,\"uses\":[\"stream\"]}\n{\"kind\":\"hello\",\"v\":1}\n",
            )
            .await;
        // 上行读端交给判据自己读（本任务随探针返回被收，读端不能跟着它没了）；下行随本任务收尾关掉
        // ⇒ 探针若错发了 ping 也等不住（读到 EOF、落「不能起会话」），不会挂住判据。
        let _ = tx.send(up);
    })
    .await;
    assert_eq!(cells, vec![reached_ssh(), reached_hello()]);
    let mut up = rx.await.unwrap();
    let mut got = Vec::new();
    let _ = tokio::io::AsyncReadExt::read_to_end(&mut up, &mut got).await;
    assert!(
        got.is_empty(),
        "对旧后端发了字节：{:?}",
        String::from_utf8_lossy(&got)
    );
    let said = copy_core::backend_old("devbox");
    assert_eq!(r["said"], json!(said));
    assert_eq!(
        (r["tone"].as_str(), r["segs"].clone()),
        (Some("warn"), segs_of(["ok"; 4])),
        "hello 到了 ⇒ 就绪那一段算过"
    );
    let r = &r["end"];
    let unknown = copy_text("beProbe.hello.noBuild", &[]);
    assert_eq!(
        r["backendHello"],
        json!(copy_text(
            "beProbe.hello.tooOld",
            &[("build", &unknown), ("said", &said)]
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
        let r = match crate::wire::typed::take::<ProbeReq>(&a) {
            Err(f) => Err(f.code),
            Ok(req) => probe_with(
                &req,
                None,
                |_req, _up, _down| async move { panic!("票不对就不该拨") },
                &tx,
            )
            .await
            .map_err(|(c, _)| c.to_string()),
        };
        assert_eq!(r, Err("bad_args".to_string()), "票 {bad} 该拒");
    }
    drop(tx);
    assert!(rx.recv().await.is_none(), "拒了就一格都不推");
}

/// 通了、hello 也到了，ping 那一问回了「不行」：那台的原话进复制详情，不进那一句（`backendHello` 只留版本与「无应答」）。
#[tokio::test]
async fn a_ping_refusal_keeps_its_raw_words_out_of_the_sentence() {
    let (r, _) = cells_of(&args(), |_req, up, mut down| async move {
        let _ = down
            .write_all(b"{\"v\":2,\"ok\":true,\"uses\":[\"stream\"]}\n")
            .await;
        let _ = down
            .write_all(
                b"{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b1\",\"commands\":[\"ping\"]}\n",
            )
            .await;
        let mut lines = tokio::io::BufReader::new(up).lines();
        let asked: Value =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        let reply =
            json!({"kind": "reply", "id": asked["id"], "ok": false, "message": "fixture-raw-2"});
        let _ = down.write_all(format!("{reply}\n").as_bytes()).await;
    })
    .await;
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

/// 通了，读那台后端的首行就出错（一行超长）：那一句只说「后端探测失败」，读错的原话进复制详情。
#[tokio::test]
async fn a_broken_first_line_keeps_its_raw_words_out_of_the_sentence() {
    let (r, _) = cells_of(&args(), |_req, _up, mut down| async move {
        let _ = down
            .write_all(b"{\"v\":2,\"ok\":true,\"uses\":[\"stream\"]}\n")
            .await;
        let _ = down.write_all(&vec![b'x'; 1024 * 1024 + 16]).await;
        let _ = down.write_all(b"\n").await;
    })
    .await;
    assert_eq!(r["said"], copy_text("beProbe.test.probeFailed", &[]));
    let detail = r["end"]["detail"].as_str().unwrap();
    assert!(!detail.is_empty(), "{r}");
}

/// 都过了 ⇒ 没有复制详情（成功不出那一格的值）。
#[tokio::test]
async fn an_all_green_probe_has_no_detail() {
    let (r, _) = cells_of(&args(), |_req, up, mut down| async move {
        let _ = down
            .write_all(b"{\"v\":2,\"ok\":true,\"uses\":[\"stream\"]}\n")
            .await;
        let _ = down
            .write_all(
                b"{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b1\",\"commands\":[\"ping\"]}\n",
            )
            .await;
        let mut lines = tokio::io::BufReader::new(up).lines();
        let asked: Value =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        let reply = json!({"kind": "reply", "id": asked["id"], "ok": true});
        let _ = down.write_all(format!("{reply}\n").as_bytes()).await;
    })
    .await;
    assert_eq!(r["said"], copy_text("beProbe.test.ok", &[]));
    assert_eq!(r["end"]["detail"], Value::Null);
}
