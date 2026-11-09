//! 资产域判据里引到上层的那几条（命令表 `stream::inbound::REGISTRY` · 分派那一层的手 · 子命令闸门 · 远端 CLI 面的 stdin 上限）：
//! 资产域是独立的包（`backend-assets`）、够不到上层 ⇒ 挂在主 crate（`cross/assets.rs`）。其余资产域的判据住各自模块底下。

use crate::assets::asset_sync::{PULL_FLAG, PUSH_FLAG, PUSH_MAX_BYTES};
use crate::wire::spec::{invoke, Prepared};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// 按命令名找那一行登记、照它的档跑一问（阻塞档就地跑；异步档照它的未来跑完），只留码。
fn run_on_frame_path(
    cmd: &str,
    args: Value,
    own: Option<&crate::platform::fs::Own>,
) -> Result<(), Option<String>> {
    let spec = crate::stream::inbound::REGISTRY
        .iter()
        .find(|s| s.name == cmd)
        .unwrap_or_else(|| panic!("{cmd} 不在 REGISTRY"));
    let req = crate::wire::Request {
        id: "t".to_string(),
        cmd: cmd.to_string(),
        args,
        ..Default::default()
    };
    match invoke(spec.run, &crate::stream::inbound::hands(own)) {
        Some(Prepared::Blocking(run)) => run(req),
        Some(Prepared::Async(run)) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(run(req)),
        _ => panic!("{cmd} 不在阻塞档也不在异步档"),
    }
    .map(|_| ())
    .map_err(|f| f.code)
}

// ── 资产目录那一族 ─────────────────────────────────────────────────────────

/// 帧面那一跳（按名找命令、照它的档跑）：入参多一个键 ⇒ `bad_args`、一个字节不写（原来不管多出来的键）。
#[test]
fn an_extra_key_is_bad_args_on_the_frame_path() {
    let d = test_home::scratch("extra");
    let own = crate::platform::fs::Own::issue_at(&d);
    let cases = [
        ("assets-catalog", json!({ "bogus": 1 })),
        ("mcp-read", json!({ "projectDir": null, "bogus": 1 })),
        ("hooks-diag", json!({ "bogus": 1 })),
        ("ext-list", json!({ "visit": false, "bogus": 1 })),
        ("ext-list-here", json!({ "visit": false, "bogus": 1 })),
        (
            "ext-uninstall-preview",
            json!({ "kind": "skill", "name": "a", "at": { "level": "user" }, "bogus": 1 }),
        ),
        (
            "ext-uninstall-apply",
            json!({ "kind": "skill", "name": "a", "at": { "level": "user" }, "token": "t", "bogus": 1 }),
        ),
        (
            "assets-catalog-merge",
            json!({ "catalog": { "machines": [] }, "bogus": 1 }),
        ),
        (
            "ext-note-set",
            json!({ "kind": "skill", "name": "a", "text": "t", "bogus": 1 }),
        ),
    ];
    for (cmd, args) in cases {
        let got = run_on_frame_path(cmd, args, Some(&own));
        assert_eq!(got, Err(Some("bad_args".to_string())), "{cmd}");
    }
    assert!(!d.join(".cc-monitor").exists(), "多一个键的那几问写了盘");
    let _ = std::fs::remove_dir_all(&d);
}

// ── 枢纽问各台的那 8 条内层命令 ──────────────────────────────────────────────

/// 入参多一个键 ⇒ `bad_args`（入参照拒；回包不拒多的键，见 `hub_tests`）。经本进程那一行登记跑（同枢纽本机那一跳）。
#[test]
fn an_inner_request_with_an_extra_key_is_bad_args() {
    let cases = [
        ("skill-read", json!({ "name": "demo", "bogus": 1 })),
        (
            "skill-install-plan",
            json!({ "name": "demo", "source": [], "bogus": 1 }),
        ),
        (
            "mcp-sync-source",
            json!({ "name": "s", "at": { "level": "user" }, "bogus": 1 }),
        ),
        (
            "mcp-sync-preview",
            json!({ "name": "s", "at": { "level": "user" }, "def": {}, "bogus": 1 }),
        ),
        ("cc-bus-install-state", json!({ "bogus": 1 })),
    ];
    for (cmd, args) in cases {
        let got = run_on_frame_path(cmd, args, None);
        assert_eq!(got, Err(Some("bad_args".to_string())), "{cmd}");
    }
}

// ── 资产目录的自动同步 ───────────────────────────────────────────────────────

/// 一块载荷（连外面那层 `{"catalog":{"machines":[…]}}` 与换行）装得进远端 CLI 面 stdin 的上限 —— 否则切出来的块对面整块拒。
#[test]
fn one_push_chunk_fits_under_the_remote_cli_stdin_cap() {
    let overhead = "{\"catalog\":{\"machines\":[]}}\n".len() as u64;
    assert!(
        PUSH_MAX_BYTES.bytes + overhead <= crate::wire::cli_args::MAX_CLI_STDIN.bytes,
        "PUSH_MAX_BYTES（{}）＋ 外层 {overhead} 字节 > 远端 CLI 面 stdin 上限 —— 切块上限要跟着它",
        PUSH_MAX_BYTES.bytes
    );
}

/// 两条一次性子命令与后端真登记的同名（跨半边：这里的常量 ↔ 子命令闸门 `stream::cli::is_subcommand` ↔ `inbound::REGISTRY`）。
#[test]
fn the_remote_flags_are_real_subcommands_of_the_catalog_commands() {
    for (flag, cmd) in [
        (PULL_FLAG, "assets-catalog"),
        (PUSH_FLAG, "assets-catalog-merge"),
    ] {
        assert!(crate::stream::cli::is_subcommand(flag), "{flag} 不是子命令");
        assert_eq!(format!("--{cmd}"), flag);
        let spec = crate::stream::inbound::REGISTRY
            .iter()
            .find(|s| s.name == cmd)
            .unwrap_or_else(|| panic!("{cmd} 不在 REGISTRY"));
        assert_eq!(
            spec.takes_input,
            flag == PUSH_FLAG,
            "{cmd} 收不收 stdin 与推 / 拉那一形对不上"
        );
    }
}

/// `assets-sync` 帧面那一跳（按名找命令、照档跑）：入参多一个键 ⇒ `bad_args`，一次都不拨、可达表不动。
#[test]
fn an_extra_sync_key_is_bad_args_on_the_frame_path() {
    let d = test_home::scratch("sync-extra");
    let own = crate::platform::fs::Own::issue_at(&d);
    let reach = &crate::dial::remote_ask::REACH;
    let before = crate::dial::remote_ask::reach_rows(reach).len();
    for args in [
        json!({ "bogus": 1 }),
        json!({ "origin": "o-extra", "dial": {"machine": {"host": "x"}}, "bogus": 1 }),
    ] {
        let got = run_on_frame_path("assets-sync", args.clone(), Some(&own));
        assert_eq!(got, Err(Some("bad_args".to_string())), "{args}");
    }
    assert_eq!(
        crate::dial::remote_ask::reach_rows(reach).len(),
        before,
        "多一个键的那一问动了可达表"
    );
    assert!(!d.join(".cc-monitor").exists(), "多一个键的那一问写了盘");
    let _ = std::fs::remove_dir_all(&d);
}

// ── 跨语言金样里的码集合 == 登记表 ───────────────────────────────────────────

/// 别名那一族（`aliases-*`）的码集合 == 金样里的 `codes`（界面 `src/frontend/ui/alias-reads.ts` 的解码器读同一份）。
#[test]
fn the_alias_codes_match_the_cross_language_golden() {
    let g: Value =
        serde_json::from_str(include_str!("../../__fixtures__/aliases.golden.json")).unwrap();
    let codes: BTreeMap<String, Vec<String>> = crate::stream::inbound::REGISTRY
        .iter()
        .filter(|s| s.name.starts_with("aliases-"))
        .map(|s| {
            (
                s.name.to_string(),
                s.codes.iter().map(|c| c.to_string()).collect(),
            )
        })
        .collect();
    assert_eq!(serde_json::to_value(&codes).unwrap(), g["codes"]);
}

/// skill 装 / 卸那几条的拒绝码 == 金样里的 `codes`。
#[test]
fn the_skill_flow_codes_match_the_cross_language_golden() {
    let g: Value =
        serde_json::from_str(include_str!("../../__fixtures__/skill-flow.golden.json")).unwrap();
    for (name, want) in g["codes"].as_object().unwrap() {
        let spec = crate::stream::inbound::REGISTRY
            .iter()
            .find(|s| s.name == name)
            .expect("登记表里没有");
        let mut codes: Vec<&str> = spec.codes.to_vec();
        codes.sort_unstable();
        let want: Vec<&str> = want
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap())
            .collect();
        assert_eq!(codes, want, "`{name}` 的拒绝码与金样不相等");
    }
}
