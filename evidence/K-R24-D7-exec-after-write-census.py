#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R24 下一拍㈡ · 人群：**「写出一个可执行文件，然后（同一个进程里）exec 它」**的判据有几条。

── 为什么切成这个形状 ─────────────────────────────────────────────
  `launch.rs` 那条偶发红的成因是 `ETXTBSY`，而 `ETXTBSY` 的**必要条件**是
  「execve 的目标此刻被某个进程打开着写」。⇒ 会撞上它的判据必须同时满足两件事：
    ① 那个可执行文件是**这一趟自己写出来的**（否则没人在写它）；
    ② 它随后**真的被 exec**（只 `stat` / 只查 `PATH` 的**不算** —— 那条路不经过 execve）。
  🔴 **刻意不切成「写了文件 + chmod 了执行位」** —— 那是候选面，不是人群：
     本仓实测**有 4 处满足它而不满足 ②**（`is_executable` 那一族只读 metadata）。
     把它当人群会多报 4 条，并把修法引到「别 chmod」而不是「把前提建起来」。

── 分母怎么数的（两级，别混着报）───────────────────────────────────
  · **候选面** = 三个 Rust 根下所有 `.rs` 文件里「给一个路径加执行位」的处数
    （`from_mode(0o?7?/0o?5?)` 或 `set_mode(0o…)` 带执行位）—— 机器数得出，本脚本打这个数。
  · **人群** = 候选面里 ② 也成立的那几处 —— **逐处读代码判的**，理由写在
    `evidence/K-R24-D7-etxtbsy.md` 那张表里。⚠ 这一半**不是机器判的**，别把它读成机检读数。

用法：
  python3 evidence/K-R24-D7-exec-after-write-census.py [--ctx N]
"""
import os
import re
import sys

ROOTS = ["src-tauri/src", "src-tauri/crates", "remote-daemon-proto/src"]
WT = "/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r24c"

# 「加执行位」的两种写法（本仓现打只有这两种）
EXEC_BIT = re.compile(r"(?:from_mode|set_mode)\(\s*0o([0-7]{3,4})\s*\)")


def files():
    out = []
    for root in ROOTS:
        for dirpath, _, names in os.walk(os.path.join(WT, root)):
            if "/vendor/" in dirpath.replace(os.sep, "/") or "/target/" in dirpath:
                continue
            for n in sorted(names):
                if n.endswith(".rs"):
                    out.append(os.path.join(dirpath, n))
    return sorted(out)


def main():
    ctx = 3
    if "--ctx" in sys.argv:
        ctx = int(sys.argv[sys.argv.index("--ctx") + 1])
    fs = files()
    print(f"分母① 扫到的 `.rs` 文件（三个根，去掉 vendor / target）= {len(fs)}")
    hits = []
    for path in fs:
        lines = open(path, encoding="utf-8", errors="replace").read().split("\n")
        for i, line in enumerate(lines):
            for m in EXEC_BIT.finditer(line):
                mode = int(m.group(1), 8)
                if mode & 0o111 == 0:
                    continue          # 不带执行位 ⇒ 连候选面都不是
                hits.append((path, i + 1, mode, lines[max(0, i - ctx):i + ctx + 1]))
    print(f"分母② 候选面（加了执行位的处数）= {len(hits)}\n")
    for path, ln, mode, block in hits:
        rel = os.path.relpath(path, WT)
        print(f"── {rel}:{ln}  mode=0o{mode:o} ──")
        for b in block:
            print("   " + b.rstrip())
        print()


if __name__ == "__main__":
    main()
