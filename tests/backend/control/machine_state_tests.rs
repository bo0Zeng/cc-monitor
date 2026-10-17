//! 远端各台的状态表：壳交来的那一轮 → 推出去的那一份就是 `deploy::machine::product` 出的成品；机器表关了「连接这台」⇒ 推一份停用；
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

#[tokio::test]
async fn a_round_handed_in_is_pushed_as_the_product() {
    use crate::wire::typed::Handle as _;
    let mut rx = changes().subscribe();
    let req = MachineRoundReq {
        machine: "ms-round".into(),
        round: Round::Down {
            problem: RoundProblem {
                text: "拨号没成".into(),
                detail: String::new(),
                why: Some(Why::Auth),
                seen_key: None,
            },
            blocker: None,
        },
        version: "9.9.9".into(),
    };
    let got = MachineRoundCmd::run(
        req,
        crate::wire::typed::Cx {
            tz: Default::default(),
            until: None,
            needs: (),
        },
    )
    .await
    .expect("交那一轮没成");
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
            "设置窗机器列表 / 卡头那一格的成品（远端：本机后端 `changed {machine}` 的 body 与 `machine-read`；本机：`backend_status` 的 machine）的跨语言金样。",
            "Rust：tests/backend/control/machine_state_tests.rs 按 name 走生产函数现产（本机那一形在 tests/frontend/shell/local_backend_host_tests.rs），逐字相等。",
            "TS：tests/frontend/ui/settings/machine-state.vitest.ts 读同一份，逐形严格解码、逐形画出那一句与修法。",
            "机器名 · 版本号 · 原因码是结构夹具，不取自任何真机；version 写成占位 <ver>（本机那一形由壳把现产的号换成它再比）。",
            "next 是壳写好的下一步那一句（只有本机起后端被拒时有），界面照原样画在问题行下面。"
        ],
        "cases": out,
    });
    let base: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/machine-state.golden.json"))
            .unwrap_or(serde_json::Value::Null);
    // 本机那两形由壳现产，这里原样带过去（不在后端判）。
    let local: Vec<serde_json::Value> = base["cases"]
        .as_array()
        .map(|c| {
            c.iter()
                .filter(|c| c["name"] == "本机被拒" || c["name"] == "本机没连上")
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let mut golden = golden;
    if let Some(c) = golden["cases"].as_array_mut() {
        c.extend(local);
    }
    let text = serde_json::to_string_pretty(&golden).unwrap() + "\n";
    crate::golden::assert_golden(
        "tests/__fixtures__/machine-state.golden.json",
        &text,
        "机器状态成品与金样对不上",
    );
}
