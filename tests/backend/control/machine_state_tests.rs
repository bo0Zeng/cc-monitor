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
        "ms-round",
        Fact::Round {
            round: Round::Down {
                problem: RoundProblem {
                    text: "拨号没成".into(),
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
                    text: "拨号没成".into(),
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
    table_moved_in(&[("ms-off".to_string(), false)]);
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
        "ms-gone",
        Fact::Round {
            round: Round::Connecting,
            version: Some("9.9.9".into()),
        },
    );
    table_moved_in(&[]);
    assert_eq!(
        read_in(&[("ms-gone".to_string(), true)])["ms-gone"].state,
        MachineStateKind::Unknown,
        "表里拿掉的那台还留着上一份"
    );
}

/// 金样：每一形由生产函数现产，与 `tests/__fixtures__/machine-state.golden.json` 逐字相等；
/// 界面那一侧（`tests/frontend/ui/settings/machine-state.vitest.ts`）读同一份逐形解码、逐形画；本机那一形由壳的判据现产比同一格。
#[test]
fn the_product_matches_the_cross_language_golden() {
    let problem = |why| RoundProblem {
        text: "拨号没成".into(),
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
    let got = read_in(&[("ms-local-peer".to_string(), true)]);
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

/// 这棵树要发的那一版：仓根 `package.json` 的 `version`（发版改的就是它）。
fn shipped_version() -> String {
    let v: serde_json::Value = serde_json::from_str(include_str!("../../../package.json"))
        .expect("仓根 package.json 读不成");
    v["version"]
        .as_str()
        .expect("package.json 没有 version")
        .to_string()
}
