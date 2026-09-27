//! # 要求住址：用户裁决 `99 §1` V138（ccm 是 claude 的壳）
//!
//! 核原文：「判据：ccm 壳层选项集合与真 claude `--help` 旗标集合交集为空（claude 以后新增同名旗标 ⇒ 红）」。
//! 异源：一侧是真解析器 `argv::parse` 的行为（这个词被 ccm 吃掉没有），另一侧是 claude 自己的 `--help` ——
//! 固定快照 `tests/__fixtures__/claude-help.snapshot.txt`（首行记版本）；PATH 上有 `claude` 时再加读真的一份。

use super::*;
use std::collections::BTreeSet;

const SNAPSHOT: &str = include_str!("../../../__fixtures__/claude-help.snapshot.txt");

/// `--help` 里的旗标：选项行是两格缩进、`-` 开头；取名字那一栏（到两个空格为止），逗号分开的每一形。
fn flags_of(help: &str) -> BTreeSet<String> {
    help.lines()
        .filter(|l| l.starts_with("  -"))
        .flat_map(|l| {
            let name_col = l.trim_start().split("  ").next().unwrap_or("");
            name_col
                .split(", ")
                .filter_map(|t| t.split([' ', '=', '<', '[']).next())
                .filter(|t| t.starts_with('-'))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// 被 ccm 吃掉（或拒掉）的那几个：单独喂 `parse`，不是原样落进透传的就算。
fn eaten_by_ccm(flags: &BTreeSet<String>) -> BTreeSet<String> {
    flags
        .iter()
        .filter(|f| {
            !matches!(parse(&[f.to_string()]),
                Ok(Parsed::Opts(o)) if o.passthru == [f.to_string()])
        })
        .cloned()
        .collect()
}

#[test]
fn no_claude_flag_is_eaten_by_the_shell() {
    let version = SNAPSHOT.lines().next().unwrap_or_default().to_string();
    let mut sources = vec![(version, flags_of(SNAPSHOT))];
    // 真 claude 只许跑 `--help`（题面红线）；没装就只看快照。
    if let Ok(out) = std::process::Command::new("claude").arg("--help").output() {
        if out.status.success() {
            let help = String::from_utf8_lossy(&out.stdout).into_owned();
            sources.push(("PATH 上的 claude".to_string(), flags_of(&help)));
        }
    }
    for (who, flags) in &sources {
        // 正控：V138 点名交给 claude 的那几个都抽得出来 ⇒ 抽取没空转。
        for want in [
            "--resume",
            "-r",
            "--continue",
            "--model",
            "-p",
            "--print",
            "--help",
            "--version",
        ] {
            assert!(flags.contains(want), "{who}：抽不出 {want}，抽取坏了");
        }
        let eaten = eaten_by_ccm(flags);
        assert!(
            eaten.is_empty(),
            "{who}：这些 claude 旗标被 ccm 当成自己的吃掉了：{eaten:?}\n\
             V138「壳层选项 ∩ claude 旗标 = ∅」不成立 —— claude 有了同名旗标，要主会话拍怎么让。"
        );
    }
}
