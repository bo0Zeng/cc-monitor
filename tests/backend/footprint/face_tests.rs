//! # 要求：「`tool_registry` ＋ `build_rows` 进后端，`HostScope::Client` 那几行照旧由 monitor 答」
//!
//! 核原文：同一裁逐字「用户 09-27「一处后端」压过「tool_registry 只住 monitor」」；红线「查不了的显示成
//! 「未确定 ＋ 为什么」，绝不显示成「缺失」」·「远端也有真栏」。本族在临时目录上真 stat 真读，判帧面 `footprint-report` 的
//! 两种问法，各一问：远端那一栏（住 monitor 那台的那一族不进人群）· 本机那一栏（monitor 只交它自己进程的几条事实，四拍收成两拍）。
//! 原 `footprint-probe` 那几条（只交事实）随实现合到这里。
//!
//! # 买不到的
//!
//! - 🔴 真远端：环境是那台**后端进程**的，不是用户交互 shell 的。
//! - 真 monitor 那一趟（`footprint_client_facts`）：monitor 侧 `tests/frontend/shell/footprint_client_tests.rs`。

use super::*;
use crate::footprint::registry::HostScope;
use crate::footprint::rows::host_label;

fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-footprint-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("建临时目录");
    p
}

fn env_of(pairs: Vec<(&'static str, String)>) -> impl Fn(&str) -> Option<String> {
    move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone())
}

fn rows_of(reply: &Value) -> Vec<Value> {
    serde_json::from_value(reply["rows"].clone()).expect("rows")
}

fn key(r: &Value) -> (String, String) {
    (
        r["tool_id"].as_str().unwrap().to_string(),
        r["path_declared"].as_str().unwrap().to_string(),
    )
}

/// 那一行的状态的 `kind`（`present` / `absent` / `undetermined`）。
fn kind_of(rows: &[Value], label: &str, declared: &str) -> String {
    rows.iter()
        .find(|r| r["host_label"] == label && r["path_declared"] == declared)
        .unwrap_or_else(|| panic!("找不到 {label} 那一行 {declared}"))["state"]["kind"]
        .as_str()
        .unwrap()
        .to_string()
}

/// 同一份 `build_rows` 在「monitor 那台」视角下的人群（`with_client` = 含 `本机` 那一族），按 (工具, 申报路径) 比。
fn monitor_vantage_population(home: &Path, with_client: bool) -> Vec<(String, String)> {
    let empty = FsProbe {
        meta: &|_| None,
        list: &|_| None,
    };
    let agent = home.join(".claude");
    let env = SurfaceEnv {
        home,
        agent_home: &agent,
        fs: &empty,
        path_env: None,
        vantage: Vantage::Monitor,
    };
    let client = host_label(HostScope::Client);
    build_rows(&env, with_client.then_some(&env))
        .into_iter()
        .filter(|r| with_client || r.host_label != client)
        .map(|r| (r.tool_id.to_string(), r.path_declared.to_string()))
        .collect()
}

/// ★ 远端那一栏：一问即得；住 monitor 那台的那一族一行都没有；这台的落点**真查**（在 ⇒ 在，不在 ⇒ 缺）；
/// 「本机或远端」那一行没找到 ⇒ 未确定（不说「缺」）；settings 两格按这台的 agent 家解、钩子字样真被读到、内容一个字节不出线。
#[test]
fn the_remote_column_really_probes_this_machine_and_drops_the_monitor_rows() {
    let h = temp_dir("remote");
    std::fs::create_dir_all(h.join(".cc-monitor/bin")).unwrap();
    std::fs::write(h.join(".cc-monitor/bin/ccm"), "x").unwrap();
    let agent = h.join(".claude");
    std::fs::create_dir_all(&agent).unwrap();
    let secretish = "SENTENCE-THAT-MUST-NOT-CROSS-THE-WIRE";
    std::fs::write(
        agent.join("settings.json"),
        format!("{{\"cmd\":\"cc-register\",\"x\":\"{secretish}\"}}"),
    )
    .unwrap();
    let got = answer_with(
        &env_of(vec![("HOME", h.display().to_string())]),
        &agent,
        &json!({}),
    )
    .unwrap();
    let rows = rows_of(&got);
    assert_eq!(
        rows.iter().map(key).collect::<Vec<_>>(),
        monitor_vantage_population(&h, false),
        "远端视角的人群不等于「monitor 那台视角去掉住 monitor 那台的那一族」"
    );
    let remote = host_label(HostScope::Remote);
    let either = host_label(HostScope::Either);
    assert_eq!(kind_of(&rows, &remote, "~/.cc-monitor/bin/ccm"), "present");
    // 同一个落点拿走之后再问一次：这台上真不在 ⇒ 缺（不是「本页不连 SSH」）。
    std::fs::remove_file(h.join(".cc-monitor/bin/ccm")).unwrap();
    let gone = answer_with(
        &env_of(vec![("HOME", h.display().to_string())]),
        &agent,
        &json!({}),
    )
    .unwrap();
    assert_eq!(
        kind_of(&rows_of(&gone), &remote, "~/.cc-monitor/bin/ccm"),
        "absent",
        "这台上真不在的远端落点 ⇒ 缺（不再是「本页不连 SSH」）"
    );
    assert_eq!(
        kind_of(&rows, &either, "~/.claude/skills/cc-bus"),
        "undetermined",
        "「本机或远端」那一行在这台上没找到，被说成了「缺」"
    );
    assert_eq!(got["claude_config_dir"], agent.display().to_string());
    assert_eq!(got["settings_scopes"][0]["has_cc_bus_hooks"], true);
    assert_eq!(
        got["settings_scopes"][1]["has_cc_bus_hooks"],
        Value::Null,
        "不在的那一份 ⇒ 不猜"
    );
    // 正控：同一把尺子在那份文件里数得到那句内容；应答里数不到。
    assert!(std::fs::read_to_string(agent.join("settings.json"))
        .unwrap()
        .contains(secretish));
    assert!(
        !serde_json::to_string(&got).unwrap().contains(secretish),
        "应答里带着文件内容"
    );
    let _ = std::fs::remove_dir_all(&h);
}

/// ★ 本机那一栏一问：monitor 只交它自己进程的那几条（家目录 · PATH；agent 家用这台后端自己解析的那一个），人群 == monitor 那台视角（含 `本机` 那一族），
/// 那一族按 **monitor 交来的**家目录解、由这台 stat（同一台、同一用户）—— 放在那个家目录下的文件答「在」，这台自己的家目录不掺进来。
#[test]
fn the_local_column_resolves_the_monitor_rows_under_the_monitor_facts() {
    let h = temp_dir("local");
    let agent = h.join(".claude");
    let m = temp_dir("local-monitor");
    std::fs::create_dir_all(m.join(".cc-monitor/bin")).unwrap();
    std::fs::write(m.join(".cc-monitor/bin/ccm"), "x").unwrap();
    let client = json!({ "home": m.display().to_string(), "path": "/m/bin" });
    let got = answer_with(
        &env_of(vec![("HOME", h.display().to_string())]),
        &agent,
        &json!({ "client": client }),
    )
    .unwrap();
    let rows = rows_of(&got);
    assert_eq!(
        rows.iter().map(key).collect::<Vec<_>>(),
        monitor_vantage_population(&h, true),
        "本机那一栏的人群不等于 monitor 那台视角"
    );
    let client_label = host_label(HostScope::Client);
    let own: Vec<&Value> = rows
        .iter()
        .filter(|r| r["host_label"] == client_label.as_str() && r["path_resolved"].is_string())
        .collect();
    assert!(
        !own.is_empty(),
        "`本机` 那一族一条路径都没解析 —— 下面是空真"
    );
    for r in &own {
        let p = r["path_resolved"].as_str().unwrap();
        assert!(
            !p.starts_with(&h.display().to_string()),
            "按这台的家目录解了 monitor 那一族：{r}"
        );
    }
    assert_eq!(
        kind_of(&rows, &client_label, "~/.cc-monitor/bin/ccm*"),
        "present",
        "monitor 家目录下那一份没被这台 stat 到"
    );
    let _ = std::fs::remove_dir_all(&h);
    let _ = std::fs::remove_dir_all(&m);
}

#[test]
fn a_directory_over_the_cap_is_unlistable_not_truncated() {
    let d = temp_dir("cap");
    for i in 0..=MAX_ENTRIES {
        std::fs::write(d.join(format!("n{i}")), "").unwrap();
    }
    assert_eq!(meta_of(&d), Some((true, 0)));
    assert_eq!(
        list_of(&d),
        None,
        "超过 {MAX_ENTRIES} 个名字应当报「列不动」，不是一份截断的清单"
    );
    std::fs::remove_file(d.join("n0")).unwrap();
    assert_eq!(list_of(&d).map(|v| v.len()), Some(MAX_ENTRIES));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn hooks_answer_yes_no_or_unknown() {
    let d = temp_dir("hooks");
    std::fs::write(d.join("with.json"), "{\"cmd\":\"cc-bus-stop-hook\"}").unwrap();
    std::fs::write(d.join("without.json"), "{}").unwrap();
    assert_eq!(hooks_in(&d.join("with.json")), Some(true));
    assert_eq!(hooks_in(&d.join("without.json")), Some(false));
    assert_eq!(hooks_in(&d.join("absent")), None, "不在的文件 ⇒ 不猜");
    assert_eq!(hooks_in(&d), None, "目录不是文件 ⇒ 不猜");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn bad_arguments_are_refused() {
    let d = temp_dir("args");
    let get = env_of(vec![("HOME", d.display().to_string())]);
    let cases = [
        json!({ "client": [] }),
        json!({ "client": { "home": "rel" } }),
        json!({ "client": { "path": "/m/bin" } }),
    ];
    for args in cases {
        let err = answer_with(&get, &d, &args).expect_err("坏入参还成功了");
        assert_eq!(err.0, "bad_args", "{args} 应当是 bad_args，实得 {err:?}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn home_falls_back_to_userprofile_and_is_required() {
    let d = temp_dir("home");
    let got = answer_with(
        &env_of(vec![("USERPROFILE", d.display().to_string())]),
        &d.join(".claude"),
        &json!({}),
    )
    .unwrap();
    assert_eq!(
        got["home"],
        d.display().to_string(),
        "HOME 缺 ⇒ 退 USERPROFILE"
    );
    let (code, _) = answer_with(&env_of(vec![]), &d, &json!({})).unwrap_err();
    assert_eq!(code, "failed", "没有家目录就解不了 `~/…`，不猜");
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ 线上成品两侧对拍：一台什么都没装的机器（家目录不在、`PATH` 取不到）上远端那一栏的整份应答 == 跨语言金样
/// `tests/__fixtures__/footprint-report.golden.json`（界面 `src/frontend/ui/settings/footprint-reads.ts::decodeFootprint` 读同一份）。
#[cfg(not(windows))]
#[test]
fn the_report_wire_matches_the_cross_language_golden() {
    let home = "/nonexistent-footprint-golden";
    let got = answer_with(
        &env_of(vec![("HOME", home.to_string())]),
        &Path::new(home).join(".claude"),
        &json!({}),
    )
    .unwrap();
    let path =
        crate::guard_support::repo_root().join("tests/__fixtures__/footprint-report.golden.json");
    let want: Value = serde_json::from_str(
        &std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到金样 {path:?}：{e}")),
    )
    .unwrap();
    assert_eq!(
        got, want,
        "后端产出与金样不等 —— 真改了成品就重打金样（界面解码器读同一份）"
    );
}
