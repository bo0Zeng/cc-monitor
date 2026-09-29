#!/usr/bin/env python3
# ruff: noqa: E501
"""CF1：**本机会话内容走本机后端的 Line 帧** —— 真后端 × 生产读循环的现打。

跑法（仓根下）：
    python3 tests/evidence/CF1-local-lines.py [后端二进制路径]

缺省二进制是 `.build/backend/debug/cc-monitor-backend`（先 `cd src/backend && cargo build`）。
它带 `CF1_BACKEND` 跑 `local_lines_tests::a_real_backend_feeds_local_lines_through_the_production_read_loop`
（平时 `#[ignore]`）：起一个**流模式**的真后端（stdio 载体；起参就是生产那一份 `LOCAL_STREAM_ARGS`；私有 HOME /
`CLAUDE_CONFIG_DIR` / `TMUX_TMPDIR`、摘掉 `TMUX` —— `C7i` 红线，不碰用户真实的 tmux server），用生产那条
`local_stdio_consumer` 接上它，从本机内容通道上直接看送进来的东西。

覆盖：
  ① 一个活会话（冒充的 `sleep`，pidfile 的 `procStart` 取它自己的 `/proc` 启动时刻）被宣告，帧带 `path`、`lines == 2`
     （prime 到的完整行数；空白行不计）；宣告之前没有这个会话的行帧
  ② 追加一行 ⇒ 这个会话的下一条 `line` 帧就是它：`seq == 2`（行号）、原文不变 —— 历史两行不作为行帧重放（tail-only）
  ③ 后端被杀 ⇒ 读循环收尾送「流结束」

**不进门禁**（要一个编好的后端二进制与 `/proc`）—— 它是一份读数，不是判据；交付报告里贴的是它的输出。
退出码：0 = 全过 · 1 = 不对 · 3 = 找不到二进制（环境不满足，不是被测对象坏了）
"""

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DEFAULT_BIN = os.path.join(ROOT, ".build", "backend", "debug", "cc-monitor-backend")
TEST = "a_real_backend_feeds_local_lines_through_the_production_read_loop"


def main():
    bin_path = os.path.abspath(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_BIN
    if not os.access(bin_path, os.X_OK):
        print(f"找不到可执行的后端二进制：{bin_path}（先 `cd src/backend && cargo build`）")
        return 3
    env = {**os.environ, "CF1_BACKEND": bin_path}
    env.pop("TMUX", None)
    env.pop("TMUX_PANE", None)
    r = subprocess.run(
        ["cargo", "test", "-p", "monitor", "--lib", TEST, "--", "--ignored", "--nocapture", "--exact", f"local_lines::tests::{TEST}"],
        cwd=os.path.join(ROOT, "src", "frontend", "shell"),
        env=env,
        capture_output=True,
        text=True,
        timeout=3000,
    )
    ok = "CF1-LOCAL-LINES ok" in r.stdout and "1 passed" in r.stdout
    tail = [ln for ln in r.stdout.splitlines() if "CF1-LOCAL-LINES" in ln or "test result" in ln or "panicked" in ln]
    print("\n".join(tail) if tail else r.stdout[-1500:])
    if not ok:
        print(r.stderr[-1500:])
    print("\n全过" if ok else "\n不对")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
