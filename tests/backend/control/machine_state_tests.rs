//! 远端各台的状态表：那台常驻流的那一轮（`control/feeds.rs` 记的）→ 推出去的那一份就是 `deploy::machine::product` 出的成品；机器表关了「连接这台」⇒ 推一份停用；
//! 成品形状与跨语言金样逐格相等（界面那一侧读同一份）。

use super::*;
use cmd_wire::attach::Relation;
use cmd_wire::machine::{Blocker, MachineFix, MachineStateKind, RoundProblem, Why};

/// 订那条通道、等到名字是 `machine` 的那一件（别的判据也在往同一条里推）。
fn next_of(
    rx: &mut tokio::sync::broadcast::Receiver<(String, MachineState)>,
    machine: &str,
) -> MachineState {
    loop {
        match rx.try_recv() {
            Ok((n, p)) if n == machine => return p,
            Ok(_) | Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {}
            Err(e) => panic!("{machine} 那一份没推出来：{e:?}"),
        }
    }
}

#[test]
fn a_round_noted_is_pushed_as_the_product() {
    let mut rx = changes().subscribe();
    let got = note_in(
        &[("ms-round".to_string(), true)],
        &Links::new(),
        "ms-round",
        Fact::Round {
            round: Round::Down {
                problem: RoundProblem {
                    said: "拨号没成".into(),
                    detail: String::new(),
                    why: Some(Why::Auth),
                    seen_key: None,
                    expected_key: None,
                },
                blocker: None,
            },
            version: Some("9.9.9".into()),
        },
    );
    let mut e = Entry::default();
    fold(
        &mut e,
        Fact::Round {
            round: Round::Down {
                problem: RoundProblem {
                    said: "拨号没成".into(),
                    detail: String::new(),
                    why: Some(Why::Auth),
                    seen_key: None,
                    expected_key: None,
                },
                blocker: None,
            },
            version: Some("9.9.9".into()),
        },
    );
    let want = product(&e, "ms-round", true, true);
    assert_eq!(got, want, "回包不是那一份成品");
    assert_eq!(next_of(&mut rx, "ms-round"), want, "推出去的不是那一份成品");
}

#[test]
fn turning_connect_off_pushes_a_disabled_row_with_connect() {
    let mut rx = changes().subscribe();
    note_in(
        &[("ms-off".to_string(), true)],
        &Links::new(),
        "ms-off",
        Fact::Round {
            round: Round::Up {
                relation: Relation::Same,
                differs: vec![],
            },
            version: Some("9.9.9".into()),
        },
    );
    assert_eq!(next_of(&mut rx, "ms-off").state, MachineStateKind::Up);
    table_moved_in(&[("ms-off".to_string(), false)], &Links::new());
    let p = next_of(&mut rx, "ms-off");
    assert_eq!(
        (p.state, p.fixes.iter().map(|f| f.fix).collect::<Vec<_>>()),
        (MachineStateKind::Disabled, vec![MachineFix::Connect])
    );
}

#[test]
fn a_machine_dropped_from_the_table_is_forgotten() {
    note_in(
        &[("ms-gone".to_string(), true)],
        &Links::new(),
        "ms-gone",
        Fact::Round {
            round: Round::Connecting,
            version: Some("9.9.9".into()),
        },
    );
    table_moved_in(&[], &Links::new());
    assert_eq!(
        read_in(&[("ms-gone".to_string(), true)], &Links::new())["ms-gone"].state,
        MachineStateKind::Unknown,
        "表里拿掉的那台还留着上一份"
    );
}

/// 金样：每一形由生产函数现产，与 `tests/__fixtures__/machine-state.golden.json` 逐字相等；
/// 界面那一侧（`tests/frontend/ui/settings/machine-state.vitest.ts`）读同一份逐形解码、逐形画；本机那一形由壳的判据现产比同一格。
#[test]
fn the_product_matches_the_cross_language_golden() {
    let problem = |why| RoundProblem {
        said: "拨号没成".into(),
        detail: "命令：dial\n原话：raw words".into(),
        why,
        seen_key: Some("SHA256:new".into()),
        expected_key: Some("SHA256:old".into()),
    };
    let round = |round| Fact::Round {
        round,
        version: Some("<ver>".into()),
    };
    let up = |relation| {
        round(Round::Up {
            relation,
            differs: vec![],
        })
    };
    let down = |why| {
        round(Round::Down {
            problem: problem(why),
            blocker: None,
        })
    };
    let cases: Vec<(&str, Option<Fact>, bool)> = vec![
        ("连着", Some(up(Relation::Same)), true),
        ("正在连", Some(round(Round::Connecting)), true),
        (
            "正在装",
            Some(Fact::Step(deploy::Step::Place { updating: false })),
            true,
        ),
        (
            "正在更新",
            Some(Fact::Step(deploy::Step::Place { updating: true })),
            true,
        ),
        ("密钥被拒", Some(down(Some(Why::Auth))), true),
        ("无应答", Some(down(Some(Why::Timeout))), true),
        ("指纹变了", Some(down(Some(Why::HostKey))), true),
        ("要更新", Some(up(Relation::Older)), true),
        ("较新", Some(up(Relation::Newer)), true),
        ("版本不可比", Some(up(Relation::Incomparable)), true),
        (
            "不是 Unix",
            Some(round(Round::Down {
                problem: problem(None),
                blocker: Some(Blocker::NotUnix),
            })),
            true,
        ),
        ("要密码", Some(down(Some(Why::Password))), true),
        ("停用", None, false),
    ];
    let mut out = Vec::new();
    for (name, fact, enabled) in cases {
        let mut e = Entry::default();
        if let Some(f) = fact {
            fold(&mut e, f);
        }
        out.push(
            serde_json::json!({ "name": name, "machine": product(&e, "gpu-01", enabled, true) }),
        );
    }
    let golden = serde_json::json!({
        "说明": [
            "设置窗机器列表 / 卡头那一格的成品（本机后端 `changed {machine}` 的 body 与 `machine-read`，本机远端同一份）的跨语言金样。",
            "Rust：tests/backend/control/machine_state_tests.rs 按 name 走生产函数现产，逐字相等。",
            "TS：tests/frontend/ui/settings/machine-state.vitest.ts 读同一份，逐形严格解码、逐形画出那一句与修法。",
            "机器名 · 版本号 · 原因码是结构夹具，不取自任何真机；version 写成占位 <ver>。"
        ],
        "cases": out,
    });
    let text = serde_json::to_string_pretty(&golden).unwrap() + "\n";
    crate::golden::assert_golden(
        "tests/__fixtures__/machine-state.golden.json",
        &text,
        "机器状态成品与金样对不上",
    );
}

/// 本机就是一台远端：本机后端在答 ⇒ 它自己那一行（键 `<local>`）由它出，与远端各台同一份成品，名字在这里取一次；
/// 机器表里没有它，也照样带出。
#[test]
fn the_local_row_comes_from_this_backend() {
    let got = read_in(&[("ms-local-peer".to_string(), true)], &Links::new());
    let local = got
        .get(cmd_wire::machine::LOCAL)
        .expect("机器状态里没有本机那一行");
    let mut e = Entry::default();
    fold(
        &mut e,
        Fact::Round {
            round: Round::Up {
                relation: Relation::Same,
                differs: vec![],
            },
            version: Some(shipped_version()),
        },
    );
    let want = product(
        &e,
        &copy_core::copy_text("remote.cards.local", &[]),
        true,
        true,
    );
    assert_eq!(
        local.version,
        Some(shipped_version()),
        "本机那一行的版本号不是这份二进制出自的那一版"
    );
    assert_eq!(local, &want);
    assert_eq!(local.state, MachineStateKind::Up);
    assert!(got.contains_key("ms-local-peer"), "远端那台被挤掉了");
}

/// ★ A2-17-6-2：首屏那一句照表里各台 ＋ 本机那一行合（本机在答就算报完了）；那台报完了 ⇒ 不再说它还在连；
/// 每推一台的成品，那一刻的这一句跟着走（`changed {machine}` 帧里那一格，`machine-read` 回包同一份）。
#[test]
fn the_empty_line_is_over_the_table_and_the_local_row() {
    let hosts = [("ms-empty".to_string(), true)];
    let up = || Fact::Round {
        round: Round::Up {
            relation: Relation::Same,
            differs: vec![],
        },
        version: Some("9.9.9".into()),
    };
    // 照一张自己的表算：进程里那一张别的判据在并行改（`table_moved_in` 会摘掉不在它那张机器表里的）。
    let mut t = BTreeMap::new();
    fold(t.entry("ms-empty".to_string()).or_default(), up());
    let local = copy_core::local_machine();
    assert_eq!(
        fleet_over(&t, &hosts, &Links::new()),
        cmd_wire::machine::FleetLines {
            line: copy_core::copy_text("beEmpty.line.some", &[("names", &local)]),
            hint: copy_core::copy_text("beEmpty.hint.pending", &[("names", "ms-empty")]),
            count: copy_core::copy_text("machineList.count.all", &[("n", "2")]),
        }
    );
    fold(t.entry("ms-empty".to_string()).or_default(), Fact::Listed);
    assert_eq!(
        fleet_over(&t, &hosts, &Links::new()).line,
        copy_core::copy_text("beEmpty.line.none", &[])
    );
}

/// 这棵树要发的那一版：仓根 `package.json` 的 `version`（发版改的就是它）。
fn shipped_version() -> String {
    let v: serde_json::Value = serde_json::from_str(include_str!("../../../package.json"))
        .expect("仓根 package.json 读不成");
    v["version"]
        .as_str()
        .expect("package.json 没有 version")
        .to_string()
}

/// ★ 到那台怎么连过去的进它的成品：收事实推的那一帧 · 表动了推的那一帧 · `machine-read` 都带同一份 `route`，
/// 每条连接那一句进复制详情；别的台不带；本机那一行没有。
#[test]
fn the_product_carries_how_that_machine_is_reached() {
    let way = ssh_dial::pool::Way {
        route: Some("经跳板（夹具）".into()),
        hops: vec!["那一句（夹具）".into()],
    };
    let links: Links = [("ms-linked".to_string(), way.clone())]
        .into_iter()
        .collect();
    let hosts = [
        ("ms-linked".to_string(), true),
        ("ms-unlinked".to_string(), true),
    ];
    let route = Some("经跳板（夹具）".to_string());
    let mut rx = changes().subscribe();
    let noted = note_in(
        &hosts,
        &links,
        "ms-linked",
        Fact::Round {
            round: Round::Connecting,
            version: None,
        },
    );
    assert_eq!(noted.route, route, "收事实回的成品没带 route");
    assert!(
        noted
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("那一句（夹具）")),
        "每条连接那一句没进复制详情：{:?}",
        noted.detail
    );
    assert_eq!(next_of(&mut rx, "ms-linked").route, route);
    table_moved_in(&hosts, &links);
    assert_eq!(
        next_of(&mut rx, "ms-linked").route,
        route,
        "表动了推的那一帧没带 route"
    );
    assert_eq!(next_of(&mut rx, "ms-unlinked").route, None);
    let read = read_in(&hosts, &links);
    assert_eq!(read["ms-linked"].route, route);
    assert_eq!(read["ms-unlinked"].route, None, "没连接的那台不该带");
    assert_eq!(read["ms-unlinked"].detail, None);
    assert_eq!(read[cmd_wire::machine::LOCAL].route, None, "本机不经 SSH");
    let wire = serde_json::to_value(&read["ms-unlinked"]).expect("成品写不出");
    assert!(wire.get("route").is_none(), "直连 ⇒ 线上省略那一格");
    assert!(
        wire.get("links").is_none(),
        "每条连接不再上线（只进复制详情）"
    );
}

/// ★ 池里成员动了一笔 ⇒ 推一遍；发端收了 ⇒ 跟的那一路收工（不空转）。
#[tokio::test]
async fn a_pool_move_repushes_and_the_follower_ends_with_the_pool() {
    static PUSHED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    fn push() {
        PUSHED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    let (tx, rx) = tokio::sync::watch::channel(0u64);
    let run = tokio::spawn(follow(rx, push));
    tx.send_modify(|n| *n += 1);
    let waited = tokio::time::timeout(guard_core::wall::CEILING, async {
        while PUSHED.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(waited.is_ok(), "成员动了一笔，没推");
    drop(tx);
    let ended = tokio::time::timeout(guard_core::wall::CEILING, run).await;
    assert!(ended.is_ok(), "池没了，跟的那一路还在等");
    assert_eq!(
        PUSHED.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "动一笔只推一遍"
    );
}
