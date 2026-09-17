#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-G8 第四把尺子：**把那条 flaky 判据的分子分母打大** —— 连打 N 趟，逐趟记 rc 与红了哪条。

# 它量的是哪条

`remote-daemon-proto/src/observe/accounts_query.rs`
`observe::accounts_query::tests::an_inherited_launch_id_is_never_reported_as_the_childs_own_identity`

上一拍（09-03 摸底）现打 **2 红 / 14 趟**（同一棵树、同一份内容、同一沙箱）。
PM 裁：它与「23 套的实得稳不稳」是**同一件事的两个面** —— 三者（地板太低恒绿 /
地板太高恒红 / 判据本身会浮动）都让「那个数」不再能拿来判事 ⇒ **同拍买**。

🔴 **只量、只报，不许修** —— 那个文件不在 `K-G8` 的写区。

# 两种模式，各自量的不是同一件事（别混成一个数）

  `--mode full`   ：跑门禁第 ③ 格**逐字同一条**命令 `cd remote-daemon-proto && cargo test`。
                    ⇒ 与门禁**同条件**：整个 lib 测试二进制、默认多线程并跑、同一台机器上
                    同时有几百条别的测试在抢 CPU / /proc / 进程表。
  `--mode single` ：只跑那一条（`cargo test --lib <全名> -- --exact`）。
                    ⇒ **换了条件**：没有并发负载。两种模式的读数**不可互相顶替** ——
                    `single` 全绿只说明「单独跑抓不到」，**不说明它不 flaky**。

# 红线

🔴 只在沙箱里跑（`K31`）。本脚本**拒绝在容器外执行**，判据是 `/.dockerenv` 存在。
⚠ 跑之前确认 `CARGO_TARGET_DIR` 指到一个**热的** target（否则第一趟要重编几分钟，
  那一趟的墙钟不可比）。门禁那条跑法把它指到 `.claude/pm-targets/<tag>`。

# 用法（在沙箱里，工作目录 = 工作树根）

  python3 evidence/K-G8-flaky-probe.py --runs 20 --mode full  --out <落日志的目录>
  python3 evidence/K-G8-flaky-probe.py --runs 20 --mode single --out <落日志的目录>

每趟落 `<mode>-<序号>.log`；末尾打一行 `红 R / 共 N`。
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import time

TEST = ("observe::accounts_query::tests::"
        "an_inherited_launch_id_is_never_reported_as_the_childs_own_identity")

# `test result: FAILED. 527 passed; 2 failed; …` / `test result: ok. 529 passed; …`
RE_RESULT = re.compile(r"test result: (\w+)\. (\d+) passed; (\d+) failed")
# 失败清单里那一行（`failures:` 之后的缩进行）
RE_FAILED_NAME = re.compile(r"^\s{4}(\S+)$")


def failing_tests(text: str) -> list[str]:
    """从 `failures:` 段里把失败测试名抠出来（cargo 会打两段，去重）。"""
    out: list[str] = []
    grab = False
    for line in text.splitlines():
        if line.strip() == "failures:":
            grab = True
            continue
        if grab:
            m = RE_FAILED_NAME.match(line)
            if m and "::" in m.group(1):
                if m.group(1) not in out:
                    out.append(m.group(1))
            elif line.strip() == "" or line.startswith("test result"):
                grab = False
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description="K-G8：flaky 判据的分子分母")
    ap.add_argument("--runs", type=int, default=20)
    ap.add_argument("--mode", choices=("full", "single"), default="full")
    ap.add_argument("--out", required=True)
    ap.add_argument("--repo", default=".")
    args = ap.parse_args()

    if not os.path.exists("/.dockerenv"):
        print("❌ 本脚本只许在沙箱里跑（K31）。没看见 /.dockerenv ⇒ 拒绝执行。", file=sys.stderr)
        return 3

    repo = os.path.abspath(args.repo)
    proto = os.path.join(repo, "remote-daemon-proto")
    os.makedirs(args.out, exist_ok=True)

    if args.mode == "full":
        cmd = ["cargo", "test"]
    else:
        cmd = ["cargo", "test", "--lib", TEST, "--", "--exact"]

    print(f"# 模式 {args.mode}   趟数 {args.runs}   命令 {' '.join(cmd)}")
    print(f"# cwd {proto}   CARGO_TARGET_DIR={os.environ.get('CARGO_TARGET_DIR', '<未设>')}")
    print()

    red = 0
    red_is_target = 0
    for i in range(1, args.runs + 1):
        log = os.path.join(args.out, f"{args.mode}-{i:03d}.log")
        start = time.time()
        proc = subprocess.run(cmd, cwd=proto, capture_output=True, text=True,
                              errors="replace")
        text = proc.stdout + proc.stderr
        with open(log, "w", encoding="utf-8") as fh:
            fh.write(text)
        dur = int(time.time() - start)
        res = RE_RESULT.findall(text)
        # 一个包可能出多行 test result（lib / bin / 集成）—— 全都记下来
        summary = " ".join(f"{v}:{p}p/{f}f" for v, p, f in res) or "<无 test result 行>"
        fails = failing_tests(text)
        hit = any(TEST in f for f in fails)
        if proc.returncode != 0:
            red += 1
            if hit:
                red_is_target += 1
        flag = "🔴" if proc.returncode != 0 else "  "
        extra = ("  ← 目标那条" if hit else ("  ← 红的是别条: " + ",".join(fails))
                 if proc.returncode != 0 else "")
        print(f"{flag} {i:>3}/{args.runs}  rc={proc.returncode:<3} {dur:>4}s  {summary}{extra}",
              flush=True)

    print()
    print(f"# 合计：红 {red} / 共 {args.runs} 趟   其中「红的是目标那条」{red_is_target} 趟")
    print("⚠ 读法：N 趟全绿**不等于**不 flaky，只是把不稳的下界抬高了一格。带着趟数报，别写「稳」。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
