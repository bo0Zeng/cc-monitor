//! 足迹那一族的命令走帧面那一跳（按名找命令、照它的档跑）：要命令表与进后端的口，那一层在主 crate ⇒ 挂主 crate。

use crate::wire::spec::{invoke, Prepared};
use serde_json::{json, Value};

/// 按名找命令、照阻塞档跑一问，回包的码（成了 ⇒ `Ok`）。
fn run_on_frame(
    cmd: &str,
    args: Value,
    own: Option<&crate::platform::fs::Own>,
) -> Result<(), String> {
    let spec = crate::stream::inbound::REGISTRY
        .iter()
        .find(|s| s.name == cmd)
        .unwrap();
    let Some(Prepared::Blocking(run)) = invoke(spec.run, &crate::stream::inbound::hands(own))
    else {
        panic!("{cmd} 不在阻塞档");
    };
    let req = crate::wire::Request {
        id: "t".to_string(),
        cmd: cmd.to_string(),
        args,
        ..Default::default()
    };
    run(req)
        .map(|_| ())
        .map_err(|f| f.code_or_fallback().to_string())
}

/// 五条小命令：入参多一个键 ⇒ `bad_args`、一个字节不写。
#[test]
fn an_extra_key_is_bad_args_on_the_frame_path_for_the_small_commands() {
    let d = std::env::temp_dir().join(format!("fp-small-wire-extra-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let own = crate::platform::fs::Own::issue_at(&d);
    let cases = [
        (
            "agent-home-check",
            json!({ "path": d.join("nope"), "bogus": 1 }),
        ),
        ("first-run", json!({ "remotes": 0, "bogus": 1 })),
        ("chores-mark", json!({ "op": "skipStart", "bogus": 1 })),
        ("last-seen-read", json!({ "origin": "devbox", "bogus": 1 })),
        (
            "last-seen-write",
            json!({ "origin": "devbox", "forget": true, "bogus": 1 }),
        ),
    ];
    for (cmd, args) in cases {
        assert_eq!(
            run_on_frame(cmd, args, Some(&own)),
            Err("bad_args".to_string()),
            "{cmd}"
        );
    }
    assert!(!d.join(".cc-monitor").exists(), "多一个键的那几问写了盘");
    let _ = std::fs::remove_dir_all(&d);
}

/// 足迹整张表不上线：命令表里只有它重排成的那一份（`data-report`），没有 `footprint-report`。
#[test]
fn the_raw_footprint_table_is_not_a_frame_command() {
    let names: Vec<&str> = crate::stream::inbound::REGISTRY
        .iter()
        .map(|s| s.name)
        .collect();
    assert!(names.contains(&"data-report"), "{names:?}");
    assert!(!names.contains(&"footprint-report"), "{names:?}");
}

/// 大命令：入参多一个键 ⇒ `bad_args`，`client` 那一层也拒（一份事实都不读）。
#[test]
fn an_extra_key_is_bad_args_on_the_frame_path_for_the_report() {
    let cases = [
        ("data-report", json!({ "bogus": 1 })),
        (
            "data-report",
            json!({ "client": { "home": "/nonexistent-footprint-pin", "path": null, "bogus": 1 } }),
        ),
    ];
    for (cmd, args) in cases {
        assert_eq!(
            run_on_frame(cmd, args.clone(), None),
            Err("bad_args".to_string()),
            "{cmd} {args}"
        );
    }
}
