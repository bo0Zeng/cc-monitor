#!/usr/bin/env python3
"""K-R48 **第二拍**的变异台：本拍新写 / 改写的每一条判据，逐条「先红后绿」。

# 它与第一拍那张台子的分工

`evidence/K-R48-ccm-native-mutations.py`（第一拍）切的是 `control/ccm/` 那套**原生实现**。
本台子切的是**第二拍自己新写或改写的判据**：删掉 `shared/ccm` 之后，有一批判据
「换了住址 / 换了语料 / 换了针」—— 那正是本仓反复栽过的「迁移把强度悄悄弄丢」的时刻，
所以每一条都要**切一刀验它真的会红**。

# 纪律（与第一拍逐字同一套）

- 每一刀**先断言锚点恰好命中 N 次**再改，并打印「变异已落地」＋命中数；对不上记 CRASH 不往下走。
- 判定**只看那一条判据的名字**，不看「总数掉了几个」——后者分不清「它红了」与「别处也红了」。
- **空刀也要写进表**（切了没红的）：那是一条读数，不是可以不提的事。

# 跑法（沙箱内，工作树根）

    python3 evidence/K-R48b-second-pass-mutations.py

⚠ 它会在切完之后把文件**原样还原**（先存字节、`finally` 写回）。中途被 kill 的话，
用 `git checkout -- <文件>` 收拾；每一刀之间也会自检「还原成功」。
"""
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]

# (刀号, 说明, 相对路径, 锚点, 期望命中数, 替换成什么, 该红的判据, 怎么跑)
#   怎么跑：("daemon", <cargo test 过滤串>) / ("monitor", <过滤串>) / ("vitest", <文件>)
CUTS = [
    (
        1,
        "容器路-中转地址那条转发整条拿掉",
        "remote-daemon-proto/src/control/ccm/plan.rs",
        'payload = format!("export ANTHROPIC_BASE_URL={}; {payload}", sq(v));',
        1,
        "let _ = v;",
        "the_container_path_forwards_every_inherited_variable_inward",
        ("daemon", "control::ccm::plan::tests::the_container_path_forwards"),
    ),
    (
        2,
        "容器路-身份 token 那条转发改成无条件加一个空 export",
        "remote-daemon-proto/src/control/ccm/plan.rs",
        'payload = format!("export CCM_LAUNCH_ID={}; {payload}", sq(v));',
        1,
        'payload = format!("export CCM_LAUNCH_ID=; {payload}"); let _ = v;',
        "the_container_path_forwards_every_inherited_variable_inward",
        ("daemon", "control::ccm::plan::tests::the_container_path_forwards"),
    ),
    (
        3,
        "cc-bus 查找次序-把 PATH 那一档删掉（真实部署里唯一还够得着的一档）",
        "remote-daemon-proto/src/control/ccm/plan.rs",
        "for dir in std::env::split_paths(&std::env::var_os(\"PATH\")?) {",
        1,
        "for dir in Vec::<std::path::PathBuf>::new() {",
        "asking_for_bus_registration_and_not_getting_it_is_never_silent",
        ("daemon", "control::ccm::plan::tests::asking_for_bus_registration"),
    ),
    (
        4,
        "cc-bus 可用性判定-`is_exec` 退回 `is_file()`（在≠跑得起来）",
        "remote-daemon-proto/src/control/ccm/plan.rs",
        "        return std::fs::metadata(p)\n            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)\n            .unwrap_or(false);",
        1,
        "        return p.is_file();",
        "asking_for_bus_registration_and_not_getting_it_is_never_silent",
        ("daemon", "control::ccm::plan::tests::asking_for_bus_registration"),
    ),
    (
        5,
        "登记不成那句诊断-闷声吃掉（要了、没做、也不说）",
        "remote-daemon-proto/src/control/ccm/plan.rs",
        '                        "ccm: --bus-register 要了登记，但找不到 cc-bus 的脚本（CC_BUS_SCRIPTS / <本程序目录>/cc-bus/scripts / PATH）——**没有登记**"',
        1,
        '                        "ccm: (闷声)"',
        "asking_for_bus_registration_and_not_getting_it_is_never_silent",
        ("daemon", "control::ccm::plan::tests::asking_for_bus_registration"),
    ),
    (
        6,
        "远端 ccm 入口-塞第二条可执行语句（就成了第二处实现）",
        "src-tauri/src/sftp.rs",
        '    format!(\n        "#!/bin/sh\\n# cc-monitor: ccm = 后端本体的一次性模式（K33：所有命令只许有一处）\\nexec {} ccm \\"$@\\"\\n",',
        1,
        '    format!(\n        "#!/bin/sh\\n# cc-monitor: ccm = 后端本体的一次性模式（K33：所有命令只许有一处）\\ncase \\"$1\\" in attach) shift ;; esac\\nexec {} ccm \\"$@\\"\\n",',
        "the_remote_ccm_entry_is_an_entry_not_an_implementation",
        ("monitor", "sftp::tests::the_remote_ccm_entry_is_an_entry"),
    ),
    (
        7,
        "远端 ccm 入口-路径不经 quote（带空格的 daemon_path 会被拆词）",
        "src-tauri/src/sftp.rs",
        "        shell_quote_core::posix_quote(daemon_path)",
        1,
        "        daemon_path",
        "the_remote_ccm_entry_is_an_entry_not_an_implementation",
        ("monitor", "sftp::tests::the_remote_ccm_entry_is_an_entry"),
    ),
    (
        8,
        "cc-spawn 找后端-只 `-x` 不 `-f`（同名目录会劫持解析）",
        "shared/cc-bus/scripts/cc-spawn",
        '    if [ -n "$_x" ] && [ -f "$_x" ] && [ -x "$_x" ]; then CCM_BIN="$_x"; break; fi',
        1,
        '    if [ -n "$_x" ] && [ -x "$_x" ]; then CCM_BIN="$_x"; break; fi',
        "cc_spawn_resolves_a_real_ccm_file_not_a_directory",
        ("monitor", "ccm_cli_contract::tests::cc_spawn_resolves_a_real_ccm_file"),
    ),
    (
        9,
        "cc-spawn 找后端-把已经不存在的旧脚本路径加回查找次序",
        "shared/cc-bus/scripts/cc-spawn",
        '  for _x in "$HOME/.cc-monitor/bin/cc-monitor-remote" \\',
        1,
        '  for _x in "$SELFDIR/../../ccm" "$HOME/.cc-monitor/bin/cc-monitor-remote" \\',
        "cc_spawn_resolves_a_real_ccm_file_not_a_directory",
        ("monitor", "ccm_cli_contract::tests::cc_spawn_resolves_a_real_ccm_file"),
    ),
    (
        10,
        "常量抠法-把段界去掉（两张表会被抠成一份）",
        "src-tauri/src/plugin_class_registry.rs",
        '        let end = tail.find("];").expect("那个常量没有收尾 `];`");',
        1,
        "        let end = tail.len();",
        "the_const_list_extractor_takes_one_list_not_the_whole_file",
        ("monitor", "plugin_class_registry::tests::the_const_list_extractor"),
    ),
    # 🔴 **#11 是一刀空刀，保留在表里是因为它本身是一条读数**（第一趟切完就是这个结果）：
    #    它把右半那条**形状校验**改成同义反复（`tail == tail`）。真判据与活体夹具**都仍绿** ——
    #    因为那条形状校验的岗位不是「抠得对不对」，是「**万一那条转发写坏了要出声**」，
    #    而把守卫掏空本身不会让差集变化。⇒ **那条形状校验今天没有判据看着它**，如实登记。
    #    #16 是它的收窄版：不掏守卫，改去把**被守的那条转发**写坏。
    (
        11,
        "容器路转发面对账-右半的形状校验放水（写成 `$(别的变量)` 也放行）",
        "src-tauri/src/backend/control/payload.rs",
        '                tail, "{}; {payload}\\", sq(v));",',
        1,
        '                tail, tail,',
        "the_outside_export_gate_really_reddens_on_a_live_breach",
        ("monitor", "backend::control::payload::tests::the_outside_export_gate"),
    ),
    (
        12,
        "`--base` 跨语言契约-Rust 侧那条 `unset` 渲染拿掉",
        "remote-daemon-proto/src/control/ccm/plan.rs",
        '        line.push_str(&format!("unset {cfg_env}; "));',
        1,
        '        let _ = cfg_env;',
        "base-flag-contract-guard",
        ("vitest", "src/base-flag-contract-guard.vitest.ts"),
    ),
    (
        13,
        "`--base` 语义-`Plan.unset_config_dir` 恒假（文案说「会清掉」而它不清）",
        "remote-daemon-proto/src/control/ccm/plan.rs",
        "        unset_config_dir: o.use_base,",
        1,
        "        unset_config_dir: false,",
        "account-base-semantics",
        ("vitest", "src/account-base-semantics.vitest.ts"),
    ),
    (
        14,
        "per-agent 表三写点-把 `default_launcher` 的 codex 臂改成别的值",
        "remote-daemon-proto/src/control/ccm/mod.rs",
        '        "codex" => "codex",',
        1,
        '        "codex" => "codex-x",',
        "agent-profile-parity",
        ("vitest", "src/agent-profile-parity.vitest.ts"),
    ),
    (
        15,
        "身份 poller 守卫-往渲出去的 shell 里塞一条与会话同寿的循环",
        "remote-daemon-proto/src/control/ccm/plan.rs",
        '    if !c.detach {\n        seq.push_str(&format!("; tmux attach -t {t}"));\n    }',
        1,
        '    if !c.detach {\n        seq.push_str(&format!("; while kill -0 $$ 2>/dev/null; do sleep 1; done; tmux attach -t {t}"));\n    }',
        "the_identity_poller_is_gone_for_good",
        ("monitor", "polling_registry::tests::the_identity_poller_is_gone_for_good"),
    ),
    (
        16,
        "容器路转发-中转地址那条**不经 `sq`**（带空格/引号的值会把载荷拆断）",
        "remote-daemon-proto/src/control/ccm/plan.rs",
        'payload = format!("export ANTHROPIC_BASE_URL={}; {payload}", sq(v));',
        1,
        'payload = format!("export ANTHROPIC_BASE_URL={}; {payload}", v);',
        "every_variable_exported_outside_ccm_is_forwarded_by_the_container_path",
        ("monitor", "backend::control::payload::tests::every_variable_exported_outside_ccm"),
    ),
]


def run(kind, filt):
    """跑一趟，回 (那条判据红没红, 原始输出)。"""
    if kind == "daemon":
        cmd = ["cargo", "test", filt]
        cwd = ROOT / "remote-daemon-proto"
    elif kind == "monitor":
        cmd = ["cargo", "test", "--lib", "-p", "monitor", filt]
        cwd = ROOT / "src-tauri"
    else:
        cmd = ["npx", "vitest", "run", filt]
        cwd = ROOT
    p = subprocess.run(cmd, cwd=str(cwd), capture_output=True, text=True)
    out = p.stdout + p.stderr
    return p.returncode != 0, out


def main():
    rows = []
    for no, what, rel, anchor, want, repl, judge, how in CUTS:
        f = ROOT / rel
        raw = f.read_bytes()
        src = raw.decode()
        hits = src.count(anchor)
        if hits != want:
            print(f"CRASH | 刀 #{no} 锚点命中 {hits} 次（该 {want}）：{rel}\n  锚点：{anchor!r}", file=sys.stderr)
            return 2
        try:
            f.write_text(src.replace(anchor, repl, 1))
            after = f.read_text()
            assert repl in after and after != src, "变异没落地"
            print(f"变异已落地 | 刀 #{no} | 锚点命中 {hits}/{want} | {rel}")
            red, out = run(*how)
            named = judge in out
            rows.append((no, what, judge, red, named))
            print(f"  ⇒ {'红' if red else '★仍绿（空刀）'}｜诊断里点名那条判据：{'是' if named else '否'}")
        finally:
            f.write_bytes(raw)
            assert f.read_bytes() == raw, f"还原失败：{rel}"
    print("\n===== 逐行表（先红后绿的「先红」那一半）=====")
    print("#\t刀\t该红的判据\t红了吗\t诊断点名")
    for no, what, judge, red, named in rows:
        print(f"{no}\t{what}\t{judge}\t{'红' if red else '★仍绿'}\t{'是' if named else '否'}")
    reds = sum(1 for r in rows if r[3])
    print(f"\n===== 合计 {len(rows)} 刀：红 {reds} · 空刀 {len(rows) - reds} =====")
    return 0


if __name__ == "__main__":
    sys.exit(main())
