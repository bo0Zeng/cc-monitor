#!/usr/bin/env python3
"""K-P4 摸底拍（`KP4Z2`）的量具 —— **今天有哪几个面能表达「这条命令在这台机器上做不到」**。

住址：`<工作树>/evidence/K-P4-Z2-capability-surface-census.py`（默认工作树 `.claude/worktrees/k-p4b`）

它不选方案，只把**候选摆法各自要动的那几处**量出来，逐处给分母与住址：

  面①  `inbound::REGISTRY`（单一真相源）+ `CommandSpec` 的字段
  面②  `inbound::COMMANDS`（`hello.commands` 的来源，**跨轨对拍锚**）
  面③  `main::SUBCOMMANDS`（CLI 面那道闸门，第三份手写镜子）
  面④  `hello` 帧的三条能力面（`capabilities` / `emits` / `commands`）与它们的 serde 形态
  面⑤  客户端侧的**预检**（`InboundClient::accepts` + `CallError::Unsupported`）
  面⑥  **调用时**说「这里做不到」的现成先例：命令级 code（`no_tmux` / `not_installed`）
        —— 连同 monitor 侧把它翻成人话的落点
  面⑦  **值级**先例（`observation` 三值）与协议级 code 闭集（`PROTOCOL_CODES`）

⚠ 尺子的射程：
  · 它数的是**源码文本**（正则切块 + 逐字子串）。「这条 code 真的发得出来」那一维
    只由「emit 住址」给一个**盘上有**，不给运行期读数 —— 本拍没有真机。
  · 「有没有平台字段」这一格用的是 needle 表（`platform|os|windows|capab|available|supported`）,
    ⇒ 它证的是「没有一个字段名**长得像**平台可用性」，不是「不存在任何表达平台的手段」。

跑法：
    python3 evidence/K-P4-Z2-capability-surface-census.py
"""

import argparse
import re
import sys
from pathlib import Path

PROJ = Path("/home/zbl/文档/claudecode-frontend")
DEFAULT_WT = PROJ / ".claude" / "worktrees" / "k-p4b"

REG_FLOOR = 5          # REGISTRY 至少几条才算切到了
SPEC_FIELD_FLOOR = 4   # CommandSpec 至少几个字段才算切到了
PLATFORMISH = ("platform", "target_os", "windows", "capab", "available", "supported", "os")


def block(src: str, start_pat: str, end_pat: str):
    """从匹配 `start_pat` 的那一行起，切到匹配 `end_pat` 的那一行为止（含）。切不到返回 None。"""
    lines = src.splitlines()
    s = re.compile(start_pat)
    e = re.compile(end_pat)
    for i, line in enumerate(lines):
        if s.search(line):
            for j in range(i + 1, len(lines)):
                if e.search(lines[j]):
                    return i + 1, j + 1, "\n".join(lines[i:j + 1])
            return None
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--wt", default=str(DEFAULT_WT))
    args = ap.parse_args()
    wt = Path(args.wt)
    rd = wt / "remote-daemon-proto" / "src"
    inbound = (rd / "inbound.rs").read_text(encoding="utf-8")
    wire = (rd / "wire.rs").read_text(encoding="utf-8")
    mainrs = (rd / "main.rs").read_text(encoding="utf-8")
    guard = (rd / "protocol_doc_guard.rs").read_text(encoding="utf-8")
    client = (wt / "src-tauri/src/inbound_client.rs").read_text(encoding="utf-8")
    ssh = (wt / "src-tauri/src/ssh_source.rs").read_text(encoding="utf-8")

    print("面① `inbound::REGISTRY` + `CommandSpec`")
    got = block(inbound, r"pub\(crate\) const REGISTRY", r"^\];")
    if not got:
        print("  CRASH: 切不出 REGISTRY 块 —— 尺子坏了")
        return 2
    a, b, reg = got
    names = re.findall(r'name:\s*"([^"]+)"', reg)
    print(f"  住址 inbound.rs:{a}-{b} · 命令 {len(names)} 条：{' '.join(sorted(names))}")
    if len(names) < REG_FLOOR:
        print(f"  CRASH: 只切出 {len(names)} 条（地板 {REG_FLOOR}）")
        return 2
    cfgs = [l.strip() for l in reg.splitlines() if "#[cfg" in l]
    print(f"  REGISTRY 块里的 `#[cfg` ：{len(cfgs)} 处  {cfgs if cfgs else '⇒ 一条命令都不按平台变'}")

    got = block(inbound, r"pub\(crate\) struct CommandSpec", r"^\}")
    if not got:
        print("  CRASH: 切不出 CommandSpec")
        return 2
    a, b, spec = got
    fields = re.findall(r"^\s*pub\(crate\) ([a-z_]+):", spec, re.M)
    print(f"  住址 inbound.rs:{a}-{b} · 字段 {len(fields)} 个：{fields}")
    if len(fields) < SPEC_FIELD_FLOOR:
        print(f"  CRASH: 只切出 {len(fields)} 个字段（地板 {SPEC_FIELD_FLOOR}）")
        return 2
    plat = [f for f in fields if any(p in f for p in PLATFORMISH)]
    print(f"  字段名里长得像「平台可用性」的：{len(plat)} 个 {plat}"
          f"（needle 表 {list(PLATFORMISH)}）")

    print()
    print("  每条命令自己的 `codes`（命令级错误码 —— 「这台机器上做不到」今天就住在这里）")
    for m in re.finditer(r'name:\s*"([^"]+)"[\s\S]*?codes:\s*&\[([^\]]*)\]', reg):
        nm = m.group(1)
        codes = re.findall(r'"([^"]+)"', m.group(2))
        mark = ""
        if "no_tmux" in codes:
            mark = "  ← 「这台机器没有 tmux」"
        if "not_installed" in codes:
            mark = "  ← 「这台机器没装那个东西」"
        print(f"    {nm:<10} {codes}{mark}")

    print()
    print("面② `inbound::COMMANDS`（`hello.commands` 的来源 · 跨轨对拍锚）")
    mm = re.search(r"pub const COMMANDS: &\[&str\] =\s*&\[([^\]]*)\]", inbound)
    cmds = re.findall(r'"([^"]+)"', mm.group(1)) if mm else []
    line_no = inbound[:mm.start()].count("\n") + 1 if mm else -1
    print(f"  住址 inbound.rs:{line_no} · {len(cmds)} 条：{cmds}")
    print(f"  与 REGISTRY 同集合：{sorted(cmds) == sorted(names)} · 它是 `pub const`，"
          f"块内 `#[cfg` {0 if mm and '#[cfg' not in mm.group(0) else '≥1'} 处")

    print()
    print("面③ `main::SUBCOMMANDS`（CLI 面那道闸门 —— 第三份手写镜子）")
    got = block(mainrs, r"const SUBCOMMANDS: &\[&str\]", r"^\];")
    if got:
        a, b, sub = got
        # ⚠ 只取 `--xxx` 形状的（块里的注释也带引号串，`存在` 那种是注释里的中文，不是子命令）
        subs = [s for s in re.findall(r'"([^"]+)"', sub) if s.startswith("--")]
        bare = {s.lstrip("-") for s in subs}
        print(f"  住址 main.rs:{a}-{b} · {len(subs)} 条（只数 `--` 打头的）：{subs}")
        print(f"  REGISTRY 有而 CLI 面没有的：{sorted(set(names) - bare)}"
              f"  （比的是去掉 `--` 之后的名字）")
    else:
        print("  CRASH: 切不出 SUBCOMMANDS")

    print()
    print("面④ `hello` 帧的三条能力面（daemon 侧 wire.rs）")
    for fname in ("capabilities", "emits", "commands", "homes"):
        for m in re.finditer(rf"^\s*{fname}: Vec<[^>]+>,", wire, re.M):
            ln = wire[:m.start()].count("\n") + 1
            prev = wire.splitlines()[ln - 2].strip()
            print(f"  wire.rs:{ln}  {fname:<13} 上一行 serde: {prev[:60]}")
            break

    print()
    print("面⑤ 客户端侧预检（monitor）")
    for pat, label in (
        (r"pub fn accepts\(&self, cmd: &str\) -> bool", "InboundClient::accepts"),
        (r"Unsupported \{ cmd: String, offered: Vec<String> \}", "CallError::Unsupported"),
        (r"if !self\.accepts\(cmd\)", "call() 里的预检臂"),
    ):
        m = re.search(pat, client)
        ln = client[:m.start()].count("\n") + 1 if m else None
        print(f"  {label:<26} {'inbound_client.rs:' + str(ln) if ln else '×  零命中'}")
    m = re.search(r"多余字段仍忽略", ssh)
    print(f"  monitor 侧 Hello 的 additive 自陈：ssh_source.rs:"
          f"{ssh[:m.start()].count(chr(10)) + 1 if m else '× 零命中'}"
          f"  逐字「多余字段仍忽略（向前兼容）」")

    print()
    print("面⑥ 「调用时说做不到」的 emit 住址（后端自报 · 运行期探测）")
    for rel in ("control/kill.rs", "control/gate.rs", "control/launch.rs", "control/cc_bus.rs"):
        src = (rd / rel).read_text(encoding="utf-8")
        for code in ('"no_tmux"', '"not_installed"'):
            for m in re.finditer(re.escape(code), src):
                ln = src[:m.start()].count("\n") + 1
                print(f"  {rel}:{ln}  {code}")
    print("  monitor 侧把它翻成人话的落点：")
    for rel in ("backend/control/daemon_launch.rs", "backend/control/daemon_kill.rs",
                "backend/control/daemon_send_keys.rs"):
        p = wt / "src-tauri/src" / rel
        if not p.exists():
            continue
        src = p.read_text(encoding="utf-8")
        for m in re.finditer(r'"no_tmux" => ([^\n]+)', src):
            ln = src[:m.start()].count("\n") + 1
            print(f"  {rel}:{ln}  {m.group(1).strip()[:52]}")

    print()
    print("面⑦ 值级先例 + 协议级 code 闭集")
    watcher = (rd / "observe/watcher.rs").read_text(encoding="utf-8")
    for m in re.finditer(r'const OBS_([A-Z_]+): &str = "([^"]+)"', watcher):
        ln = watcher[:m.start()].count("\n") + 1
        print(f"  observe/watcher.rs:{ln}  observation = \"{m.group(2)}\"")
    got = block(guard, r"const PROTOCOL_CODES", r"\];")
    if got:
        a, b, pc = got
        codes = re.findall(r'"([^"]+)"', pc)
        print(f"  protocol_doc_guard.rs:{a}-{b} · 协议级 code **闭集** {len(codes)} 条：{codes}")
        print(f"  里面有没有一条意思是「这台机器上没有这个能力」："
              f"{[c for c in codes if 'support' in c or 'platform' in c] or '没有'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
