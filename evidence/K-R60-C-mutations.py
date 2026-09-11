#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R60 的变异台（死值验）。

被测对象：工作树 `.claude/worktrees/k-r60`（分支 `track/k-r60`）—— **写在这里，别靠目录名认**。
量法：每一刀
  ① 在**真文件**上按逐字锚点替换，并**断言锚点恰好命中 1 次**（「没切」与「没红」在输出上同形）；
  ② 印一行「变异已落地」并核变异后的串真的在盘上；
  ③ 在**沙箱**里跑 `cargo test --lib`（K31：开发测试一律进沙箱）；
  ④ 逐条记下哪几个判据红了；
  ⑤ `git checkout --` 复原，并核工作树回到干净。

⚠ 它会改工作树里的文件再复原 —— 跑之前工作树必须是干净的，脚本自己先核这一条。
"""
import subprocess
import sys
import re
from pathlib import Path

WT = Path("/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r60")
BOX = Path(
    "/tmp/claude-1000/-home-zbl----claudecode-frontend/"
    "f80a7aea-0ccf-4854-9b7a-12961a9c55aa/scratchpad/box.sh"
)

# (刀号, 说明, 文件, 锚点, 替换, 期望命中次数)
CUTS = [
    (
        "M1",
        "KR60D3：把 cc-bus 的 installable 翻回错的值（false）",
        "src-tauri/src/tool_registry.rs",
        "它左边读这个字段、右边钉 `cc_bus_deploy.rs` 的函数签名，两边必须相等。\n"
        "        installable: true,",
        "它左边读这个字段、右边钉 `cc_bus_deploy.rs` 的函数签名，两边必须相等。\n"
        "        installable: false,",
        1,
    ),
    (
        "M2",
        "KR60D2：把建表的人群退回 TOOLS（视图与清单又是两份）",
        "src-tauri/src/config_surface.rs",
        "    crate::tool_registry::environment()\n        .iter()",
        "    crate::tool_registry::environment()\n        .iter()\n        .filter(|e| "
        "matches!(e.backing, EnvBacking::Managed(_)))",
        1,
    ),
    (
        "M3",
        "KR60D1②：把第三档整档搬空（回到「不写进去就算第三档」那个盘面）",
        "src-tauri/src/tool_registry.rs",
        "pub const UNMANAGED_ENV: &[UnmanagedEnv] = &[",
        "pub const UNMANAGED_ENV: &[UnmanagedEnv] = &[];\n"
        "#[allow(dead_code)]\n"
        "const UNMANAGED_ENV_MUTANT: &[UnmanagedEnv] = &[",
        1,
    ),
    (
        "M4",
        "KR60D1③：把一项的档改成「随手填的」（why 里不给代码住址）",
        "src-tauri/src/tool_registry.rs",
        'why: "tmux 容器那条起法要它；缺了只在回绝里报一句能力名 —— \\\n'
        "              backend/control/ccm_invocation.rs::Refusal\",",
        'why: "常用工具，一般机器上都有",',
        1,
    ),
    (
        "M6",
        "a_hand_written_entry_is_never_app_installs：给一条手写项声明「app 装的」",
        "src-tauri/src/tool_registry.rs",
        '''        id: "pgrep",
        display_name: "pgrep",
        tier: EnvTier::AppAssumesPresent,''',
        '''        id: "pgrep",
        display_name: "pgrep",
        tier: EnvTier::AppInstalls,''',
        1,
    ),
    (
        "M7",
        "every_managed_tool_reaches_the_closed_set：让闭集漏掉 TOOLS 里的一条",
        "src-tauri/src/config_surface.rs",
        "    crate::tool_registry::environment()\n        .iter()",
        "    crate::tool_registry::environment()\n        .iter()\n        .filter(|e| e.id != \"ccm\")",
        1,
    ),
    (
        "M8",
        "the_environment_is_one_closed_list：让闭集里出现两个同名 id",
        "src-tauri/src/tool_registry.rs",
        '''        id: "pgrep",
        display_name: "pgrep",''',
        '''        id: "git",
        display_name: "pgrep",''',
        1,
    ),
    (
        "M5",
        "7u：把本件的实现整个退掉（人群退回 TOOLS + 第三档搬空 + 字段翻回错值）",
        None,  # 组合刀，见下面的 COMBO
        None,
        None,
        None,
    ),
]
COMBO = ["M1", "M2", "M3"]


def sh(cmd, cwd=None):
    return subprocess.run(
        cmd, shell=True, cwd=cwd, capture_output=True, text=True
    )


def tree_is_clean():
    r = sh("git status --porcelain", cwd=WT)
    return r.stdout.strip() == ""


def apply_cut(cid):
    """落一刀，返回它改过的文件集合。"""
    touched = set()
    ids = COMBO if cid == "M5" else [cid]
    for one in ids:
        for c in CUTS:
            if c[0] != one:
                continue
            _, _, rel, old, new, want = c
            p = WT / rel
            s = p.read_text(encoding="utf-8")
            n = s.count(old)
            assert n == want, (
                f"[{one}] 锚点命中 {n} 次，期望 {want} —— **没切**。"
                f"「没切」与「没红」在输出上同形，所以这里必须硬停。\n锚点：{old[:60]!r}"
            )
            s2 = s.replace(old, new)
            assert s2 != s
            p.write_text(s2, encoding="utf-8")
            after = (WT / rel).read_text(encoding="utf-8")
            assert after.count(new) >= 1, f"[{one}] 变异没落到盘上"
            print(f"  · [{one}] 变异已落地：{rel}（锚点命中 {n} 次，逐字核过）")
            touched.add(rel)
    return touched


def run_tests():
    r = sh(f'{BOX} \'cd src-tauri && cargo test --lib 2>&1\'')
    out = r.stdout + r.stderr
    failed = sorted(set(re.findall(r"^test (\S+) \.\.\. FAILED$", out, re.M)))
    m = re.search(r"^test result: (\w+)\. (\d+) passed; (\d+) failed", out, re.M)
    bar = m.group(0) if m else None
    compile_err = "error[E" in out or "could not compile" in out
    return failed, bar, compile_err, out


def main():
    assert tree_is_clean(), "工作树不干净 —— 先提交或清理，否则复原那一步会吃掉你的改动"
    head = sh("git rev-parse --short HEAD", cwd=WT).stdout.strip()
    print(f"被测对象：{WT}  HEAD={head}")

    print("\n===== 基线（不切刀）=====")
    failed, bar, cerr, _ = run_tests()
    print(f"  判定行：{bar}   编译错：{cerr}   红：{failed or '无'}")

    for cid, why, *_ in CUTS:
        print(f"\n===== {cid} · {why} =====")
        touched = apply_cut(cid)
        failed, bar, cerr, out = run_tests()
        print(f"  判定行：{bar}")
        print(f"  编译错：{cerr}")
        if failed:
            for f in failed:
                print(f"  红 → {f}")
        else:
            print("  红 → 无（⚠ 这一刀没买到任何东西）")
        # 老那条必需词守卫的现状（KR60D3 的第二半：它看不看得见字段值）
        old_guard = "tool_registry::tests::cc_bus_says_why_it_is_not_installable_at_the_real_depth"
        if f"test {old_guard} ... ok" in out:
            print(f"  绿 → {old_guard}（必需词守卫：它对这一刀是瞎的）")
        for rel in sorted(touched):
            sh(f"git checkout -- {rel}", cwd=WT)
        assert tree_is_clean(), f"{cid} 复原失败 —— 停"
        print("  复原：工作树已回到干净")


if __name__ == "__main__":
    sys.exit(main())
