#!/usr/bin/env python3
"""W-F1b 摸底量具 ③：**monitor 那段 `russh` 代码今天有没有「不带 GUI 的承载」**（`WF1bD2`）。

`WF1bD2` 的 acceptor 失效点逐字：「可以只报『盘上有个 `ssh_source.rs`』就说有入口 ——
🔴 **两维都要报**：**盘上有** 与 **实测跑得起来**。」
⇒ 本量具专管**第二维**：在**沙箱里、无 GUI、`--network none`** 的条件下，
把 `ssh_source.rs` 里那几条真的驱动 `russh` 客户端握手的测试跑一遍，看它们**真的跑得起来**。

## 为什么这一维不能靠读代码换

`src-tauri` 的 lib 依赖 `tauri`，Linux 上要链 `webkit2gtk-4.1` + `gtk-3`。
「源码里有个 `#[tokio::test]`」与「这台机器上它真的编得出来、跑得过」是两件事，
而后者正是「把 monitor 侧塞进容器」那条路的地基。

## 🔴 一律在沙箱里跑（本轮红线）

「宿主上一次 `cargo test` 都不许跑」。本量具复用 `.claude/devbox/gate` 那一套挂载
（项目目录 · crates 缓存具名卷 · `CARGO_TARGET_DIR` 指向 `pm-targets/<tag>`），
但**不跑九格门禁**，只跑点名的那几条测试 —— 门禁归门禁，读数归读数。
⚠ 本量具**不改** `gate`，也不经过它；两者互不影响。

## 分母怎么数的

  · 人群 = `cargo test --lib -- --list` 印出来的**全部**测试名（这是机器自己给的分母，
    不是 grep 出来的）。
  · 其中「真的驱动 russh 客户端」的那一族 = `ssh_source::tier1_tests` 下、
    名字里带 `race_` 的那几条 —— 它们经 `race_connect` → `client::connect`
    （`src-tauri/src/ssh_source.rs:573`，逐字 `match client::connect(config, ...)`）。
  · 🔴 **本量具不断言「只有这几条碰 russh」**（那是个给不出分母的全称）。
    它只报：**点名跑的这几条，在无 GUI 沙箱里的真实退出码与判定行**。

## 被测对象指向哪棵树

`WT`（下方常量）= `/home/zbl/文档/claudecode-frontend/.claude/worktrees/w-f1b`。
换树重跑要改这一个常量，别照抄输出当另一棵树的读数。

用法：  python3 evidence/W-F1b-headless-russh-carrier-probe.py
"""

from __future__ import annotations

import json
import re
import subprocess
import sys

PROJ = "/home/zbl/文档/claudecode-frontend"
WT = f"{PROJ}/.claude/worktrees/w-f1b"
TAG = "w-f1b"
SKILL = "/home/zbl/.claude-accts/z/skills/planned-build"
CARGO_CACHE = "ccmon-cargo-registry"
IMG = "ccmon-devbox:latest"

# 点名跑的那一族（理由见模块头注「分母怎么数的」）
FILTER = "ssh_source::tier1_tests::race_"


def sandbox(script: str, timeout: float = 3600.0) -> tuple[int, str, str]:
    """与 `.claude/devbox/gate` 同一套挂载，但跑点名的命令而不是九格门禁。"""
    cmd = [
        "docker", "run", "--rm",
        "--network", "none",
        "-v", f"{PROJ}:{PROJ}",
        "-v", f"{SKILL}:{SKILL}:ro",
        "-v", f"{CARGO_CACHE}:/opt/rust/cargo/registry",
        "-e", f"CARGO_TARGET_DIR={PROJ}/.claude/pm-targets/{TAG}",
        "-e", "HOME=/home/zbl",
        "-w", WT,
        IMG,
        "bash", "-o", "pipefail", "-c", script,
    ]
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    return p.returncode, p.stdout, p.stderr


def main() -> int:
    report: dict = {"worktree": WT, "filter": FILTER}

    # 沙箱自证：无 DISPLAY / 无 WAYLAND_DISPLAY ⇒「不带 GUI」这句话不是靠嘴说的
    rc, out, _ = sandbox(
        'echo "DISPLAY=[${DISPLAY:-<未设>}] WAYLAND=[${WAYLAND_DISPLAY:-<未设>}]"; '
        'ls /tmp/.X11-unix 2>&1 | head -1; '
        'echo -n "net-ifaces: "; cat /proc/net/dev | tail -n +3 | cut -d: -f1 | tr -d " " | tr "\\n" " "; echo',
        timeout=300)
    report["headless_env"] = {"rc": rc, "out": out.strip()}

    # ① 分母：机器自己印的全部测试名
    #
    # ⚠ 初版这里写的是 `--list 2>&1 | tail -400` —— **那是一个分母 bug**：`tail` 只留最后
    #   400 行，于是「印出来的测试名」被数成 398 条，而同一趟真跑的判定行写着
    #   `9 passed; 1325 filtered out` ⇒ 真分母是 1334。09-05 自查逮到，逐字记在这里，
    #   别再把一个被 `tail` 截过的人群当分母。〔`brief` 第 12 条：报的每个数，分母怎么数的都要写明〕
    rc, out, err = sandbox(
        'cargo test --manifest-path src-tauri/Cargo.toml --lib -- --list 2>&1')
    names = re.findall(r"^(\S+): test$", out, re.M)
    m = re.search(r"^(\d+) tests?, (\d+) benchmarks?$", out, re.M)
    report["list"] = {
        "rc": rc,
        "names_parsed": len(names),
        "harness_summary_line": m.group(0) if m else None,
        "tier1_race": sorted(n for n in names if FILTER in n),
    }

    # ② 真跑：点名那一族
    rc, out, err = sandbox(
        f'cargo test --manifest-path src-tauri/Cargo.toml --lib {FILTER} '
        f'-- --nocapture --test-threads=1 2>&1 | tail -60')
    bar = [ln for ln in out.splitlines() if ln.startswith("test result:")]
    report["run"] = {"rc": rc, "verdict_lines": bar, "tail": out.strip().splitlines()[-25:]}

    print("=" * 78)
    print("W-F1b · monitor 侧 russh 代码的「无 GUI 承载」实测")
    print(f"树：{WT}")
    print("=" * 78)
    print("【沙箱确实无 GUI】", report["headless_env"]["out"])
    print("-" * 78)
    print(f"【分母】`cargo test --lib -- --list` 解析到的测试名：{report['list']['names_parsed']} 条"
          f"（退出码 {report['list']['rc']}）· harness 自己印的汇总行："
          f"{report['list']['harness_summary_line']}")
    print(f"【点名的那一族】{FILTER}  ⇒ {len(report['list']['tier1_race'])} 条：")
    for n in report["list"]["tier1_race"]:
        print(f"    {n}")
    print("-" * 78)
    print(f"【真跑】退出码={report['run']['rc']}")
    for ln in report["run"]["verdict_lines"]:
        print(f"  判定行：{ln}")
    print("  尾部输出：")
    for ln in report["run"]["tail"]:
        print(f"    {ln}")
    print("=" * 78)
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
