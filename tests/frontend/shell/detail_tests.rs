//! 壳这一端写的复制详情：远端拒绝补「本机」、本机拒绝不补；通道断了壳写全份；壳命令失败那一形。

use super::*;
use crate::origin::Origin;

fn label(k: &str) -> String {
    copy_text(k, &[])
}

#[test]
fn a_remote_refusal_gets_a_local_line_and_a_local_one_does_not() {
    let remote = format!("{}：kill_failed", label("detail.label.code"));
    let got = relayed(&Origin("devbox".into()), &remote);
    assert!(got.starts_with(&remote), "{got}");
    assert!(
        got.contains(&format!("\n{}：cc-monitor ", label("detail.label.local"))),
        "{got}"
    );
    assert_eq!(relayed(&Origin::local(), &remote), remote);
}

#[test]
fn the_refusal_body_detail_is_read_and_missing_reads_empty() {
    assert_eq!(
        of_refusal_body(br#"{"code":"x","message":"m","detail":"d"}"#),
        "d"
    );
    assert_eq!(of_refusal_body(br#"{"code":"x","message":"m"}"#), "");
    assert_eq!(of_refusal_body(b"not json"), "");
}

#[test]
fn a_channel_failure_detail_names_the_machine_command_hop_and_code() {
    let e = w::CallError::Hop {
        at: w::HopId {
            idx: 1,
            tag: "open",
        },
        reach: w::Reach::NotSent,
        why: w::HopFault::Unreachable,
    };
    let d = of_channel(&Origin("devbox".into()), "accounts-list", &e);
    for want in [
        format!(
            "{}：devbox（{}）",
            label("detail.label.machine"),
            label("detail.value.notConnected")
        ),
        format!("{}：accounts-list", label("detail.label.command")),
        format!("{}：1:open NotSent", label("detail.label.hop")),
        format!("{}：Unreachable", label("detail.label.code")),
        format!("{}：cc-monitor ", label("detail.label.local")),
    ] {
        assert!(d.contains(&want), "缺「{want}」：\n{d}");
    }
    let local = of_channel(&Origin::local(), "kill", &w::OursFault::Broken.into());
    assert!(
        !local.contains(&format!("{}：cc-monitor", label("detail.label.local"))),
        "{local}"
    );
    assert!(
        local.contains(&format!("{}：Broken", label("detail.label.code"))),
        "{local}"
    );
}

#[test]
fn a_shell_command_failure_is_one_shape_with_a_nonempty_detail() {
    let s = Said::new("读取配置失败", "load_config", Some("os error 2"));
    assert_eq!(s.said, "读取配置失败");
    assert!(s
        .detail
        .contains(&format!("{}：load_config", label("detail.label.command"))));
    assert!(s
        .detail
        .contains(&format!("{}：os error 2", label("detail.label.raw"))));
    let v = serde_json::to_value(&s).unwrap();
    assert_eq!(v.as_object().unwrap().len(), 2, "{v}");
}

/// 问后端一条命令没成：那台写了详情 ⇒ 原样带上（远端补「本机」）；没写（老后端 / 没走到那台）⇒ 壳写时刻 · 本机 · 命令。
#[test]
fn a_backend_call_failure_keeps_the_detail_the_backend_wrote() {
    use crate::inbound_client::CallError;
    let wrote = format!(
        "{}：refused\n{}：uname: not found",
        label("detail.label.code"),
        label("detail.label.raw")
    );
    let refused = CallError::Remote {
        code: "refused".into(),
        message: "box 系统未知".into(),
        detail: wrote.clone(),
        data: None,
    };
    let s = Said::of_call(
        "box 系统未知".into(),
        "deploy-plan",
        &Origin::local(),
        &refused,
    );
    assert_eq!(s.said, "box 系统未知");
    assert_eq!(s.detail, wrote, "本机那台写的那份原样");
    let far = Said::of_call(
        "x".into(),
        "deploy-plan",
        &Origin("devbox".into()),
        &refused,
    );
    assert!(far.detail.starts_with(&wrote), "{}", far.detail);
    assert!(
        far.detail
            .contains(&format!("\n{}：cc-monitor ", label("detail.label.local"))),
        "{}",
        far.detail
    );
    let s = Said::of_call(
        "y".into(),
        "deploy-plan",
        &Origin::local(),
        &CallError::Disconnected,
    );
    assert!(
        s.detail
            .contains(&format!("{}：deploy-plan", label("detail.label.command"))),
        "{}",
        s.detail
    );
}

/// 已有「命令」那一项（自己写的 · 那台后端写的）⇒ 不补第二项；只有时刻 · 本机那两项 ⇒ 补上命令名。
#[test]
fn naming_a_shell_command_failure_adds_the_command_once() {
    let cmd = |d: &str| {
        d.lines()
            .filter(|l| l.starts_with(&format!("{}：", label("detail.label.command"))))
            .count()
    };
    let bare = Said::from("读取失败".to_string()).named("load_config");
    assert_eq!(cmd(&bare.detail), 1, "{}", bare.detail);
    assert!(
        bare.detail
            .ends_with(&format!("{}：load_config", label("detail.label.command"))),
        "{}",
        bare.detail
    );
    let own = Said::new("x", "deploy-plan", Some("raw")).named("deploy_remote_backend");
    assert_eq!(cmd(&own.detail), 1, "{}", own.detail);
    assert!(own.detail.contains("deploy-plan"), "{}", own.detail);
}

/// 每条返回 `Said` 的壳命令，最外层都经 `.named("<它自己的名字>")` 一次（复制详情里有「命令」那一项）。人群：生产源码里全部 `#[tauri::command]`。
#[test]
fn every_shell_command_names_itself_in_its_failure_detail() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let (mut seen, mut bad) = (0usize, Vec::new());
    for (p, text) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        let prod = guard_core::production_code(&text);
        let starts: Vec<usize> = prod
            .match_indices("#[tauri::command]")
            .map(|(i, _)| i)
            .collect();
        for (k, at) in starts.iter().enumerate() {
            let end = starts.get(k + 1).copied().unwrap_or(prod.len());
            let chunk = &prod[*at..end];
            let Some(f) = chunk.find("fn ") else { continue };
            let name: String = chunk[f + 3..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let sig_end = chunk[f..].find('{').map_or(chunk.len(), |b| f + b);
            if !chunk[..sig_end].contains("Said>") {
                continue;
            }
            seen += 1;
            if !chunk.contains(&format!(".named(\"{name}\")")) {
                bad.push(format!(
                    "{}::{name}",
                    p.file_name().unwrap().to_string_lossy()
                ));
            }
        }
    }
    assert!(seen >= 30, "人群只扫到 {seen} 条 —— 取法坏了");
    assert!(
        bad.is_empty(),
        "这几条壳命令失败时复制详情里没有命令名：{bad:?}"
    );
}

/// 补上的「命令」排在原话前面（项名次序照 `Label::ALL`）。
#[test]
fn the_named_command_goes_before_the_raw_words() {
    let s = Said::with_raw("x".into(), "boom").named("open_log_dir");
    let lines: Vec<&str> = s.detail.lines().collect();
    let at = |k: &str| {
        lines
            .iter()
            .position(|l| l.starts_with(&format!("{}：", label(k))))
            .unwrap()
    };
    assert!(
        at("detail.label.command") < at("detail.label.raw"),
        "{}",
        s.detail
    );
}
