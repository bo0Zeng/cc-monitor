#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R52 事3 的变异台：**新判据到底有没有牙** —— 一刀一句声称，空刀也进表。

被测对象（写死）：
    <本文件所在工作树>/remote-daemon-proto        （相对本文件：../remote-daemon-proto）
判据本体：`src/platform/cfgless_guard.rs`（`K-R52` 09-11 新立）

# 纪律（本仓真栽过才立的，逐条落在代码里）

- **每一刀先断言锚点恰好命中 N 次再改，并打印「变异已落地」**；命中数不对 ⇒ 记成**没切**，
  不许报成「新红 0」。
- **每一行都记锚点** —— 切在哪个文件、哪一处、命中几次。不记，下一轮谁也复不出这一刀。
- **跑完先核判定行**：`test result:` 那一行的 `passed + failed` 总数。总数掉了 / 台子异常退出
  ⇒ 按 **CRASH** 记，不许读成「新红 0」。
- **空刀也要写进表**：一刀打不红是读数，不是失败 —— 它告诉你那一格盖不到哪儿。
- 🔴 **原地切、切完必还原**：每一刀之后比文件 md5，对不上当场停。

# 跑在哪儿

`cargo test` 一律进沙箱（`K31`：开发测试不许直接在本机跑）。本台子从宿主发 `docker run`，
镜像与挂载抄 `.claude/devbox/gate` 那一份。**它不是门禁** —— 门禁是
`PB_WS=… .claude/devbox/gate <工作树> <tag>`，两者别混。

用法：
    python3 K-R52-D3-mutation-table.py
"""

from __future__ import annotations

import hashlib
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
WT = os.path.abspath(os.path.join(HERE, ".."))
CRATE = os.path.join(WT, "remote-daemon-proto")
PROJ = "/home/zbl/文档/claudecode-frontend"


def run_suite() -> tuple[int, int, list[str], str]:
    """跑一趟 daemon 全量 `cargo test`。返回 (passed, failed, 红掉的测试名, 原始尾巴)。"""
    cmd = [
        "docker", "run", "--rm", "--network", "none",
        "-v", f"{PROJ}:{PROJ}",
        "-v", "ccmon-cargo-registry:/opt/rust/cargo/registry",
        "-e", f"CARGO_TARGET_DIR={PROJ}/.claude/pm-targets/k-r52-mut",
        "-e", "HOME=/home/zbl",
        "-w", CRATE,
        "ccmon-devbox:latest",
        "bash", "-o", "pipefail", "-c", "cargo test 2>&1",
    ]
    out = subprocess.run(cmd, capture_output=True, text=True, timeout=1800)
    text = out.stdout + out.stderr
    m = re.search(r"test result: \w+\. (\d+) passed; (\d+) failed", text)
    reds = re.findall(r"^\s{4}(\S+)$", text[text.find("\nfailures:\n") :], re.M) if "\nfailures:\n" in text else []
    reds = sorted({r for r in reds if "::" in r})
    if not m:
        return (-1, -1, reds, text[-1500:])
    return (int(m.group(1)), int(m.group(2)), reds, text[-400:])


def md5(p: str) -> str:
    return hashlib.md5(open(p, "rb").read()).hexdigest()


# (刀名, 切的是**哪一句声称**, 相对 crate 的文件, 锚点, 期望锚点命中数, 替换成)
CUTS: list[tuple[str, str, str, str, int, str]] = [
    (
        "M1 摘掉 acquire 的门",
        "A1（换个 target 就编不过的那一档）一处都不许在门外",
        "src/sidecars/codepicture/acquire.rs",
        "    #[cfg(unix)]\n    {\n        use std::os::unix::fs::OpenOptionsExt;",
        1,
        "    {\n        use std::os::unix::fs::OpenOptionsExt;",
    ),
    (
        "M2 把 discover 换回裸 true",
        "B 族补人群：`platform/` 之外的回退臂不许编乐观答案",
        "src/plugin/discover.rs",
        "    #[cfg(not(unix))]\n    {\n        false\n    }\n}",
        1,
        "    #[cfg(not(unix))]\n    {\n        true\n    }\n}",
    ),
    (
        "M3 新长一处没签字的 A2",
        "A2 门外的每一处都要在 REGISTERED 里签过字（「有命中而没登记」那半）",
        "src/plugin/discover.rs",
        "use std::path::{Path, PathBuf};",
        1,
        'use std::path::{Path, PathBuf};\n\npub(crate) const MUTANT: &str = "/proc/self/exe";',
    ),
    (
        "M4 往登记表塞一条过期条目",
        "登记表不许烂掉（「有登记而匹配不上」那半）",
        "src/platform/cfgless_guard.rs",
        '    const REGISTERED: &[(&str, &str, &str, &str)] = &[\n',
        1,
        '    const REGISTERED: &[(&str, &str, &str, &str)] = &[\n'
        '        (\n            "no/such/file.rs",\n            "no-such-anchor",\n'
        '            "真漏",\n            "这是变异台塞进来的一条过期条目，用来验「表不许烂掉」那一半有没有牙",\n        ),\n',
    ),
    (
        "M5【空刀】注释里写一行平台代码",
        "剥注释那一格：散文不许被数成代码",
        "src/platform/paths.rs",
        "//!",
        -1,  # -1 = 只要求「至少 1 次」，改第一处
        None,  # 特判：在第一处 `//!` 那一行后面插一行注释
    ),
    (
        "M6 摘掉文件级那道门",
        "文件级门（`#[cfg(平台)] mod x;` 整份选进来）这一格认得出来",
        "src/platform/pidwatch/mod.rs",
        '#[cfg(target_os = "linux")]\nmod linux;',
        1,
        "mod linux;",
    ),
]

WATCH = [
    "platform::cfgless_guard::tests::platform_primitives_that_break_the_build_must_all_sit_behind_a_gate",
    "platform::cfgless_guard::tests::platform_assumptions_outside_a_gate_are_each_signed_for",
    "platform::cfgless_guard::tests::fallback_arms_outside_the_adapter_layer_must_not_fabricate_success",
    "platform::cfgless_guard::tests::the_population_is_not_silently_empty",
    "platform::cfgless_guard::tests::the_detector_bites_on_synthetic_defects_and_not_on_prose",
]


def apply_cut(path: str, anchor: str, want: int, repl: str | None) -> tuple[bool, str, int]:
    src = open(path, encoding="utf-8").read()
    n = src.count(anchor)
    if want >= 0 and n != want:
        return (False, src, n)
    if want < 0 and n < 1:
        return (False, src, n)
    if repl is None:
        # M5 特判：在**第一处** `//!` 之后插一行注释，逐字写着一句平台代码。
        i = src.find(anchor)
        j = src.find("\n", i) + 1
        new = src[:j] + "//! 曾经这里写着 let uid = unsafe { libc::getuid() };\n" + src[j:]
    else:
        new = src.replace(anchor, repl, 1)
    open(path, "w", encoding="utf-8").write(new)
    return (True, src, n)


def main() -> int:
    print("== K-R52 事3 变异台 ==")
    print(f"被测 crate：{CRATE}")
    sha = subprocess.run(
        ["git", "-C", WT, "rev-parse", "HEAD"], capture_output=True, text=True
    ).stdout.strip()
    dirty = subprocess.run(
        ["git", "-C", WT, "status", "--short"], capture_output=True, text=True
    ).stdout.strip()
    print(f"基线提交：{sha}（工作树{'**有**未提交改动 —— 变异跑的是工作树那一份' if dirty else '干净'}）")
    print()

    print("— 入场（未变异）—")
    p0, f0, r0, tail0 = run_suite()
    print(f"  判定行：{p0} passed · {f0} failed   ⇒ 基线总数 = {p0 + f0}")
    if f0 != 0:
        print(f"  🔴 入场就不是全绿（红 {f0} 条：{r0}）—— 后面每一行都要带着这个前提读")
    base_total = p0 + f0
    print()

    rows = []
    for name, claim, relp, anchor, want, repl in CUTS:
        path = os.path.join(CRATE, relp)
        before = md5(path)
        ok, orig, n = apply_cut(path, anchor, want, repl)
        if not ok:
            rows.append((name, claim, relp, n, "没切", "—", "锚点命中数不对 ⇒ **这一刀没切下去**，不许读成「新红 0」"))
            print(f"  [{name}] 🔴 锚点命中 {n} 次（期望 {want}）⇒ 没切")
            continue
        print(f"  [{name}] 变异已落地：{relp}，锚点命中 {n} 次")
        try:
            p, f, reds, tail = run_suite()
        finally:
            open(path, "w", encoding="utf-8").write(orig)
            assert md5(path) == before, f"{relp} 还原失败 —— 当场停"
        total = p + f
        if p < 0 or total < base_total:
            verdict = "CRASH"
            detail = f"判定行掉了或台子异常退出（总数 {total} vs 基线 {base_total}）：{tail}"
        else:
            new_reds = [r for r in reds if r not in r0]
            hit = [r for r in new_reds if r in WATCH]
            other = [r for r in new_reds if r not in WATCH]
            verdict = f"新红 {len(new_reds)}"
            detail = "本判据红：" + (", ".join(x.rsplit("::", 1)[-1] for x in hit) or "**无**")
            if other:
                detail += " ｜ 别的判据也红：" + ", ".join(x.rsplit("::", 1)[-1] for x in other)
        rows.append((name, claim, relp, n, verdict, f"{p}/{f}", detail))
        print(f"        ⇒ {verdict}（{p} passed / {f} failed）：{detail}")

    print()
    print("— 变异表 —")
    print(f"{'刀':<28}{'锚点命中':<10}{'判定行':<12}{'读数':<10}明细")
    for name, claim, relp, n, verdict, pf, detail in rows:
        print(f"{name:<28}{n:<10}{pf:<12}{verdict:<10}{detail}")
        print(f"{'':<28}切的那句声称：{claim}")
        print(f"{'':<28}切在：{relp}")
    print()
    print("⚠ 空刀（M5）打不红是**正确读数**，不是失败：它证的是「注释里的散文不会被数成代码」。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
