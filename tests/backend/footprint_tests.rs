//! 〔RM1a · 第四波〕`footprint.rs` 的判据 —— 「足迹」的这台机器那一半（只读路径事实）。
//!
//! # 买到的（临时目录上真 stat 真读）
//!
//! - 三种事实分得开：文件（带大小）· 目录（带一层名字）· 不在（`null`）。
//! - 目录名字超上限 ⇒ `entries: null`（列不动），**不截断**（截断的清单会被当成完整的去数 glob）。
//! - 字样：有 ⇒ `true`、没有 ⇒ `false`、读不动 / 太大 / 是目录 ⇒ `null`（不猜）；
//!   🔴 应答里**一个字节的文件内容都没有**（正控：同一把尺子在那份文件里数得到那句内容）。
//! - 入参闸：相对路径 · 超条数 · 给了路径没给字样 · 空字样 各拒一次。
//! - 环境从注入的取值器来（`HOME` 缺 ⇒ 退 `USERPROFILE`），agent 家目录是入参。
//!
//! # 买不到的
//!
//! - 🔴 真远端：环境是那台**后端进程**的，不是用户交互 shell 的（模块头注）。
//! - 判定对不对（哪一行属于哪个工具）：那在 monitor 的 `config_surface`，不在这里。

use super::*;

fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-footprint-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("建临时目录");
    p
}

fn env_of(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    move |k| {
        pairs
            .iter()
            .find(|(n, _)| *n == k)
            .map(|(_, v)| v.to_string())
    }
}

#[test]
fn file_dir_and_absent_are_told_apart() {
    let d = temp_dir("kinds");
    std::fs::write(d.join("f.txt"), "12345").unwrap();
    std::fs::create_dir(d.join("sub")).unwrap();
    std::fs::write(d.join("sub").join("a"), "").unwrap();
    std::fs::write(d.join("sub").join("b"), "").unwrap();
    let paths: Vec<String> = ["f.txt", "sub", "nope"]
        .iter()
        .map(|n| d.join(n).display().to_string())
        .collect();
    let got = answer_with(&env_of(&[]), &d, &json!({ "stat": paths })).unwrap();
    let st = &got["stat"];
    assert_eq!(st[&paths[0]], json!({ "kind": "file", "size": 5 }));
    assert_eq!(st[&paths[1]]["kind"], "dir");
    let mut names: Vec<String> = serde_json::from_value(st[&paths[1]]["entries"].clone()).unwrap();
    names.sort();
    assert_eq!(names, vec!["a", "b"]);
    assert!(
        st[&paths[2]].is_null(),
        "不在的路径应当是 null：{}",
        st[&paths[2]]
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_directory_over_the_cap_is_reported_as_unlistable_not_truncated() {
    let d = temp_dir("cap");
    let big = d.join("big");
    std::fs::create_dir(&big).unwrap();
    for i in 0..=MAX_ENTRIES {
        std::fs::write(big.join(format!("n{i}")), "").unwrap();
    }
    let p = big.display().to_string();
    let got = answer_with(&env_of(&[]), &d, &json!({ "stat": [p.clone()] })).unwrap();
    assert_eq!(got["stat"][&p]["kind"], "dir");
    assert!(
        got["stat"][&p]["entries"].is_null(),
        "超过 {MAX_ENTRIES} 个名字应当报「列不动」，不是一份截断的清单"
    );
    assert!(
        got["stat"][&p]["notice"]
            .as_str()
            .is_some_and(|n| n.contains("没列")),
        "降级了却没说为什么：{}",
        got["stat"][&p]
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn hooks_answer_yes_no_or_unknown_and_never_carry_the_content() {
    let d = temp_dir("hooks");
    let with = d.join("with.json");
    let without = d.join("without.json");
    let secretish = "SENTENCE-THAT-MUST-NOT-CROSS-THE-WIRE";
    std::fs::write(
        &with,
        format!("{{\"cmd\":\"cc-register\",\"x\":\"{secretish}\"}}"),
    )
    .unwrap();
    std::fs::write(&without, format!("{{\"x\":\"{secretish}\"}}")).unwrap();
    let paths: Vec<String> = [&with, &without, &d.join("absent"), &d]
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    let got = answer_with(
        &env_of(&[]),
        &d,
        &json!({ "hooks": { "paths": paths, "needles": ["cc-register", "cc-bus-stop-hook"] } }),
    )
    .unwrap();
    let h = &got["hooks"];
    assert_eq!(h[&paths[0]], true);
    assert_eq!(h[&paths[1]], false);
    assert!(h[&paths[2]].is_null(), "不在的文件应当是 null（不猜）");
    assert!(h[&paths[3]].is_null(), "目录不是文件，应当是 null");
    // 答 `null` 的那两条各自说了为什么；答了布尔的两条不带话。
    let n = &got["notices"];
    assert!(
        n[&paths[2]].is_string() && n[&paths[3]].is_string(),
        "null 没说为什么：{n}"
    );
    assert!(
        n.get(&paths[0]).is_none() && n.get(&paths[1]).is_none(),
        "答了布尔还带话：{n}"
    );
    // 正控：同一把尺子在那份文件里数得到那句内容；应答里数不到。
    assert!(std::fs::read_to_string(&with).unwrap().contains(secretish));
    let wire = serde_json::to_string(&got).unwrap();
    assert!(!wire.contains(secretish), "应答里带着文件内容：{wire}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn bad_arguments_are_refused() {
    let d = temp_dir("args");
    let many: Vec<String> = (0..=MAX_PATHS).map(|i| format!("/p{i}")).collect();
    let cases = [
        (json!({ "stat": "not-an-array" }), "bad_args"),
        (json!({ "stat": ["relative/path"] }), "bad_args"),
        (json!({ "stat": [3] }), "bad_args"),
        (json!({ "stat": many }), "too_large"),
        (json!({ "hooks": { "paths": ["/x"] } }), "bad_args"),
        (
            json!({ "hooks": { "paths": ["/x"], "needles": [""] } }),
            "bad_args",
        ),
    ];
    for (args, code) in cases {
        let err = answer_with(&env_of(&[]), &d, &args).expect_err("坏入参还成功了");
        assert_eq!(err.0, code, "{args} 应当是 {code}，实得 {err:?}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_environment_comes_from_the_getter_and_home_falls_back() {
    let d = temp_dir("env");
    let got = answer_with(
        &env_of(&[("HOME", "/h/u"), ("PATH", "/a:/b")]),
        &d,
        &json!({}),
    )
    .unwrap();
    assert_eq!(got["env"]["home"], "/h/u");
    assert_eq!(got["env"]["path"], "/a:/b");
    assert_eq!(got["env"]["agentHome"], d.display().to_string());
    assert_eq!(got["env"]["agentHomeIsDir"], true);
    let got = answer_with(
        &env_of(&[("USERPROFILE", "C:\\Users\\u")]),
        &d.join("no"),
        &json!({}),
    )
    .unwrap();
    assert_eq!(
        got["env"]["home"], "C:\\Users\\u",
        "HOME 缺 ⇒ 退 USERPROFILE"
    );
    assert!(
        got["env"]["path"].is_null(),
        "PATH 缺 ⇒ null（查不动，不是空）"
    );
    assert_eq!(got["env"]["agentHomeIsDir"], false);
    let _ = std::fs::remove_dir_all(&d);
}
