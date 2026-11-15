//! Codex 历史清单那一面的判据。
//!
//! 守的要求：「codex 合成的项目与会话一起进后端
//! （join 只一个家）」—— 口径逐格照搬 monitor 那两份（枚举 · 首条真用户话），搬家那一刻用户看到的 Codex 会话不许变样。
//!
//! 判据：① 枚举只认 `rollout-<ts>-<uuid>.jsonl`、sid 取末尾 UUID、cwd 取首行 `session_meta`（缺 ⇒ 空串）；
//! ② 首条真用户话跳过注入上下文、拍平数组、取 200 字符；③ 注入上下文那张表与 monitor 渲染那一侧**逐项相等**
//! （运行期读 monitor 源码现抠，异源）。

use super::*;

const SID_A: &str = "0199aaaa-0000-7000-8000-000000000001";
const SID_B: &str = "0199aaaa-0000-7000-8000-000000000002";

fn meta(cwd: &str) -> String {
    format!(r#"{{"timestamp":"t","type":"session_meta","payload":{{"id":"x","cwd":"{cwd}"}}}}"#)
}

fn user(text_json: &str) -> String {
    format!(
        r#"{{"timestamp":"t","type":"response_item","payload":{{"type":"message","role":"user","content":{text_json}}}}}"#
    )
}

#[test]
fn only_rollout_files_are_sessions_and_cwd_comes_from_the_first_line() {
    let home = test_home::scratch("enum");
    let day = home.join("sessions/2026/09/25");
    std::fs::create_dir_all(&day).unwrap();
    std::fs::write(
        day.join(format!("rollout-2026-09-25T01-02-03-{SID_A}.jsonl")),
        format!(
            "{}\n{}\n",
            meta("/w/a"),
            user(r#"[{"type":"input_text","text":"hi"}]"#)
        ),
    )
    .unwrap();
    // 首行不是 session_meta ⇒ cwd 空串（归「(codex)」组）。
    std::fs::write(
        day.join(format!("rollout-2026-09-25T01-02-04-{SID_B}.jsonl")),
        format!("{}\n", user(r#""x""#)),
    )
    .unwrap();
    // 不是 rollout 命名 / 末尾不是 UUID / 不是 .jsonl ⇒ 不算会话。
    std::fs::write(day.join("notes.jsonl"), "{}\n").unwrap();
    std::fs::write(day.join("rollout-2026-09-25T01-02-05-nope.jsonl"), "{}\n").unwrap();
    std::fs::write(
        day.join(format!("rollout-2026-09-25T01-02-06-{SID_A}.txt")),
        "{}\n",
    )
    .unwrap();

    let mut got: Vec<(String, String)> = sessions_under(&home)
        .into_iter()
        .map(|s| (s.sid, s.cwd))
        .collect();
    got.sort();
    assert_eq!(
        got,
        vec![
            (SID_A.to_string(), "/w/a".to_string()),
            (SID_B.to_string(), String::new()),
        ]
    );
    // 没装 Codex（没有会话目录）⇒ 空，不报错。
    assert!(sessions_under(&home.join("nope")).is_empty());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_first_real_user_line_skips_injected_context_and_comes_whole() {
    let d = test_home::scratch("excerpt");
    let p = d.join("r.jsonl");
    let long = "问".repeat(250);
    std::fs::write(
        &p,
        [
            meta("/w"),
            user(r#"[{"type":"input_text","text":"<environment_context>\n<cwd>/w</cwd>"}]"#),
            user(r#"[{"type":"input_text","text":"  # AGENTS.md instructions for /w"}]"#),
            r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"不是用户"}]}}"#.to_string(),
            user(&format!(r#"[{{"type":"input_text","text":"{long}"}}]"#)),
        ]
        .join("\n"),
    )
    .unwrap();
    // 交原文：截 · 压空白是核心的事（`history::search_rules::excerpt_of`）。
    assert_eq!(first_user_excerpt(&p), long);
    // 数组拍平：多段 text 用换行连起来。
    let q = d.join("q.jsonl");
    std::fs::write(
        &q,
        user(r#"[{"type":"input_text","text":"甲"},{"type":"input_image"},{"type":"input_text","text":"乙"}]"#),
    )
    .unwrap();
    assert_eq!(first_user_excerpt(&q), "甲\n乙");
    assert_eq!(first_user_excerpt(&d.join("missing.jsonl")), "");
    let _ = std::fs::remove_dir_all(&d);
}

// 核心取名那一处（经历史清单）认这一家记录格式的那一条挂在 `history` 底下。

/// 按 sid 找那份 rollout（注册表 `RecordFace.find_session` 那一格）：日期树里那一份 · 形状不合法先于 IO 就拒 ·
/// 文件名末尾不是这个 sid 的不算 · 符号链接不算命中 · 找不到照实拒。
#[test]
fn a_session_is_found_by_sid_under_the_dated_tree() {
    let home = test_home::scratch("find");
    let root = parse::sessions_root(&home);
    let day = root.join("2026/10/02");
    std::fs::create_dir_all(&day).unwrap();
    let want = day.join(format!("rollout-2026-10-02T10-00-00-{SID_A}.jsonl"));
    std::fs::write(&want, format!("{}\n", meta("/w"))).unwrap();
    // 名字里带着这个 sid、却不是末尾那一段 ⇒ 不是它。
    std::fs::write(
        day.join(format!("rollout-{SID_A}-{SID_B}.jsonl")),
        format!("{}\n", meta("/w")),
    )
    .unwrap();
    assert_eq!(find_session(&root, SID_A), Ok(want.clone()));
    assert!(find_session(&root, "0199aaaa-0000-7000-8000-00000000000f").is_err());
    assert!(find_session(&root, "../x").is_err());
    assert!(find_session(&home.join("nowhere"), SID_A).is_err());
    // 指过去的链接不算：同一个 sid 只剩链接那一份 ⇒ 找不到。
    #[cfg(unix)]
    {
        std::fs::remove_file(&want).unwrap();
        let elsewhere = home.join(format!("rollout-2026-10-02T10-00-00-{SID_A}.jsonl"));
        std::fs::write(&elsewhere, format!("{}\n", meta("/w"))).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &want).unwrap();
        assert!(find_session(&root, SID_A).is_err());
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// 第一句：前面垫两条注入的上下文 ⇒ 取第三条；人话落在第 201 行 ⇒ 空串（只看前 200 行）。
#[test]
fn the_first_line_skips_two_frames_and_stops_after_two_hundred_lines() {
    let d = test_home::scratch("excerpt-201");
    let p = d.join("r.jsonl");
    std::fs::write(
        &p,
        [
            user(r#"[{"type":"input_text","text":"<environment_context><cwd>/w</cwd></environment_context>"}]"#),
            user(r##"[{"type":"input_text","text":"# AGENTS.md instructions for /w"}]"##),
            user(r#"[{"type":"input_text","text":"第三条"}]"#),
        ]
        .join("\n"),
    )
    .unwrap();
    assert_eq!(first_user_excerpt(&p), "第三条");
    let q = d.join("q.jsonl");
    let mut lines: Vec<String> = (0..200).map(|_| meta("/w")).collect();
    lines.push(user(r#"[{"type":"input_text","text":"太晚了"}]"#));
    std::fs::write(&q, lines.join("\n")).unwrap();
    assert_eq!(first_user_excerpt(&q), "");
    let _ = std::fs::remove_dir_all(&d);
}

/// 两个号各一个家、各一份会话 ⇒ 注册表那一格交两条、各在自己的家底下；账号 0 的家照样列；没有会话目录的家 ⇒ 不出。
#[test]
fn each_account_home_lists_its_own_sessions() {
    use crate::agents::AccountHome;
    let base = test_home::scratch("homes");
    let (a, b, zero) = (
        base.join("acct-a"),
        base.join("acct-b"),
        base.join("dot-codex"),
    );
    for (home, sid) in [(&a, SID_A), (&b, SID_B)] {
        let day = home.join("sessions/2026/11/04");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::write(
            day.join(format!("rollout-2026-11-04T00-00-00-{sid}.jsonl")),
            format!("{}\n", meta("/w")),
        )
        .unwrap();
    }
    let homes = [
        AccountHome {
            account: Some("a".into()),
            home: a.clone(),
        },
        AccountHome {
            account: Some("b".into()),
            home: b.clone(),
        },
        AccountHome {
            account: None,
            home: zero.clone(),
        },
    ];
    let mut got: Vec<(std::path::PathBuf, String)> = (super::super::HISTORY.sessions)(&homes)
        .into_iter()
        .map(|s| (s.path, s.sid))
        .collect();
    got.sort();
    assert_eq!(got.len(), 2, "{got:?}");
    assert!(got[0].0.starts_with(&a) && got[0].1 == SID_A, "{got:?}");
    assert!(got[1].0.starts_with(&b) && got[1].1 == SID_B, "{got:?}");
    // 账号 0 的家里也有一份 ⇒ 照样列。
    let day = zero.join("sessions/2026/11/05");
    std::fs::create_dir_all(&day).unwrap();
    std::fs::write(
        day.join(format!("rollout-2026-11-05T00-00-00-{SID_A}.jsonl")),
        format!("{}\n", meta("/w")),
    )
    .unwrap();
    let zero_rows: Vec<_> = (super::super::HISTORY.sessions)(&homes[2..])
        .into_iter()
        .map(|s| s.path)
        .collect();
    assert_eq!(zero_rows.len(), 1);
    assert!(zero_rows[0].starts_with(&zero), "{zero_rows:?}");
    let _ = std::fs::remove_dir_all(&base);
}

/// 归档了的会话（Codex 挪进平铺的 `<codex home>/archived_sessions/`）照样在清单里、按 sid 照样找得到：
/// 不然界面给了「归档」，归档以后那一条就看不见、点不到「取消归档」。
#[test]
fn archived_sessions_are_listed_and_found_too() {
    let home = test_home::scratch("archived");
    let day = home.join("sessions/2026/10/02");
    let shelf = home.join("archived_sessions");
    std::fs::create_dir_all(&day).unwrap();
    std::fs::create_dir_all(&shelf).unwrap();
    let live = day.join(format!("rollout-2026-10-02T10-00-00-{SID_A}.jsonl"));
    let parked = shelf.join(format!("rollout-2026-10-01T10-00-00-{SID_B}.jsonl"));
    std::fs::write(&live, format!("{}\n", meta("/w"))).unwrap();
    std::fs::write(&parked, format!("{}\n", meta("/v"))).unwrap();

    let mut got: Vec<(String, std::path::PathBuf, String)> = sessions_under(&home)
        .into_iter()
        .map(|s| (s.sid, s.path, s.cwd))
        .collect();
    got.sort();
    assert_eq!(
        got,
        [
            (SID_A.to_string(), live.clone(), "/w".to_string()),
            (SID_B.to_string(), parked.clone(), "/v".to_string()),
        ]
    );
    // 按 sid 找照根找：归档了的那一份在归档根下，日期树那个根下不再兜着找。
    let root = parse::sessions_root(&home);
    assert_eq!(find_session(&shelf, SID_B), Ok(parked));
    assert!(find_session(&root, SID_B).is_err());
    assert_eq!(find_session(&root, SID_A), Ok(live));
    let _ = std::fs::remove_dir_all(&home);
}

/// 根表那一格：每个家两个根（日期树 · 归档根），家照交来的次序。
#[test]
fn every_home_has_the_dated_root_and_the_archive_root() {
    let homes = [
        crate::agents::AccountHome {
            account: None,
            home: "/h/.codex".into(),
        },
        crate::agents::AccountHome {
            account: Some("a".into()),
            home: "/h/.cc-monitor/accounts/a".into(),
        },
    ];
    let p = std::path::PathBuf::from;
    assert_eq!(
        records_roots(&homes),
        [
            p("/h/.codex/sessions"),
            p("/h/.codex/archived_sessions"),
            p("/h/.cc-monitor/accounts/a/sessions"),
            p("/h/.cc-monitor/accounts/a/archived_sessions"),
        ]
    );
    assert!(records_roots(&[]).is_empty());
}
