#!/usr/bin/env python3
"""K-P5b 的读数量具：起会话方人群 · 两个锚点 · 「只有一份」那两个针。

跑法（零依赖、只读）：
    python3 evidence/K-P5b-launcher-identity-census.py

被测对象 = **它自己所在的那棵树**（`Path(__file__).resolve().parents[1]`）。
第一行印的就是「被测树 + 量于哪个提交」—— **复跑前先看那一行**，
两份同名量具住过不同的树时，输出长得一模一样而数来自另一棵树。

⚠ 本文件只是让 PM 能复跑同一组数；**买住这些数的不是它，是判据**：
  · `history.rs::the_launcher_plants_the_session_identity_into_the_process_environment`
  · `launcher_identity_registry.rs` 那四条
本量具**不判对错**，只印读数与分母。
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# ── 与判据同源的口径（判据在 Rust 那侧，这里是它的可读副本；两边漂开时以判据为准）──
LAUNCH_CAPS = {"session.launch", "launch.send-into"}
ANCHORS = [
    ("shared/ccm", 'exec "${argv[@]}"'),
    ("shared/ccm", 'exec bash -c "$seq"'),
    ("src-tauri/src/profile_installer.rs", "& claude $RemainingArgs"),
]
PROBES = [("CCM_LAUNCH_ID", "身份变量名的字面量"), ("route_key_for_session(", "那一份铸法的调用形")]


def head() -> str:
    try:
        sha = subprocess.run(
            ["git", "-C", str(ROOT), "rev-parse", "--short", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
    except Exception as e:  # noqa: BLE001 - 量具，读不到就如实说
        sha = f"<读不到：{e}>"
    return sha


def strip_hash(src: str) -> str:
    """shell：只剥**整行** `#` 注释。行尾注释剥不掉 —— 这是上界，不是精确值。"""
    return "\n".join(l for l in src.splitlines() if not l.lstrip().startswith("#"))


def strip_rs(src: str) -> str:
    """Rust：剥 `#[cfg(test)]` 段 + 整行 `//` 注释。

    ⚠ **它是 `guard_core::production_code` 的近似**，不是同一份实现：
    这里按缩进 + 花括号配平粗切 `#[cfg(test)]` 的下一项，行尾注释**不剥**。
    ⇒ 本量具印的数可能比判据的数**大**（多算了行尾注释里的命中）。
    两边对不上时**以判据为准**，别拿这里的数去驳它。
    """
    lines = src.splitlines()
    out, i = [], 0
    while i < len(lines):
        if lines[i].strip() == "#[cfg(test)]":
            # 跳过下一项：找到第一个 `{`，配平到它的 `}`。
            depth, started, i = 0, False, i + 1
            while i < len(lines):
                depth += lines[i].count("{") - lines[i].count("}")
                if "{" in lines[i]:
                    started = True
                i += 1
                if started and depth <= 0:
                    break
            continue
        if not lines[i].lstrip().startswith("//"):
            out.append(lines[i])
        i += 1
    return "\n".join(out)


def ledger_rows(raw: str):
    """按**行的形状**抠 `("<命令>", "<能力>", …)` —— 与判据同一口径。

    只认单行三元组 ⇒ 抠到的总数是**下界**，不是账本大小
    （`K-P5 §3 六` 记过同一格：116 / 141 / 145 是三把作用域不同的尺子）。
    """
    rows = []
    for line in raw.splitlines():
        m = re.match(r'^\("([^"]+)",\s*"([^"]+)"', line.strip())
        if m:
            rows.append((m.group(1), m.group(2)))
    return rows


def main() -> int:
    print(f"被测树 = {ROOT}")
    print(f"量于提交 = {head()}")
    print()

    ledger = ROOT / "src-tauri/src/parity_ledger.rs"
    rows = ledger_rows(ledger.read_text(encoding="utf-8"))
    caps = {c for _, c in rows}
    print(f"一 · 账本那半（分母 = `{ledger.relative_to(ROOT)}` 里抠到的单行三元组 {len(rows)} 行 / "
          f"{len(caps)} 种能力）")
    for cmd, cap in rows:
        if cap in LAUNCH_CAPS:
            print(f"    {cap:<20} {cmd}")
    print(f"    ⇒ 起会话方（账本那半）恰好 "
          f"{sum(1 for _, c in rows if c in LAUNCH_CAPS)} 处")
    print()

    print("二 · 终端那半（人点的 —— 它们不是 tauri 命令，结构上进不了账本）")
    for rel, needle in ANCHORS:
        p = ROOT / rel
        raw = p.read_text(encoding="utf-8")
        prod = strip_rs(raw) if rel.endswith(".rs") else strip_hash(raw)
        print(f"    {rel:<40} `{needle}` 生产段 {prod.count(needle)} 处 "
              f"（原文 {raw.count(needle)} 处 · 剥完 {len(prod)} 字节 / 原文 {len(raw)}）")
    print()

    rs = sorted((ROOT / "src-tauri/src").rglob("*.rs"))
    print(f"三 · 「只有一份」那两个针（分母 = `src-tauri/src` 下 {len(rs)} 份 `.rs` 的生产段）")
    for needle, what in PROBES:
        hits = []
        for f in rs:
            n = strip_rs(f.read_text(encoding="utf-8")).count(needle)
            if n:
                hits.append(f"{f.relative_to(ROOT)} × {n}")
        print(f"    `{needle}`（{what}）合计 "
              f"{sum(int(h.rsplit('× ', 1)[1]) for h in hits)} 处：")
        for h in hits:
            print(f"        {h}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
