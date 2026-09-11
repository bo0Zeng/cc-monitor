#!/usr/bin/env python3
"""K-R48 变异台：把 `control/ccm/` 那几条新判据逐个打红，证明它们真的咬得动。

跑法（沙箱内，工作树根）：
    python3 evidence/K-R48-ccm-native-mutations.py

每一刀：① 断言锚点在生产段里**恰好命中 N 次**（命中数不对就 CRASH，不许继续）；
② 落刀并打印「变异已落地」；③ 只跑它该打红的那条判据；④ 无论结果如何都还原。

⚠ 锚点一律记在表里（切在哪个函数 / 命中几次）—— 不记的话下一轮谁也复不出这一刀。
"""
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
CRATE = ROOT / "remote-daemon-proto"

# (刀名, 文件, 锚点, 锚点该命中几次, 换成什么, 该打红的判据, 切在哪)
CUTS = [
    (
        "D4-第二处默认值",
        "src/control/ccm/argv.rs",
        "agent: Defaults::AGENT.to_string(),",
        1,
        'agent: "codex".to_string(),',
        "control::ccm::argv::tests::every_default_lives_only_in_the_defaults_block",
        "parse() 的初值块（KR48D4 的靶子：在别处再写一份默认）",
    ),
    (
        "D2-第二处旗标字面量",
        "src/control/ccm/plan.rs",
        "pub(crate) fn qarg(s: &str) -> String {",
        1,
        'pub(crate) fn qarg(s: &str) -> String {\n    let _second_home = "--tmux";',
        "control::ccm::argv::tests::the_ccm_argv_is_parsed_in_exactly_one_place",
        "plan.rs::qarg 头（KR48D2 的靶子：旗标名长出第二处住址）",
    ),
    (
        "用法行-掏掉一个开关",
        "src/control/ccm/mod.rs",
        "  --detach           建完就返回，不接进去（只在容器路有意义）\n",
        1,
        "",
        "control::ccm::tests::every_flag_we_accept_has_a_usage_line",
        "USAGE 常量（认一个开关却不说 = 隐藏开关）",
    ),
    (
        "起会话命令-不清嵌套标记",
        "src/control/ccm/plan.rs",
        '    if !d.nested.is_empty() {\n        line.push_str(&format!("unset {}; ", d.nested.join(" ")));\n    }\n',
        1,
        "",
        "control::ccm::plan::tests::the_shape_of_one_launch_command_line",
        "render_direct 的段序（嵌套标记那一段整段掉了）",
    ),
    (
        "起会话命令-账号排到模型后面",
        "src/control/ccm/plan.rs",
        '    let cfg_env = &d.account_env;\n    if !d.config_dir.is_empty() {\n        line.push_str(&format!("export {cfg_env}={}; ", sq(&d.config_dir)));\n    }\n    if d.unset_config_dir {\n        line.push_str(&format!("unset {cfg_env}; "));\n    }\n    if !d.model.is_empty() {\n        line.push_str(&format!("export ANTHROPIC_MODEL={}; ", sq(&d.model)));\n    }\n',
        1,
        '    let cfg_env = &d.account_env;\n    if !d.model.is_empty() {\n        line.push_str(&format!("export ANTHROPIC_MODEL={}; ", sq(&d.model)));\n    }\n    if !d.config_dir.is_empty() {\n        line.push_str(&format!("export {cfg_env}={}; ", sq(&d.config_dir)));\n    }\n    if d.unset_config_dir {\n        line.push_str(&format!("unset {cfg_env}; "));\n    }\n',
        "control::ccm::plan::tests::the_account_dir_comes_before_the_model",
        "render_direct 的两段互换（顺序即契约）",
    ),
    (
        "容器路-继承的账号不显式化",
        "src/control/ccm/plan.rs",
        'payload = format!("export {}={}; {payload}", env.account_env, sq(v));',
        1,
        "let _ = v;",
        "control::ccm::plan::tests::the_container_path_carries_every_intent_inward",
        "build() 的容器分支（R08 原病：靠继承穿 tmux 边界）",
    ),
    (
        "容器路-写事实标记而不是意图标记",
        "src/control/ccm/plan.rs",
        "@ccm_sid_expect {} 2>/dev/null || true)",
        1,
        "@ccm_sid {} 2>/dev/null || true)",
        "control::ccm::plan::tests::the_container_path_carries_every_intent_inward",
        "render_container（F04：通道A 只许写意图标）",
    ),
    (
        "attach-目标退回裸名字",
        "src/control/ccm/plan.rs",
        'Plan::Attach { name } => format!("tmux attach -t {}", sq(&format!("={name}:"))),',
        1,
        'Plan::Attach { name } => format!("tmux attach -t {}", sq(name)),',
        "control::ccm::plan::tests::attach_uses_the_exact_match_target",
        "render()（F01：裸目标会接错兄弟会话）",
    ),
    (
        "会话名派生-不折叠连续横杠",
        "src/control/ccm/plan.rs",
        "        if c == '-' {\n            if last_dash {\n                continue;\n            }\n",
        1,
        "        if c == '-' {\n            if last_dash && false {\n                continue;\n            }\n",
        "control::ccm::plan::tests::the_session_name_derivation_rule",
        "derive_tmux_name（跨语言双写点的本侧规则）",
    ),
    (
        "会话名校验-放过 tmux 目标语法",
        "src/control/ccm/plan.rs",
        'if n.chars().any(|c| "*?.:=".contains(c)) {',
        1,
        "if false {",
        "control::ccm::plan::tests::a_session_name_that_would_confuse_tmux_is_refused",
        "validate_tmux_name（会话名是一条注入面）",
    ),
    (
        "窗口标题-退回 #T",
        "src/control/ccm/mod.rs",
        'pub(crate) const RBIND_TITLE_FORMAT: &str = "#{?@ccm_sid,ccm-rbind-#{@ccm_sid},#T}";',
        1,
        'pub(crate) const RBIND_TITLE_FORMAT: &str = "#T";',
        "control::ccm::tests::the_window_title_is_synthesised_from_the_identity_tag_not_the_pane_title",
        "RBIND_TITLE_FORMAT（marker 被 claude 的状态标题冲掉的那条真机病）",
    ),
    (
        "probe-首行不是 name=ccm",
        "src/control/ccm/mod.rs",
        '"name=ccm\\nversion={CCM_VERSION}\\nself={self_path}\\ncapabilities={}\\nagents={}\\n",',
        1,
        '"name=cc-monitor\\nversion={CCM_VERSION}\\nself={self_path}\\ncapabilities={}\\nagents={}\\n",',
        "control::ccm::tests::the_probe_output_is_the_shape_its_parser_expects",
        "probe_output（ccm_probe.rs 的判活依据）",
    ),
    (
        "账号-显式选号解析不出来时悄悄降级",
        "src/control/ccm/plan.rs",
        '            None => Err(Die(format!(\n                "账号 \'{}\' 不可用（不在 {}，或其目录不存在）。可用: {}",',
        1,
        '            None => Ok((String::new(), String::new())),\n            #[allow(unreachable_patterns)]\n            None => Err(Die(format!(\n                "账号 \'{}\' 不可用（不在 {}，或其目录不存在）。可用: {}",',
        "control::ccm::plan::tests::picking_an_account_never_falls_back_to_a_different_one",
        "resolve_account 的显式分支（显式选号绝不静默降级）",
    ),
    (
        "账号-覆盖调用方已选好的号",
        "src/control/ccm/plan.rs",
        "    if env\n        .inherited_config_dir\n        .as_deref()\n        .is_some_and(|v| !v.is_empty())\n    {",
        1,
        "    if false {",
        "control::ccm::plan::tests::the_four_ways_an_account_gets_picked",
        "resolve_account 的继承闸（R08：真机复现过的静默换号）",
    ),
    (
        "argv-resume 把下一个旗标当 sid",
        "src/control/ccm/argv.rs",
        'Some(v) if !v.starts_with(\'-\') => o.sid = v.clone(),',
        1,
        "Some(v) => o.sid = v.clone(),",
        "control::ccm::argv::tests::a_positional_action_never_swallows_the_next_flag_as_its_value",
        "parse() 的 resume 位置分支",
    ),
    (
        "argv-`--` 之后的东西被本层认走",
        "src/control/ccm/argv.rs",
        "            flag::END => {\n                o.passthru.extend_from_slice(&args[i + 1..]);\n                break;\n            }\n",
        1,
        "",
        "control::ccm::argv::tests::everything_after_the_terminator_goes_to_the_agent_untouched",
        "parse() 的 `--` 臂（透传参数被当成本层旗标）",
    ),
    (
        "argv-尺寸校验放水",
        "src/control/ccm/argv.rs",
        "    if !w.bytes().all(|c| c.is_ascii_digit()) || !h.bytes().all(|c| c.is_ascii_digit()) {\n        return None;\n    }\n",
        1,
        "",
        "control::ccm::argv::tests::the_size_is_two_plain_decimals_or_it_is_refused",
        "parse_size（尺寸会被拼进 tmux argv，是一条注入面）",
    ),
    (
        "取名-基名撞了不退让",
        "src/control/ccm/plan.rs",
        "(o.tmux_base.clone(), true)",
        1,
        "(o.tmux_base.clone(), false)",
        "control::ccm::plan::tests::only_two_of_the_three_naming_paths_step_aside_on_a_collision",
        "build() 的取名三分支（--tmux-base 静默退化成 --tmux=<名>）",
    ),
    (
        "取名-退让规则从 3 起跳",
        "src/control/ccm/plan.rs",
        "    let mut k = 2usize;",
        1,
        "    let mut k = 3usize;",
        "control::ccm::plan::tests::only_two_of_the_three_naming_paths_step_aside_on_a_collision",
        "next_free_name 的起跳值",
    ),
    (
        "入口-把 ccmonitor 也当成 ccm",
        "src/control/ccm/mod.rs",
        "    if base == SUBCOMMAND_WORD {",
        1,
        "    if base.starts_with(SUBCOMMAND_WORD) {",
        "control::ccm::tests::there_are_exactly_two_ways_in",
        "intercept()（子串不算 —— `com.ccmonitor.app` 那次假读数的同形）",
    ),
]


def run(cut):
    name, rel, anchor, want, repl, test, where = cut
    path = CRATE / rel
    orig = path.read_text()
    hits = orig.count(anchor)
    if hits != want:
        return (name, where, hits, want, "CRASH：锚点命中数不对，这一刀没切成")
    path.write_text(orig.replace(anchor, repl, 1))
    print(f"  变异已落地：{name}（{rel} · 锚点命中 {hits} 次）", flush=True)
    try:
        p = subprocess.run(
            ["cargo", "test", "--", "--exact", test],
            cwd=CRATE, capture_output=True, text=True,
        )
        out = p.stdout + p.stderr
        if "error[E" in out or "error: could not compile" in out:
            verdict = "CRASH：台子炸了（编译不过），不是读数"
        elif " 1 passed" in out and "0 failed" in out:
            verdict = "🔴 仍绿 —— 这条判据在这一刀上没有牙"
        elif "1 failed" in out:
            verdict = "红（判据咬住了）"
        else:
            verdict = f"判不了：既没绿也没红，原文见下\n{out[-400:]}"
        return (name, where, hits, want, verdict)
    finally:
        path.write_text(orig)


def main():
    rows = [run(c) for c in CUTS]
    print("\n| # | 刀 | 切在哪 · 锚点命中 | 结果 |")
    print("|---|---|---|---|")
    for i, (name, where, hits, want, verdict) in enumerate(rows, 1):
        print(f"| {i} | {name} | {where} · {hits}/{want} | {verdict} |")
    bad = [r for r in rows if not r[4].startswith("红")]
    print(f"\n合计 {len(rows)} 刀 · 红 {len(rows) - len(bad)} · 非红 {len(bad)}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
