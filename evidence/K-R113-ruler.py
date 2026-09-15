#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`K-R113` 的尺子 —— **只出读数、不判红**（退出码恒 0，除非自己坏了）。

它答四个问题，每个都**现算**：

1. 后端的两个命令面今天各有几条：CLI 面 `main.rs::SUBCOMMANDS` · 帧面
   `inbound.rs::REGISTRY`（单一事实源）与 `inbound.rs::COMMANDS`（镜子）。
2. 两张帧面表对不对得上（**数据对数据**，不是数字面量）。
3. `control/cc_bus.rs` 的**生产段**今天经通用调用口转调了哪几条 cc-bus 命令。
4. 今天的 `BUILD_ID`。

住址：本文件住 `<某棵工作树>/evidence/`；默认量的是**它自己所在的那棵树**，
`--daemon-root` 可以指到别处（`brief` 12：量具住址要唯一定位到被测对象，不许拿 `cwd` 猜）。
每次跑都把「被测对象是哪棵树 · 那棵树的尖是哪个提交」印在最前面 —— 少了这两行，
同一张读数表下一轮就说不清它量的是谁。

⚠ **它剥的是「行首 `//` 注释」**，不是 Rust 编译器那套：足够把 `SUBCOMMANDS` 块里
那几段散文里的 `--dial` / `--relay` 剥掉（它们都在整行注释里），**接不住行尾注释**。
接不住的形状写在这里，别当它是编译器。
"""
import argparse
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
DEFAULT_ROOT = HERE.parent


def strip_line_comments(text: str) -> str:
    out = []
    for line in text.splitlines():
        s = line.lstrip()
        if s.startswith("//"):
            continue
        out.append(line)
    return "\n".join(out)


def block_of(text: str, head: str) -> str:
    i = text.find(head)
    if i < 0:
        print(f"❌ 找不到 `{head}` —— 锚点变了，本尺子此刻量不出东西")
        sys.exit(3)
    j = text.find("\n];", i)
    if j < 0:
        print(f"❌ `{head}` 的块没有收尾 —— 抽取坏了")
        sys.exit(3)
    return text[i:j]


def quoted(text: str) -> list[str]:
    return re.findall(r'"([^"\\\n]*)"', text)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--daemon-root", default=str(DEFAULT_ROOT / "remote-daemon-proto"))
    a = ap.parse_args()
    root = Path(a.daemon_root).resolve()
    tree = root.parent
    try:
        tip = subprocess.run(
            ["git", "-C", str(tree), "rev-parse", "--short", "HEAD"],
            capture_output=True, text=True, timeout=20,
        ).stdout.strip() or "<读不到>"
    except Exception:  # noqa: BLE001
        tip = "<读不到>"
    print(f"被测对象：{root}")
    print(f"那棵树的尖：{tip}（`git rev-parse --short HEAD`，现打）")
    print("—— 本尺子只出读数、不判红 ——\n")

    main_rs = (root / "src" / "main.rs").read_text(encoding="utf-8")
    inbound_rs = (root / "src" / "inbound.rs").read_text(encoding="utf-8")
    ccbus_rs = (root / "src" / "control" / "cc_bus.rs").read_text(encoding="utf-8")

    subs = quoted(strip_line_comments(block_of(main_rs, "const SUBCOMMANDS: &[&str] = &[")))
    print(f"[CLI 面] `SUBCOMMANDS` **{len(subs)}** 条")
    print("   " + " ".join(sorted(subs)))

    cmds = quoted(strip_line_comments(block_of(inbound_rs, "pub const COMMANDS: &[&str] = &[")))
    reg_block = block_of(inbound_rs, "pub(crate) const REGISTRY: &[CommandSpec] = &[")
    reg = re.findall(r'name:\s*"([^"]+)"', reg_block)
    print(f"\n[帧面] `REGISTRY`（单一事实源）**{len(reg)}** 条 · `COMMANDS`（镜子）**{len(cmds)}** 条")
    print("   REGISTRY: " + " ".join(sorted(reg)))
    only_reg = sorted(set(reg) - set(cmds))
    only_cmd = sorted(set(cmds) - set(reg))
    print(f"   只在 REGISTRY：{only_reg or '（无）'} · 只在 COMMANDS：{only_cmd or '（无）'}")

    # `cc_bus.rs` 的生产段（剥测试段：从 `#[cfg(test)]` 那一行截断 —— 本文件只有一个测试模块，
    # 且它在文件末尾。⚠ 这条前提是**现打**的，下面那行断言会说出它成不成立）。
    marker = "#[cfg" + "(test)]"
    assert ccbus_rs.count(marker) == 1, f"cc_bus.rs 里有 {ccbus_rs.count(marker)} 个测试模块标记 —— 本尺子的剥法只接得住 1 个"
    prod = ccbus_rs.split(marker)[0]
    calls = sorted(set(re.findall(r'run(?:_as)?\("([^"]+)"', prod)))
    print(f"\n[转调] `control/cc_bus.rs` 生产段经通用调用口转调 **{len(calls)}** 条 cc-bus 命令")
    print("   " + " ".join(calls))

    bid = re.search(r'const BUILD_ID: &str = "([^"]+)";', main_rs)
    print(f"\n[身份] `BUILD_ID` = {bid.group(1) if bid else '<抠不出来>'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
