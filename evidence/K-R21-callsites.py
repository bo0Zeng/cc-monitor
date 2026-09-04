#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R21 第一把尺子：`proc_env_var` 的**调用方人群**，以及每一处把那个 `None` 读成了什么。

# 它答的是 `KR21D1`

`remote-daemon-proto/src/platform/proc.rs::proc_env_var` 把**三件事**压成同一个 `None`：
  支一 `:87`  `std::fs::read("/proc/<pid>/environ").ok()?`  ⇒ 环境**读不到**
  支二 `:95`  `v.is_empty()` ⇒ `return None`               ⇒ 键**在**、值是**空串**
  支三 `:100` 落到函数尾                                    ⇒ **压根没这个键**

本尺子只回答「**今天谁在读它**」这一问，**不判对错**（对错写在件文件 §7）。

# 尺子怎么切的（这段就是分母的定义，转述时必须带上）

- **搜哪儿**：工作树根往下**整棵树**，排除 `target/`、`node_modules/`、`.git/`，
  以及**本脚本自己**（它的头注里逐字写着那三支与 `fn proc_env_var(`，
  第一版把自己数进去了 —— 合计从 25 虚报到含 10 处自指。**排除写在这里，不是省略**）。
- **搜什么**：正则 `\\bproc_env_var\\b`，**不限后缀**（不是只扫 `.rs` —— 文档/脚本里
  提到它也要进视野，否则「有几处知识依赖它」这个数会偏小）。
- **怎么分类**：每一处按所在行归成三类之一 ——
    · `调用`   ：行里有 `proc_env_var(` 且**不是** `fn proc_env_var(`，且**不在** `#[cfg(test)]` 段
    · `定义`   ：`fn proc_env_var(`
    · `提及`   ：其余（注释 / 文档 / 判据里的字符串字面量）
  🔴 「判据里数它的字符串」（`prod.matches("proc_env_var(pid, ")`）**归 `提及`，不归 `调用`**：
     它是一把**尺子**，不是一条读回路。把两者混成一个数正是本工作区最贵的那族病。
- **`#[cfg(test)]` 怎么判**：本尺子用**粗判** —— 一个文件里第一次出现 `#[cfg(test)]`
  之后的行全算测试段。⚠ 这对 `mod tests` 之后还有生产代码的文件会**判错**；
  本仓今天涉及的三个文件都是「测试段在文件尾」的形状，逐个人工核过。
  **这条限制写在这里，是因为它是本尺子的射程边界，不是省略。**

# 用法

    python3 evidence/K-R21-callsites.py [--repo <工作树根>]

它只读文件、不跑测试、不起进程 ⇒ 宿主上跑也不违反 `K31`（`K31` 管的是开发测试）。
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys

NEEDLE = re.compile(r"\bproc_env_var\b")
SKIP_DIRS = {"target", "node_modules", ".git", "dist", ".venv"}


def files(repo: str) -> list[str]:
    out: list[str] = []
    for root, dirs, names in os.walk(repo):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
        for n in names:
            out.append(os.path.join(root, n))
    return sorted(out)


def classify(line: str) -> str:
    if "fn proc_env_var(" in line:
        return "定义"
    # 判据里数字符串的那种写法：`.matches("proc_env_var(pid, ")`
    if '"proc_env_var' in line or "`proc_env_var" in line:
        return "提及"
    if "proc_env_var(" in line:
        return "调用"
    return "提及"


def main() -> int:
    ap = argparse.ArgumentParser(description="K-R21：proc_env_var 的调用方人群")
    ap.add_argument("--repo", default=".")
    args = ap.parse_args()
    repo = os.path.abspath(args.repo)

    try:
        sha = subprocess.run(["git", "-C", repo, "rev-parse", "HEAD"],
                             capture_output=True, text=True, check=True).stdout.strip()
    except Exception:
        sha = "<不是 git 树>"

    me = os.path.abspath(__file__)
    scanned = 0
    hits: list[tuple[str, int, str, str, bool]] = []
    for path in files(repo):
        if os.path.abspath(path) == me:
            continue  # 排除本脚本自己（见头注）
        try:
            with open(path, "r", encoding="utf-8") as fh:
                text = fh.read()
        except (UnicodeDecodeError, OSError, IsADirectoryError):
            continue
        scanned += 1
        if not NEEDLE.search(text):
            continue
        test_from = text.find("#[cfg(test)]")
        lines = text.splitlines()
        # 行号 → 是否落在测试段（粗判，见头注）
        off = 0
        for i, line in enumerate(lines, 1):
            in_test = test_from >= 0 and off >= test_from
            off += len(line) + 1
            if not NEEDLE.search(line):
                continue
            hits.append((os.path.relpath(path, repo), i, classify(line),
                         line.strip(), in_test))

    print(f"# 量于提交 {sha}")
    print(f"# 分母：可读文本文件 {scanned} 个（排除 {sorted(SKIP_DIRS)}）")
    print()
    kinds = {"定义": 0, "调用": 0, "提及": 0}
    prod_calls: list[tuple[str, int, str]] = []
    for rel, ln, kind, txt, in_test in hits:
        kinds[kind] += 1
        seg = "测试段" if in_test else "生产段"
        if kind == "调用" and not in_test:
            prod_calls.append((rel, ln, txt))
        print(f"{kind}  {seg}  {rel}:{ln}\n        {txt}")
    print()
    print(f"# 合计 {len(hits)} 处： 定义 {kinds['定义']} · 调用 {kinds['调用']} · 提及 {kinds['提及']}")
    print(f"# 🔴 **生产段的调用点 = {len(prod_calls)} 处**（这才是 `KR21D1` 要的那个人群）：")
    for rel, ln, txt in prod_calls:
        print(f"    {rel}:{ln}   {txt}")
    print()
    print("⚠ 本尺子不判「读错了会怎样」—— 那要读调用方的下游，写在件文件 §7。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
