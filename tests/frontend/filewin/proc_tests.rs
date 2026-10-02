//! 窗口进程那一侧的躯体（`proc.rs`：拨回通道 · 列第一屏 · `child_main` 的行序）那几条判据 ——
//! 原住 `tests/frontend/shell/filewin/proc_tests.rs` 的后半，随躯体搬进独立包；进程形态（起进程 · 种子 · 就绪那一行）那一半留在 monitor 那一侧。

use super::*;

/// 一台合成远端的名字（带中文与连字符）。
fn synthetic_cfg() -> String {
    "台架-远端".to_string()
}

// ════════════════════════════════════════════════════════════════════════
// 🔴窗口进程拨回 monitor 那一下
// ════════════════════════════════════════════════════════════════════════

/// 🔴 **拿着交接件拨得通；钥匙不对 / 口不在就是错 —— 而且错的时候窗口不开**（`D11`）。
///
/// ① 阳性：一个真通道口（合成后端挂在宿主上）⇒ 拨得通，而且那条线**真能说一次 `call`**；
/// ② 钥匙错一个字节 ⇒ 错（不是「连上了再说」）；③ 口不在 ⇒ 错。
/// ④ `child_main` 里「先拨、拨不通就回 `EXIT_WINDOW_FAILED`」排在开窗之前（源码行序，代理）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_window_dials_back_with_the_handoff_and_refuses_to_open_without_it() {
    use crate::find::testing::{Declared, FakeBackend};
    // ① 阳性：真口 ＋ 真钥匙。
    let be = std::sync::Arc::new(FakeBackendHost::new(FakeBackend::new(
        &["files-stat"],
        Declared::default(),
    )));
    let h = chan_core::chan::handoff::start_with(
        be,
        chan_core::chan::handoff::mint_key(),
        1 << 20,
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("回环口绑得上");
    let line = dial_back(&h).await.expect("拿着对的交接件却拨不通");
    let origin = chan_core::origin::Origin("proc-dial".into());
    let r = crate::source::ask(
        &line,
        &origin,
        "files-stat",
        &serde_json::json!({ "path": "/" }),
        std::time::Duration::from_secs(5),
    )
    .await;
    assert!(r.is_ok(), "拨通了却说不了一次 call：{r:?}");
    // ② 钥匙错。
    let mut wrong = h.clone();
    wrong.key = chan_core::chan::wire::Key("0".repeat(64));
    let e = match dial_back(&wrong).await {
        Ok(_) => panic!("钥匙不对竟然拨通了"),
        Err(e) => e,
    };
    assert!(e.contains("窗口连不上主程序"), "{e}");
    // ③ 口不在（拿一个刚放掉的回环端口）。
    let dead = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap()
    };
    let mut gone = h.clone();
    gone.addr = dead;
    assert!(dial_back(&gone).await.is_err(), "口不在竟然拨通了");
    // ④ 行序：拨在开窗之前，拨不通就回失败码。
    let prod =
        guard_core::production_code(include_str!("../../../src/frontend/filewin/src/proc.rs"));
    let at_dial = guard_core::find_pinned(&prod, "rt.block_on(dial_back(&req.handoff))")
        .expect("child_main 里没有拨回那一下");
    let at_open = guard_core::find_pinned(&prod, "super::shell::open_detached_seeded(")
        .expect("child_main 里没有开窗那一下");
    assert!(
        at_dial < at_open,
        "开窗排在拨通道之前 —— 拨不通时窗口已经开了"
    );
    // ⑤种子里每一格都真的交给了开窗那一下（漏交一格 ＝ 那一格在窗口那侧恒是默认值，
    //    种子对拍照样绿 —— 它只判「过得了进程边界」，判不了「过去之后有人接」）。
    for f in ["reveal", "bookmarks", "machines"] {
        let at = guard_core::find_pinned(&prod, &format!("        req.{f},\n"))
            .unwrap_or_else(|e| panic!("child_main 没把种子里的 `{f}` 交给开窗那一下：{e}"));
        assert!(at > at_open, "`req.{f}` 不在开窗那一下的实参里");
    }
    // 那台的名字（种子 `origin`）先造成窗口那一侧的 `Source`，再交给开窗那一下。
    let at_src = guard_core::find_pinned(&prod, "Source::remote(req.origin.clone())")
        .expect("child_main 没拿种子里那台的名字造 `Source`");
    let at_src_arg = guard_core::find_pinned(&prod[at_open..], "        source,\n")
        .expect("开窗那一下的实参里没有 `source`");
    assert!(at_src < at_open, "`Source` 造在开窗之后");
    let _ = at_src_arg;
    // ⑥〔09-28 裁 3〕第一屏：拨通之后、开窗之前列；列不出来就退（不开窗）；起点与那一屏是它列出来的那一份。
    let at_first = guard_core::find_pinned(
        &prod,
        "rt.block_on(first_screen(&line, &source, req.cwd.clone()))",
    )
    .expect("child_main 里没有列第一屏那一下");
    let at_refuse = guard_core::find_pinned(&prod, "Err(e) => return refuse(e, EXIT_NOT_LISTED),")
        .expect("第一屏列不出来那一支不是「说原话、退」");
    let at_ready = guard_core::find_pinned(&prod, "say(&Ready::Listed(rows.len()));")
        .expect("列出来之后没说就绪那一行");
    assert!(
        at_dial < at_first && at_first < at_refuse && at_refuse < at_ready && at_ready < at_open,
        "行序不是「拨 → 列 → 列不出来就退 → 说就绪 → 开窗」"
    );
    for f in ["cwd", "rows"] {
        let at = guard_core::find_pinned(&prod, &format!("        {f},\n"))
            .unwrap_or_else(|e| panic!("开窗那一下没收列出来的 `{f}`：{e}"));
        assert!(at > at_open, "列出来的 `{f}` 不在开窗那一下的实参里");
    }
}

/// 把 `find::testing::FakeBackend` 挂成宿主句柄的最小包装（`wire_up` 那一份不交出句柄本身）。
struct FakeBackendHost(std::sync::Mutex<crate::find::testing::FakeBackend>);

impl FakeBackendHost {
    fn new(be: crate::find::testing::FakeBackend) -> Self {
        Self(std::sync::Mutex::new(be))
    }
}

impl chan_core::chan::router::Backends for FakeBackendHost {
    fn call(
        &self,
        _origin: chan_core::chan::wire::Origin,
        op: chan_core::chan::wire::Op,
        _payload: chan_core::chan::wire::Body,
        _left: std::time::Duration,
        _cancel: chan_core::chan::wire::CancelToken,
    ) -> futures::future::BoxFuture<
        'static,
        Result<chan_core::chan::wire::Body, chan_core::chan::wire::CallError>,
    > {
        let known = self.0.lock().unwrap().offered.iter().any(|c| c == &op.0);
        Box::pin(async move {
            if known {
                Ok(chan_core::chan::wire::Body(b"{\"kind\":\"dir\"}".to_vec()))
            } else {
                Err(chan_core::chan::wire::CallError::Peer {
                    why: chan_core::chan::wire::PeerFault::Unsupported,
                })
            }
        })
    }

    fn subscribe(
        &self,
        _origin: chan_core::chan::wire::Origin,
        _kind: chan_core::chan::wire::Kind,
        _from: Option<chan_core::chan::wire::Cursor>,
    ) -> futures::stream::BoxStream<'static, chan_core::chan::wire::Item> {
        Box::pin(futures::stream::empty())
    }
}

/// 〔09-28 裁 3〕答 `files-home` / `files-ls` 的替身后端：记下问了哪几条；`refuse` 里的那条回「不行 + 原话」。
struct ScreenHost {
    asked: std::sync::Mutex<Vec<String>>,
    refuse: Option<&'static str>,
}

impl chan_core::chan::router::Backends for ScreenHost {
    fn call(
        &self,
        _origin: chan_core::chan::wire::Origin,
        op: chan_core::chan::wire::Op,
        payload: chan_core::chan::wire::Body,
        _left: std::time::Duration,
        _cancel: chan_core::chan::wire::CancelToken,
    ) -> futures::future::BoxFuture<
        'static,
        Result<chan_core::chan::wire::Body, chan_core::chan::wire::CallError>,
    > {
        self.asked.lock().unwrap().push(op.0.clone());
        let refused = self.refuse == Some(op.0.as_str());
        let args: serde_json::Value = serde_json::from_slice(&payload.0).unwrap_or_default();
        Box::pin(async move {
            let json =
                |v: serde_json::Value| Ok(chan_core::chan::wire::Body(v.to_string().into_bytes()));
            if refused {
                return Err(chan_core::chan::wire::CallError::Peer {
                    why: chan_core::chan::wire::PeerFault::Refused {
                        body: chan_core::chan::wire::Body(
                            r#"{"code":"io_failed","message":"那台说：没有这个目录"}"#
                                .as_bytes()
                                .to_vec(),
                        ),
                    },
                });
            }
            match op.0.as_str() {
                "files-home" => json(serde_json::json!({ "path": "/home/台架" })),
                "files-ls" => {
                    let dir = args
                        .get("path")
                        .and_then(|p| p.as_str())
                        .unwrap_or("?")
                        .to_string();
                    json(serde_json::json!({
                        "entries": [
                            { "path": format!("{dir}/甲"), "kind": "dir" },
                            { "path": format!("{dir}/乙.txt"), "kind": "file", "size": 3 },
                        ],
                        "truncated": false,
                    }))
                }
                _ => Err(chan_core::chan::wire::CallError::Peer {
                    why: chan_core::chan::wire::PeerFault::Unsupported,
                }),
            }
        })
    }

    fn subscribe(
        &self,
        _origin: chan_core::chan::wire::Origin,
        _kind: chan_core::chan::wire::Kind,
        _from: Option<chan_core::chan::wire::Cursor>,
    ) -> futures::stream::BoxStream<'static, chan_core::chan::wire::Item> {
        Box::pin(futures::stream::empty())
    }
}

/// 🔴**窗口进程自己问第一屏**（真通道口 ＋ 替身后端）：
/// ① 没给目录 ⇒ 先问 home、再列 home（恰好这两问，按这个顺序）；② 给了目录 ⇒ 只列它、**不问 home**；
/// ③ 列不出来 ⇒ 带那台的原话回错（窗口那侧据此说 `Failed` 并不开窗）；④ home 问不到 ⇒ 同样带原话、不去列。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_first_screen_asks_home_only_when_told_nothing() {
    async fn run(
        refuse: Option<&'static str>,
        cwd: Option<String>,
    ) -> (Result<(String, Vec<String>), String>, Vec<String>) {
        let be = std::sync::Arc::new(ScreenHost {
            asked: std::sync::Mutex::new(Vec::new()),
            refuse,
        });
        let h = chan_core::chan::handoff::start_with(
            be.clone(),
            chan_core::chan::handoff::mint_key(),
            1 << 20,
            std::time::Duration::from_secs(5),
        )
        .await
        .expect("回环口绑得上");
        let line = dial_back(&h).await.expect("拨得通");
        let got = first_screen(&line, &Source::remote(synthetic_cfg()), cwd)
            .await
            .map(|(d, rows)| (d, rows.into_iter().map(|l| l.row.name).collect()));
        let asked = be.asked.lock().unwrap().clone();
        (got, asked)
    }
    // ①
    let (got, asked) = run(None, None).await;
    assert_eq!(
        asked,
        ["files-home", "files-ls"],
        "没给目录时问的不是「先 home、再列」"
    );
    let (dir, names) = got.expect("问得到 home、列得出来，却回了错");
    assert_eq!(dir, "/home/台架");
    assert_eq!(names.len(), 2, "那一屏不是替身答的那两行：{names:?}");
    // ②
    let (got, asked) = run(None, Some("/srv/给了".into())).await;
    assert_eq!(asked, ["files-ls"], "给了目录还去问了 home");
    assert_eq!(got.expect("列得出来").0, "/srv/给了");
    // ③
    let (got, asked) = run(Some("files-ls"), Some("/srv/不在".into())).await;
    assert_eq!(asked, ["files-ls"]);
    let e = got.expect_err("列不出来竟然回了一屏");
    assert!(e.contains("那台说：没有这个目录"), "原话没带回来：{e}");
    // ④
    let (got, asked) = run(Some("files-home"), None).await;
    assert_eq!(asked, ["files-home"], "home 问不到还去列了");
    assert!(got
        .expect_err("home 问不到竟然过了")
        .contains("那台说：没有这个目录"));
}
