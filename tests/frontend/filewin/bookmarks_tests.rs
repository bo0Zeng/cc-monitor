//! [`super`] 的判据：窗口只按机器名问那台后端（本机与远端同一形）· 书签栏真画得出、真点得到（跑生产那个 `Workspace::frame`）·
//! 别的写者加的那一条换一次目录就看得见 · 问不成那一句画在栏上 · 先发后到的回包不盖掉后发的。
//!
//! 那台后端是本文件里一台合成的书签后端（[`Books`]，挂在真通道上）：切换 / 删照那台的规矩做一份，判据只看线上发了什么、
//! 窗口照回包画了什么。切换 · 归一 · 两个写者不丢的本体判据在后端（`tests/backend/control/bookmarks_tests.rs`）。

use super::*;
use crate::copy::testing::{rects_of, PaintedText};
use crate::find::testing::{window_on, wire_up, Declared, FakeBackend};
use crate::shell::FileWindow;
use crate::workspace::Workspace;
use cmd_wire::bookmarks::normalize_dir;
use comms_inward::chan::wire::{Body, CallError, PeerFault};

/// 合成的那台书签后端：一份书签 ＋ 收到过的每一问（机器名 · 命令 · 入参）。
#[derive(Default)]
pub(crate) struct Books {
    pub(crate) dirs: Mutex<Vec<String>>,
    log: Mutex<Vec<(String, String, serde_json::Value)>>,
    /// 设了 ⇒ 每一问都回这一句（`io_failed`）。
    refuse: Mutex<Option<String>>,
}

impl Books {
    fn asked(&self) -> Vec<(String, String, serde_json::Value)> {
        self.log.lock().unwrap().clone()
    }
    fn dirs(&self) -> Vec<String> {
        self.dirs.lock().unwrap().clone()
    }
}

/// 不接通道、手上已有这几条的一份，那台答过「此刻在第一条」（判据直接喂画面用：悬停扫 · 截图场景；窗口开在 `dirs[0]`）。
pub(crate) fn holding(origin: &str, dirs: &[&str]) -> Shelf {
    let at = dirs.first().copied().unwrap_or("/");
    let s = Shelf::open(None, None, &Origin::named(origin), at);
    s.0.land(
        1,
        at,
        Ok(Bookmarks {
            dirs: dirs.iter().map(|d| normalize_dir(d)).collect(),
            here: (!dirs.is_empty()).then_some(0),
        }),
    );
    s
}

impl comms_inward::chan::router::Backends for Books {
    fn call(
        &self,
        origin: comms_inward::chan::wire::Origin,
        op: comms_inward::chan::wire::Op,
        payload: Body,
        _view: Option<serde_json::Value>,
        _left: std::time::Duration,
        _cancel: comms_inward::chan::wire::CancelToken,
    ) -> futures::future::BoxFuture<'static, Result<Body, CallError>> {
        let args: serde_json::Value =
            serde_json::from_slice(&payload.0).unwrap_or(serde_json::Value::Null);
        self.log
            .lock()
            .unwrap()
            .push((origin.to_string(), op.0.clone(), args.clone()));
        if let Some(said) = self.refuse.lock().unwrap().clone() {
            let body = serde_json::json!({ "code": "io_failed", "message": said });
            return Box::pin(async move {
                Err(CallError::Peer {
                    why: PeerFault::Refused {
                        body: Body(serde_json::to_vec(&body).unwrap()),
                    },
                })
            });
        }
        let mut dirs = self.dirs.lock().unwrap();
        if op.0 == "bookmarks-edit" {
            let req: BookmarksEditReq =
                serde_json::from_value(args.clone()).expect("入参照类型");
            for d in &req.dirs {
                let d = normalize_dir(d);
                let at = dirs.iter().position(|x| *x == d);
                match (req.op, at) {
                    (BookmarkOp::Toggle | BookmarkOp::Remove, Some(i)) => {
                        dirs.remove(i);
                    }
                    (BookmarkOp::Toggle | BookmarkOp::Add, None) => dirs.push(d),
                    _ => {}
                }
            }
        }
        let here = args
            .get("at")
            .and_then(|a| a.as_str())
            .and_then(|a| dirs.iter().position(|x| *x == normalize_dir(a)))
            .map(|i| i as u32);
        let reply = Bookmarks {
            dirs: dirs.clone(),
            here,
        };
        Box::pin(async move { Ok(Body(serde_json::to_vec(&reply).unwrap())) })
    }

    fn subscribe(
        &self,
        _origin: comms_inward::chan::wire::Origin,
        _kind: comms_inward::chan::wire::Kind,
        _from: Option<comms_inward::chan::wire::Cursor>,
    ) -> futures::stream::BoxStream<'static, comms_inward::chan::wire::Item> {
        Box::pin(futures::stream::empty())
    }
}

/// 一台书签后端 ＋ 拨通它的那条线。**要在 tokio 运行时里调。**
pub(crate) fn books() -> (Arc<Books>, Line) {
    let be = Arc::new(Books::default());
    let line = crate::find::testing::wire(be.clone(), 1 << 22);
    (be, line)
}

pub(crate) fn shelf_on(line: &Line, origin: &str, at: &str) -> Shelf {
    Shelf::open(
        tokio::runtime::Handle::try_current().ok(),
        Some(line.clone()),
        &Origin::named(origin),
        at,
    )
}

/// 等到 `ok` 成立（带上限，绝不挂死）。
async fn until(who: &str, ok: impl Fn() -> bool) {
    for _ in guard_core::wall::rounds() {
        if ok() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("{who}：到兜底上限还没等到");
}

/// 🔴 先红的那一条：`<local>` 与远端**同一形** —— 同一串动作，线上发的命令与入参逐字相等，只有机器名不同；
/// 窗口里没有「本机走文件、远端走命令」两条路。每一问都带上此刻的目录（`at`），「在不在」照回包的 `here`。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_and_remote_ask_their_backend_in_the_same_shape() {
    let mut seen = Vec::new();
    for origin in [cmd_wire::LOCAL, "devbox"] {
        let (be, line) = books();
        let s = shelf_on(&line, origin, "/srv/");
        until("开窗那一问", || be.asked().len() == 1).await;
        s.toggle("/srv/");
        until("切换落地", || s.here("/srv/") == Some(0)).await;
        s.remove("/srv", "/srv/");
        until("删落地", || s.list().is_empty()).await;
        assert_eq!(s.here("/srv/"), None, "删了还说在");
        let asked = be.asked();
        assert!(
            asked.iter().all(|(o, _, _)| o == origin),
            "问的不是这台：{asked:?}"
        );
        seen.push(
            asked
                .into_iter()
                .map(|(_, c, a)| (c, a))
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(
        seen[0],
        vec![
            (
                "bookmarks-read".to_string(),
                serde_json::json!({ "at": "/srv/" })
            ),
            (
                "bookmarks-edit".to_string(),
                serde_json::json!({ "op": "toggle", "dirs": ["/srv/"], "at": "/srv/" })
            ),
            (
                "bookmarks-edit".to_string(),
                serde_json::json!({ "op": "remove", "dirs": ["/srv"], "at": "/srv/" })
            ),
        ]
    );
    assert_eq!(seen[0], seen[1], "本机与远端发的不一样");
}

/// 先发后到的回包不盖掉后发的那一问（第 1 问晚到 ⇒ 不认）。
#[test]
fn a_late_reply_of_an_earlier_ask_is_not_taken() {
    let s = Shelf::open(None, None, &Origin::named("x"), "/");
    s.0.land(
        2,
        "/new",
        Ok(Bookmarks {
            dirs: vec!["/new".into()],
            here: Some(0),
        }),
    );
    s.0.land(
        1,
        "/old",
        Ok(Bookmarks {
            dirs: vec!["/old".into()],
            here: Some(0),
        }),
    );
    assert_eq!(s.list(), vec!["/new".to_string()]);
    assert_eq!((s.here("/new"), s.here("/old")), (Some(0), None));
    s.0.land(3, "/new", Err("那一句".into()));
    assert_eq!(s.notice().as_deref(), Some("那一句"));
    assert_eq!(
        s.list(),
        vec!["/new".to_string()],
        "问不成 ⇒ 手上那一份不动"
    );
}

/// 「在不在」只认那台对**此刻这个目录**的回答：答的是别的目录（换了目录、那一问还没回）⇒ 当没答；窗口不自己比路径。
#[test]
fn here_is_only_what_the_backend_said_about_this_very_dir() {
    let s = Shelf::open(None, None, &Origin::named("x"), "/");
    s.0.land(
        1,
        "/a",
        Ok(Bookmarks {
            dirs: vec!["/a".into(), "/b".into()],
            here: Some(0),
        }),
    );
    assert_eq!(s.here("/a"), Some(0));
    assert_eq!(s.here("/b"), None, "那一问答的是 /a，不是 /b");
    assert_eq!(s.here("/a/"), None, "窗口不自己归一：那一问带的是 /a");
}

// ═══════════════════════════════════════════════════════════════════
// 窗口上那一半：左栏的书签那一段真画得出、真点得到
// ═══════════════════════════════════════════════════════════════════

/// 一帧生产那个 `Workspace::frame`（左栏在它的框里），交回这一帧画出来的字 ＋ 位置。
fn frame(ctx: &egui::Context, ws: &mut Workspace, events: Vec<egui::Event>) -> Vec<PaintedText> {
    crate::chrome::testing::ws_frame(ctx, ws, events)
}

fn click_label(ctx: &egui::Context, ws: &mut Workspace, label: &str) -> Vec<PaintedText> {
    crate::chrome::testing::ws_click(ctx, ws, label)
}

/// 等列目录落地（带上限，绝不挂死）。
async fn settle_listing(w: &FileWindow, who: &str) {
    until(who, || !w.listing.is_loading()).await;
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-fw-bm-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("建临时目录");
    d
}

/// 🔴 **真点一下**：☆ 加进去（那台后端真有）→ 换目录 → 点那条书签跳回来 → × 删掉（那台后端真没了）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_bar_is_really_clickable_and_the_backend_follows() {
    // 垫两段很长的名字：路径长到盖过地址栏右端那颗星时星照样点得到；收藏的目录名很长时左栏那一条截断显示、它和 × 都点得到。
    let base = scratch("ui");
    let root = base.join("x".repeat(160));
    let (a, b) = (root.join(format!("a{}", "y".repeat(120))), root.join("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    let (a, b) = (
        a.to_string_lossy().to_string(),
        b.to_string_lossy().to_string(),
    );
    let wired = wire_up(
        "fw-bm-ui",
        FakeBackend::new(&["files-ls"], Declared::default()),
    )
    .await;
    let (be, line) = books();
    let mut w = window_on(&wired, &a);
    w.shelf = Some(shelf_on(&line, "fw-bm-ui", &a));
    let mut ws = Workspace::new(w);
    let ctx = egui::Context::default();

    // ① 地址栏那颗星（灰）⇒ 那台后端里恰好是当前目录；星换成强调色。
    let star = egui_phosphor::regular::STAR;
    let p = crate::theme::palette(&ctx);
    assert_eq!(
        crate::chrome::testing::ws_colors_of(&ctx, &mut ws, star),
        vec![p.text2],
        "没加书签时那颗星该是灰的"
    );
    let _ = click_label(&ctx, &mut ws, star);
    let shelf = ws.pane_on(0).shelf.clone().unwrap();
    until("☆ 落地", || shelf.here(&a) == Some(0)).await;
    assert_eq!(be.dirs(), vec![a.clone()]);
    let after = crate::chrome::testing::ws_colors_of(&ctx, &mut ws, star);
    assert_eq!(after.len(), 2, "地址栏一颗 ＋ 左栏那条书签一颗：{after:?}");
    assert!(
        after.contains(&p.accent),
        "加完之后地址栏那颗星该涂强调色：{after:?}"
    );

    // ② 换到 b，点左栏那条书签（写的是目录的尾段）⇒ 真的跳回 a。
    ws.pane_on_mut(0).navigate_to(b.clone());
    settle_listing(ws.pane_on(0), "换到 b").await;
    let _ = click_label(&ctx, &mut ws, crate::source::remote_basename(&a));
    assert_eq!(ws.pane_on(0).cwd, a, "点了书签却没跳过去");

    // ③ × 删掉 ⇒ 那台后端里没了（× 只在悬停那一行时出）。
    let tail = crate::source::remote_basename(&a).to_string();
    let painted = frame(&ctx, &mut ws, Vec::new());
    let row = rects_of(&painted, &tail)
        .into_iter()
        .find(|r| r.left() < 220.0 && r.top() > 60.0)
        .expect("左栏里没有那条书签");
    let hover = vec![egui::Event::PointerMoved(row.center())];
    let _ = frame(&ctx, &mut ws, hover.clone());
    let painted = frame(&ctx, &mut ws, hover);
    let x = rects_of(&painted, egui_phosphor::regular::X)
        .into_iter()
        .find(|r| r.center().x < row.right() + 40.0 && (r.center().y - row.center().y).abs() < 14.0)
        .expect("悬停那一行没出 ×");
    let _ = frame(&ctx, &mut ws, crate::rows::testing::click_at(x.center()));
    until("× 落地", || shelf.list().is_empty()).await;
    assert!(be.dirs().is_empty(), "点了 × 那台后端里还在");
    let _ = std::fs::remove_dir_all(&base);
}

/// 别的写者（另一个窗口 · 手机）刚加的书签，本窗口**换一次目录**就看得见；之前看不见（不是定时去问）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn another_writers_bookmark_shows_up_after_a_navigation() {
    let root = scratch("other");
    let a = root.join("a");
    std::fs::create_dir_all(&a).unwrap();
    let a = a.to_string_lossy().to_string();
    let wired = wire_up(
        "fw-bm-other",
        FakeBackend::new(&["files-ls"], Declared::default()),
    )
    .await;
    let (be, line) = books();
    let mut w = window_on(&wired, "/");
    w.shelf = Some(shelf_on(&line, "fw-bm-other", "/"));
    until("开窗那一问", || be.asked().len() == 1).await;
    be.dirs.lock().unwrap().push("/elsewhere".into());
    let shelf = w.shelf.clone().unwrap();
    assert!(
        shelf.list().is_empty(),
        "还没换目录就看见了 —— 那说明在定时去问"
    );
    w.navigate_to(a.clone());
    until("换目录才问第二次", || be.asked().len() == 2).await;
    let reads: Vec<serde_json::Value> = be
        .asked()
        .into_iter()
        .filter(|(_, c, _)| c == "bookmarks-read")
        .map(|(_, _, args)| args)
        .collect();
    assert_eq!(
        reads,
        vec![serde_json::json!({ "at": "/" }), serde_json::json!({ "at": a })],
        "开窗一次 ＋ 换目录一次，别的时候不问；每一问带上那一刻的目录"
    );
    until("换目录那一问", || {
        shelf.list() == vec!["/elsewhere".to_string()]
    })
    .await;
    let _ = std::fs::remove_dir_all(&root);
}

/// 那台后端问不成 ⇒ 左栏书签那一段**出声**（那一句从这一帧的 galley 里读回来）；没接书签的窗口整条栏不画。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refusal_is_said_on_the_bar() {
    let wired = wire_up("fw-bm-refused", FakeBackend::new(&[], Declared::default())).await;
    let (be, line) = books();
    *be.refuse.lock().unwrap() = Some("书签读不出来".into());
    let mut w = window_on(&wired, "/srv");
    w.shelf = Some(shelf_on(&line, "fw-bm-refused", "/srv"));
    let shelf = w.shelf.clone().unwrap();
    until("那一问回来", || shelf.notice().is_some()).await;
    let said = shelf.notice().unwrap();
    assert!(
        said.contains("书签读不出来"),
        "那一句没带那台的原话：{said}"
    );
    let mut ws = Workspace::new(w);
    let ctx = egui::Context::default();
    let painted = frame(&ctx, &mut ws, Vec::new());
    assert!(
        painted.iter().any(|(t, _)| *t == said),
        "那句话没画出来：{painted:?}"
    );
    // 阴性对照：没接书签（判据直接建的窗口）⇒ 加 / 去那颗与出声那句都不画。
    let mut bare = Workspace::new(window_on(&wired, "/srv"));
    let painted = frame(&ctx, &mut bare, Vec::new());
    assert!(
        rects_of(&painted, egui_phosphor::regular::STAR).is_empty()
            && !painted.iter().any(|(t, _)| *t == said)
    );
}
