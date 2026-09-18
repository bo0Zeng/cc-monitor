#!/usr/bin/env python3
"""`K-R109` 的量具。**只读**，一条命令一格读数，每格自带分母与剥法。

住址：本文件住在 `<本工作树>/evidence/`；`ROOT` = **它所在的那棵树的仓根**
（`brief` 第 12 条：量具住址要唯一定位到那一份被测对象 —— 不许拿 `cwd` 猜）。
⚠ 与 `K-R105-ruler.py` / `K-R106-ruler.py` 是**三份不同的量具**，被测对象是三棵不同的树，
   读数别互相照搬。**跑之前先看它印出来的 `被测对象` 那一行指向哪棵树。**

用法：`python3 evidence/K-R109-ruler.py [格名 …]`（不给格名 = 全跑）。

格：
  seat-consumers    座（`SESSION_BACKEND`）今天有哪几个**生产**消费者文件
  command-params    `#[tauri::command]` 的入参里有几处 `&str`（剥注释 / 不剥注释各一份）
  counts            跟着「新增一条命令」走的那几个写死的数，逐处点名 + 现值
  fallback-reach    兜底那条路（`renderFallback`）今天的生产入口逐处

⚠ 它**不跑 cargo / npm**（那两样一律进沙箱）。它只读源码。
   `dead_code` 那一格量不了 —— 那要真编一趟，读数与命令逐字写在
   `evidence/K-R109-deathvalue.md` 的「dead_code 三读」一节。
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def strip_line_comments(src: str) -> str:
    """整行 `//` 注释置空（**保留行数**，别让行号漂）。

    ⚠ 射程如实写：它**不剥行尾注释**，也不剥 `/* */`。
    本量具用它的两格（`command-params` / `counts`）都不受行尾注释影响；
    `seat-consumers` 那一格用的是 Rust 侧同名判据的口径（见那一格）。
    """
    return "\n".join("" if l.lstrip().startswith("//") else l for l in src.split("\n"))


def ts_production(src: str) -> str:
    """与 `launch_wire::production_ts` 同口径：整行注释 + 行尾 `//` 一起去掉。"""
    out = []
    for l in src.split("\n"):
        t = l.lstrip()
        if t.startswith("//") or t.startswith("*") or t.startswith("/*"):
            out.append("")
            continue
        i = l.find("//")
        out.append(l[:i] if i >= 0 else l)
    return "\n".join(out)


def word(hay: str, needle: str) -> bool:
    return re.search(rf"(?<![A-Za-z0-9_$]){re.escape(needle)}(?![A-Za-z0-9_$])", hay) is not None


def seat_consumers() -> None:
    """座今天有哪几个生产消费者文件。

    人群 = `src/**.ts` 去掉 `*.test.ts` / `*.vitest.ts`（与 `launch_wire::production_ts` 同口径）。
    ⚠ 座自己（`session-backend.ts`）**单列**：它是被问的那一层，不是问的人
    （口径与 `doc_claim_registry` 量法 ② 逐字同源）。
    """
    files = sorted(p for p in (ROOT / "src").rglob("*.ts")
                   if not p.name.endswith((".test.ts", ".vitest.ts")))
    hits, seat = [], []
    for p in files:
        if not word(ts_production(p.read_text()), "SESSION_BACKEND"):
            continue
        rel = p.relative_to(ROOT).as_posix()
        (seat if rel == "src/session-backend.ts" else hits).append(rel)
    print(f"[seat-consumers] 人群（生产 TS）= {len(files)} 份")
    print(f"[seat-consumers] 座自己 = {seat}")
    print(f"[seat-consumers] 生产消费者 = {len(hits)} 份 {hits}")
    print("[seat-consumers] ⚠ 射程：整词匹配、剥完注释 ⇒ **别名 import 与动态取属性数不到**，不声称堵住。")


def command_params() -> None:
    """`#[tauri::command]` 的入参形态。**两份读数一起印** —— 剥法不同答案不同。"""
    for label, strip in (("剥整行注释", True), ("不剥", False)):
        tot, borrowed = 0, []
        for f in sorted((ROOT / "src-tauri" / "src").rglob("*.rs")):
            src = f.read_text()
            src = strip_line_comments(src) if strip else src
            for m in re.finditer(r"#\[tauri::command\b[^\]]*\]", src):
                fm = re.search(r"\bfn\s+([a-z_0-9]+)\s*\(", src[m.end():m.end() + 200])
                if not fm:
                    continue
                i, depth, end = m.end() + fm.end() - 1, 0, 0
                for j, c in enumerate(src[i:]):
                    if c == "(":
                        depth += 1
                    elif c == ")":
                        depth -= 1
                        if depth == 0:
                            end = i + j
                            break
                tot += 1
                if re.search(r"&\s*(?:'[a-z]+\s+)?str\b", src[i + 1:end]):
                    borrowed.append(f"{f.name}::{fm.group(1)}")
        print(f"[command-params] {label}：属性紧跟 fn 的处数 = {tot} · 入参含 `&str` = {len(borrowed)} {borrowed}")
    print("[command-params] ⚠ 处数 ≠ 命令数：`bring_monitor_to_front` 有两份 cfg 实现，同名 ⇒ 唯一名少 1。")


def counts() -> None:
    """跟着「新增一条命令」走的那几个写死的数 —— **逐处点名**，别只报「两个 147」。"""
    spots = [
        ("src/ipc/commands.vitest.ts", r"const RUST_COMMAND_COUNT = (\d+);", "Rust 声明=注册 的唯一命令名数"),
        ("src/ipc/commands.vitest.ts", r"const TS_LITERAL_COMMAND_COUNT = (\d+);", "TS 字面量命令名数"),
        ("src/ipc/commands.vitest.ts", r"包装层今天覆盖 \$\{keys\.length\} 个`\)\.toBe\((\d+)\)", "包装层条目数"),
        ("src-tauri/src/parity_ledger.rs", r"assert_eq!\(LEDGER\.len\(\), (\d+),", "平价对账表行数"),
        ("src-tauri/src/parity_ledger.rs", r"assert_eq!\(sides\.len\(\), (\d+),", "能力总数"),
        ("src-tauri/src/parity_ledger.rs", r"assert_eq!\(asym\.len\(\), (\d+),", "不对称能力数"),
        ("src-tauri/src/parity_ledger.rs", r"const EXPECTED_LOCAL_OR_BOTH: usize = (\d+);", "Local/Both 命令数"),
    ]
    for rel, pat, what in spots:
        m = re.search(pat, (ROOT / rel).read_text())
        print(f"[counts] {rel:38s} {what:22s} = {m.group(1) if m else '找不到（正则或措辞变了）'}")
    print("[counts] ⚠ 分母 = 这张表自己（7 处），**不是「盘上全部写死的数」** —— 那个分母我没数。")


def fallback_reach() -> None:
    """兜底那条路今天的生产入口逐处（尺子B 的粗版；权威读数在 `launch_wire::TS_FALLBACK_REACH`）。"""
    src = ts_production((ROOT / "src/remote-launch-run.ts").read_text())
    print(f"[fallback-reach] `renderFallback` 在 remote-launch-run.ts 生产段出现 {src.count('renderFallback')} 处")
    probe = (ROOT / "src/ccm-probe.ts").read_text()
    for st in ('"installed"', '"not-installed"', '"unknown"'):
        print(f"[fallback-reach] `ccm-probe.ts` 三态 {st}: {'在' if st in probe else '不在'}")
    print("[fallback-reach] ⚠ 本格只数字面，权威读数（有没有生产调用方）住 `launch_wire::TS_FALLBACK_REACH`。")


CELLS = {
    "seat-consumers": seat_consumers,
    "command-params": command_params,
    "counts": counts,
    "fallback-reach": fallback_reach,
}

if __name__ == "__main__":
    print(f"被测对象 = {ROOT}")
    want = sys.argv[1:] or list(CELLS)
    for name in want:
        if name not in CELLS:
            raise SystemExit(f"没有这一格：{name}（有的是 {list(CELLS)}）")
        CELLS[name]()
