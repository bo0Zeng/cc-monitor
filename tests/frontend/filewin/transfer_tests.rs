//! 一组搬到另一台（上传 · 下载 · 复制到另一台）在窗口这一侧的判据：通道上恰好一条寻址本机的 `transfer-copy`（两头 · 搬哪几项 ·
//! 落到哪 · 带票）＋ 一条 `files-batch/<那张票>` 订阅；回包是一组的结局，「进度」那一行照它落、剩多久照帧里那一句。
//!
//! 对端是合成的（[`OneLine`]，挂在真回环通道口上），记下每一步寻址的那台 · 那一步 · 载荷。

use super::*;

/// 合成对端：记下每一步寻址的那台 · 那一步（`call` 的名字 / `subscribe`）· 载荷。
/// `transfer-copy` 回一组的结局（每一项都成）；订阅推一格带剩多久的快照，之后不收场。
pub(crate) struct OneLine {
    pub(crate) log: std::sync::Arc<std::sync::Mutex<Vec<(String, String, serde_json::Value)>>>,
    /// 给了 ⇒ `transfer-copy` 等它放行再回（看「在跑」那一格用；一直不放 ⇒ 一直在跑）。
    pub(crate) hold: Option<std::sync::Arc<tokio::sync::Notify>>,
    /// `transfer-copy` 没带 `decide` ⇒ 说第一项同名（回 `exists` 带同名清单）；带了 ⇒ 照常回结局。
    pub(crate) clash: bool,
}

impl comms_inward::chan::router::Backends for OneLine {
    fn call(
        &self,
        origin: comms_inward::chan::wire::Origin,
        op: comms_inward::chan::wire::Op,
        payload: comms_inward::chan::wire::Body,
        _view: Option<serde_json::Value>,
        _left: std::time::Duration,
        _cancel: comms_inward::chan::wire::CancelToken,
    ) -> futures::future::BoxFuture<
        'static,
        Result<comms_inward::chan::wire::Body, comms_inward::chan::wire::CallError>,
    > {
        let args: serde_json::Value = serde_json::from_slice(&payload.0).unwrap_or_default();
        self.log
            .lock()
            .unwrap()
            .push((origin.to_string(), op.0.clone(), args.clone()));
        let n = args["items"].as_array().map_or(0, Vec::len);
        if op.0 == CMD_TRANSFER && self.clash && args.get("decide").is_none() {
            let body = serde_json::to_vec(&serde_json::json!({
                "code": "exists",
                "said": "同名 1 项",
                "data": {"conflicts": [{"key": "0", "name": "a.bin", "kind": "file", "size": 3,
                    "mtime_text": "10-12 14:02", "there": "file",
                    "there_mtime_text": "10-01 09:10", "answers": ["overwrite", "skip", "keep_both"]}]},
            }))
            .unwrap_or_default();
            return Box::pin(async move {
                Err(comms_inward::chan::wire::err_from_wire(
                    comms_inward::chan::wire::WireErr::Refused,
                    body,
                ))
            });
        }
        let (answer, hold) = match op.0.as_str() {
            CMD_TRANSFER => (
                serde_json::json!({
                    "said": "已复制 2 项",
                    "outcomes": (0..n).map(|k| serde_json::json!({"key": k.to_string(), "state": "done", "bytes": 3})).collect::<Vec<_>>(),
                }),
                self.hold.clone(),
            ),
            "files-home" => (serde_json::json!({ "path": "/srv" }), None),
            _ => (serde_json::json!({}), None),
        };
        Box::pin(async move {
            if let Some(gate) = hold {
                gate.notified().await;
            }
            Ok(comms_inward::chan::wire::Body(
                serde_json::to_vec(&answer).unwrap_or_default(),
            ))
        })
    }

    fn subscribe(
        &self,
        origin: comms_inward::chan::wire::Origin,
        kind: comms_inward::chan::wire::Kind,
        _from: Option<comms_inward::chan::wire::Cursor>,
    ) -> futures::stream::BoxStream<'static, comms_inward::chan::wire::Item> {
        use futures::StreamExt as _;
        self.log.lock().unwrap().push((
            origin.to_string(),
            "subscribe".into(),
            serde_json::json!(kind.0),
        ));
        let cell = comms_inward::chan::wire::Item::Frame {
            seq: 1,
            body: comms_inward::chan::wire::Body(
                serde_json::to_vec(&serde_json::json!({
                    "done": 0, "total": 2, "bytes_done": 1024, "bytes_total": 4096, "left_text": "约 3 秒",
                }))
                .unwrap_or_default(),
            ),
        };
        futures::stream::iter([cell])
            .chain(futures::stream::pending())
            .boxed()
    }
}

type Log = std::sync::Arc<std::sync::Mutex<Vec<(String, String, serde_json::Value)>>>;

/// 一扇看着 `判据机器·一条` 那台 `/srv` 的窗口，接在 [`OneLine`] 上。
pub(crate) fn one_line_window(
    hold: Option<std::sync::Arc<tokio::sync::Notify>>,
) -> (crate::shell::FileWindow, Log) {
    one_line_window_with(hold, false)
}

/// 同 [`one_line_window`]，`clash` ＝ 对端先说第一项同名（[`OneLine::clash`]）。
pub(crate) fn one_line_window_with(
    hold: Option<std::sync::Arc<tokio::sync::Notify>>,
    clash: bool,
) -> (crate::shell::FileWindow, Log) {
    let log = Log::default();
    let line = crate::find::testing::wire(
        std::sync::Arc::new(OneLine {
            log: log.clone(),
            hold,
            clash,
        }),
        1 << 20,
    );
    let mut w = crate::shell::FileWindow::seeded(
        crate::source::Source::remote(MACHINE.to_string()),
        "/srv".to_string(),
        tokio::runtime::Handle::try_current().ok(),
        Vec::<crate::source::Row>::new(),
    );
    w.attach_line(line);
    (w, log)
}

const MACHINE: &str = "判据机器·一条";

/// 等到通道上出现 `step` 那一步（寻址的那台 · 载荷）。
pub(crate) async fn step_of(
    log: &std::sync::Mutex<Vec<(String, String, serde_json::Value)>>,
    step: &str,
) -> (String, serde_json::Value) {
    tokio::time::timeout(guard_core::wall::CEILING, async {
        loop {
            if let Some((o, _, v)) = log.lock().unwrap().iter().find(|(_, s, _)| s == step) {
                return (o.clone(), v.clone());
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("通道上没出现 {step}：{:?}", log.lock().unwrap()))
}

/// 那一组发出去的入参（去掉票，票另判）与那张票；订阅的正是那张票。
async fn the_group(
    log: &std::sync::Mutex<Vec<(String, String, serde_json::Value)>>,
) -> serde_json::Value {
    let (at, mut args) = step_of(log, CMD_TRANSFER).await;
    assert_eq!(at, cmd_wire::LOCAL, "transfer-copy 没寻址本机");
    let ticket = args["ticket"].as_str().expect("没带票").to_string();
    let (watched_at, kind) = step_of(log, "subscribe").await;
    assert_eq!(watched_at, cmd_wire::LOCAL, "进度没订在本机上");
    assert_eq!(
        kind,
        serde_json::json!(format!(
            "{}{ticket}",
            filewin_contract::FILES_BATCH_KIND_PREFIX
        )),
        "订的不是那张票"
    );
    args.as_object_mut().unwrap().remove("ticket");
    args
}

/// 通道上搬字节那几步（传输 · 提交 · 订阅）恰好是一条 `transfer-copy` ＋ 一条订阅。
fn only_one_group(log: &std::sync::Mutex<Vec<(String, String, serde_json::Value)>>) {
    let mut moving: Vec<String> = log
        .lock()
        .unwrap()
        .iter()
        .map(|(_, s, _)| s.clone())
        .filter(|s| {
            s.starts_with("transfer")
                || s.starts_with("files-commit")
                || s.starts_with("files-stage")
                || s == "subscribe"
        })
        .collect();
    moving.sort();
    assert_eq!(
        moving,
        ["subscribe", CMD_TRANSFER],
        "通道上传输那几步不是恰好这两条"
    );
}

/// 拖进两份本机文件 ⇒ 通道上只有一条寻址本机的 `transfer-copy`（from ＝ 本机 · to ＝ 这一台 · 落点 ＝ 当前目录 · 带票）
/// 和一条 `files-batch/<那张票>` 订阅；不开单、不逐件订 `transfer/<id>`、不提交。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_two_files_sends_one_local_transfer_copy_and_watches_its_ticket() {
    let (mut w, log) = one_line_window(Default::default());
    assert!(
        w.start_drop(
            &["/tmp/up/a.bin".to_string(), "/tmp/up/b.bin".to_string()],
            None
        ),
        "拖入没起来"
    );
    assert_eq!(
        the_group(&log).await,
        serde_json::json!({
            "from": cmd_wire::LOCAL,
            "to": MACHINE,
            "items": [{ "path": "/tmp/up/a.bin" }, { "path": "/tmp/up/b.bin" }],
            "to_dir": "/srv",
        })
    );

    only_one_group(&log);
}

/// 有损目录里上传：落点按目录的原始字节；拿不到末段名的那一份（`/`）不进这一组，一份都没有 ⇒ 不起。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_upload_into_a_lossy_directory_lands_by_its_bytes_and_skips_nameless_paths() {
    let (mut w, log) = one_line_window(Default::default());
    w.cwd_raw = Some(b"/srv/d\xff".to_vec());
    assert!(
        !w.start_drop(&["/".to_string()], None),
        "一份能放的都没有却起了"
    );
    assert!(w.start_drop(&["/".to_string(), "/tmp/up.txt".to_string()], None));
    let args = the_group(&log).await;
    assert_eq!(args["to_dir"], crate::source::wire_bytes(b"/srv/d\xff"));
    assert_eq!(
        args["items"],
        serde_json::json!([{ "path": "/tmp/up.txt" }])
    );
}

/// 下载一项：这一台 → 本机，落点是存盘框给的那一条（拆成目录 ＋ 名字）、同名照盖；名字没改 ⇒ 不交名字（本机后端照源那一段落、
/// 改成这台认的合法名），改了 ⇒ 交他敲的。源路径照原始字节（有损名也是）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn downloading_one_item_is_one_group_from_this_machine_to_the_local_one() {
    let (mut w, log) = one_line_window(Default::default());
    assert!(w.start_pull(
        b"/srv/a\xffb".to_vec(),
        "a\u{fffd}b",
        "/tmp/dl/a\u{fffd}b",
        None
    ));
    assert_eq!(
        the_group(&log).await,
        serde_json::json!({
            "from": MACHINE,
            "to": cmd_wire::LOCAL,
            "items": [{ "path": crate::source::wire_bytes(b"/srv/a\xffb") }],
            "to_dir": "/tmp/dl",
            "on_exists": "overwrite",
        })
    );
    only_one_group(&log);
    let (mut w, log) = one_line_window(Default::default());
    assert!(w.start_pull(b"/srv/a.bin".to_vec(), "a.bin", "/tmp/dl/b.bin", None));
    assert_eq!(
        the_group(&log).await["items"],
        serde_json::json!([{ "path": "/srv/a.bin", "name": "b.bin" }])
    );
    let (mut w, _log) = one_line_window(Default::default());
    assert!(
        !w.start_pull(b"/srv/a.bin".to_vec(), "a.bin", "nodir", None),
        "落点拆不出目录却起了"
    );
}

/// 一组在跑：「进度」那一行的右格是字节数 ＋ 本机后端写好的剩多久；回包是一组的结局 ⇒ 那一行照它落成「成了」、回执是后端那一句。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_group_row_shows_the_backends_left_text_and_lands_on_the_group_outcome() {
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let (mut w, log) = one_line_window(Some(gate.clone()));
    assert!(w.start_drop(&["/tmp/up/a.bin".to_string()], None));
    let _ = the_group(&log).await;
    let row = || {
        w.my_trips()
            .into_iter()
            .next()
            .expect("「进度」里没有那一行")
    };
    let running = tokio::time::timeout(guard_core::wall::CEILING, async {
        loop {
            let v = row().view();
            if v.nums.contains("约 3 秒") {
                return v;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("剩多久那一句没画出来");
    assert_eq!(running.state, crate::progress::State::Running);
    gate.notify_one();
    tokio::time::timeout(guard_core::wall::CEILING, async {
        while row().state() == crate::progress::State::Running {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("一组的结局没落到那一行上");
    let done = row();
    assert_eq!(done.state(), crate::progress::State::Done);
    assert_eq!(
        done.landing().and_then(|l| l.receipt).as_deref(),
        Some("已复制 2 项")
    );
    assert!(!done.dropped(), "结局落了却当成没开过");
}

/// 别的判据摆「进度」表用：一组上传（`names` 那几份放进 `devbox` 的 `dir`）那一行与它的看板。
pub(crate) fn upload_trip(
    names: &[&str],
    dir: &str,
) -> (crate::progress::Trip, crate::batch::BatchBoard) {
    let paths: Vec<String> = names.iter().map(|n| format!("/tmp/{n}")).collect();
    let plan = upload_plan(
        &crate::source::Origin::named("devbox"),
        "devbox".into(),
        &paths,
        dir,
        None,
    )
    .expect("夹具给了没名字的路径");
    let board = crate::batch::BatchBoard::new(plan);
    (
        crate::progress::Trip::Batch {
            board: board.clone(),
            icon: egui_phosphor::regular::UPLOAD_SIMPLE,
        },
        board,
    )
}

/// 那一组落成 `states`（逐项）· 后端那一句 `said`。
pub(crate) fn land(
    board: &crate::batch::BatchBoard,
    states: &[cmd_wire::batch::ItemState],
    said: &str,
) {
    board.set(batch_core::Phase::Ended(cmd_wire::batch::BatchReply {
        outcomes: states
            .iter()
            .enumerate()
            .map(|(i, s)| cmd_wire::batch::BatchOutcome::bare(i.to_string(), *s))
            .collect(),
        said: said.into(),
    }));
}

/// 别的判据摆「进度」表用：一组下载（`devbox` 那台的 `/srv/<name>` 落到本机 `~/下载/<name>`）那一行与它的看板。
pub(crate) fn download_trip(name: &str) -> (crate::progress::Trip, crate::batch::BatchBoard) {
    let plan = crate::download::one_plan(
        &crate::source::Origin::named("devbox"),
        cmd_wire::files::WirePath::from(format!("/srv/{name}").as_str()),
        name,
        &format!("~/下载/{name}"),
    )
    .expect("夹具的落点拆不开");
    let board = crate::batch::BatchBoard::new(plan);
    (
        crate::progress::Trip::Batch {
            board: board.clone(),
            icon: egui_phosphor::regular::DOWNLOAD_SIMPLE,
        },
        board,
    )
}

/// 一组走到 `(已到, 共)` 字节、第 `done` 项（共 `total` 项），带本机后端写好的剩多久 `left`。
pub(crate) fn moved(
    board: &crate::batch::BatchBoard,
    (done, total): (u64, u64),
    bytes: (u64, u64),
    left: Option<&str>,
) {
    board.progress(
        batch_core::Tick {
            done,
            total,
            bytes: Some(bytes),
        },
        left.map(str::to_string),
    );
}
