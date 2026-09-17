#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R61 变异台的**驱动** —— 施一刀 → 在沙箱里跑整包（拿最小面）→ 撤刀 → 记一行。

住址：`evidence/K-R61-run-cuts.py`（本文件）
量具：`evidence/K-R61-mutations.py`（刀本身）· `evidence/K-R61-7u-hollow-out.py`（`7u` 那一刀）
被测对象：由 `--worktree` 给（本轮量的是 `.claude/worktrees/k-r61`，尖见变异表头）
跑法：`python3 evidence/K-R61-run-cuts.py --worktree <绝对路径> M1·… M6·… …`

🔴 **为什么它是 `.py` 而不是 `.sh`**：第一版写成 bash 落进 `evidence/`，
`shell_lint_registry::every_shell_script_is_either_linted_or_registered_as_exempt`
当场红（既不在 CI 的 shellcheck 表达式里、也没登记豁免）。那条判据是对的 —— 改语言，
不改判据。〔`K-R61` 09-11 现打〕

⚠ **第一版还有一个真栽过的坑，登记在这里**：`case` 里 `M1*` 写在 `M10*` 前面，
shell glob 于是把 `M10` / `M11` 吞进了 monitor 那一支 —— 跑错了套、撤错了文件，
`M11` 那一行是带着没撤干净的 `M10` 一起跑的。**兜住它的是「撤刀后 git status 必须干净」**
那句自检。本版按前缀精确匹配，并保留那句自检。
"""
import argparse
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
PROJ = "/home/zbl/文档/claudecode-frontend"
IMAGE = "ccmon-devbox:latest"

# 刀名前缀 → （跑哪个套, 撤哪个文件）。**精确前缀**，不用 glob。
ROUTES = [
    ("M10", "daemon", "remote-daemon-proto/src/control/ccm/mod.rs"),
    ("M11", "daemon", "remote-daemon-proto/src/control/ccm/plan.rs"),
    ("M13", "daemon", "remote-daemon-proto/src/control/ccm/plan.rs"),
    ("M1", "monitor", "src-tauri/src/history.rs"),
    ("M2", "monitor", "src-tauri/src/history.rs"),
    ("M3", "monitor", "src-tauri/src/history.rs"),
    ("M4", "monitor", "src-tauri/src/history.rs"),
    ("M5", "monitor", "src-tauri/src/history.rs"),
    ("M6", "monitor", "src-tauri/src/history.rs"),
    ("M7", "monitor", "src-tauri/src/history.rs"),
    ("M8", "monitor", "src-tauri/src/history.rs"),
]

SUITES = {
    "monitor": "cd src-tauri && cargo test --lib 2>&1",
    "daemon": "cd remote-daemon-proto && cargo test --bin cc-monitor-remote 2>&1",
    "e2e": "cd remote-daemon-proto && cargo build --bin cc-monitor-remote 2>&1 | tail -1; "
           "cd .. ; bash e2e/ccm-contract-parity.sh 2>&1",
}


def route(cut):
    for prefix, suite, path in ROUTES:
        # 精确前缀 + 分隔符，`M1` 不许吃掉 `M10`
        if cut == prefix or cut.startswith(prefix + "·"):
            return suite, path
    sys.exit(f"🔴 不认得这把刀：{cut}")


def box(wt, cmd):
    """与 `.claude/devbox/gate` 同镜像、同 target 目录、同断网。"""
    return subprocess.run(
        ["docker", "run", "--rm", "--network", "none",
         "-v", f"{PROJ}:{PROJ}",
         "-v", "ccmon-cargo-registry:/opt/rust/cargo/registry",
         "-e", f"CARGO_TARGET_DIR={PROJ}/.claude/pm-targets/k-r61",
         "-e", "HOME=/home/zbl", "-w", wt, IMAGE,
         "bash", "-o", "pipefail", "-c",
         'mkdir -p "$HOME/.claude/projects"; ' + cmd],
        capture_output=True, text=True).stdout


def git(wt, *a):
    return subprocess.run(["git", "-C", wt, *a], capture_output=True, text=True).stdout


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--worktree", required=True)
    ap.add_argument("--suite", default=None, help="覆盖默认套（monitor/daemon/e2e）")
    ap.add_argument("cuts", nargs="+")
    a = ap.parse_args()
    wt = os.path.abspath(a.worktree)

    for cut in a.cuts:
        suite, path = route(cut)
        if a.suite:
            suite = a.suite
        print(f"════════ {cut}（套 {suite}）════════", flush=True)
        r = subprocess.run([sys.executable, os.path.join(HERE, "K-R61-mutations.py"), cut],
                           capture_output=True, text=True)
        print(r.stdout.strip(), flush=True)
        if r.returncode != 0:
            sys.exit(r.stdout + r.stderr)

        out = box(wt, SUITES[suite])
        verdicts = re.findall(r"^test result:.*$", out, re.M) or \
            re.findall(r"^===== 合计 PASS=.*$", out, re.M)
        print("--- 判定行 ---")
        print("\n".join(verdicts) if verdicts
              else "🔴 判定行不见了 ⇒ 按 CRASH 记，不许报「新红 0」")
        print("--- 红了哪几条（= 最小面）---")
        block = re.search(r"^failures:$(.*?)^test result:", out, re.M | re.S)
        names = sorted(set(re.findall(r"^    ([a-z][\w:]+)$", block.group(1), re.M))) if block else []
        print("\n".join(names) if names else "（一条都没红）")

        git(wt, "checkout", "--", path)
        st = git(wt, "status", "--porcelain").strip()
        if st:
            sys.exit(f"🔴 撤刀没干净，后面每一行读数都不可信：\n{st}")
        print("撤刀后 git status 干净 ✓", flush=True)


if __name__ == "__main__":
    main()
