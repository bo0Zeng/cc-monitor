#!/usr/bin/env python3
"""`K-R89` 的两把尺子 + 一把 PM 没给的第三把 —— **自己量，不抄单子上的数**。

尺子 A（`TS_FALLBACK_KEEPERS` 那把）：`renderFallback` / `SESSION_BACKEND` 在 `src/` 生产段的处数。
   剥法与 `launch_wire.rs::production_ts` **同口径**：整行 `//` / `*` / `/*` ＋ 行尾 `//`；
   人群 = `src/**.ts(x)` 去掉 `*.test.ts` / `*.vitest.ts`（`TS_FALLBACK_KEEPERS` 头注逐字）。

尺子 B（`daemon_kill.rs::CREATION_PATHS` 那把）：全仓生产段里真正产 `tmux new-session` 的文件。
   ⚠ 这把的发现机制是**遍历**，不是清单。

尺子 C（PM 单子上**没有**的那把）：那 5 个 builder **有没有生产调用方**。
   —— `TS_FALLBACK_KEEPERS` 数的是「文件里那个标识符出现几次」，它答不了「谁在调这个文件导出的东西」。
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()


def production_ts(src: str) -> str:
    out = []
    for line in src.splitlines():
        t = line.lstrip()
        if t.startswith("//") or t.startswith("*") or t.startswith("/*"):
            out.append("")
            continue
        i = line.find("//")
        out.append(line[:i] if i >= 0 else line)
    return "\n".join(out)


def ts_files(sub: str, keep_tests: bool):
    for p in sorted((ROOT / sub).rglob("*")):
        if p.suffix not in (".ts", ".tsx", ".mts"):
            continue
        if "node_modules" in p.parts:
            continue
        n = p.name
        if not keep_tests and (n.endswith(".test.ts") or n.endswith(".vitest.ts")):
            continue
        yield p


print("══ 尺子 A：`renderFallback` / `SESSION_BACKEND` 在 src/ 生产段的处数 ══")
print("   （口径：整行 // * /* ＋ 行尾 // 剥掉；去掉 *.test.ts / *.vitest.ts）")
for sym in ("renderFallback", "SESSION_BACKEND"):
    total = 0
    for p in ts_files("src", keep_tests=False):
        n = production_ts(p.read_text()).count(sym)
        if n:
            print(f"   {sym:16s} {p.relative_to(ROOT)}  {n}")
            total += n
    print(f"   {sym:16s} 合计 {total}")

print()
print("══ 尺子 B：全仓生产段里真正产 `tmux new-session` 的文件（遍历，不是清单）══")
NEEDLE = "tmux new-session"
hits = []
SKIP_DIRS = {"node_modules", "target", ".git", "dist", "pm-targets", ".claude"}
for p in sorted(ROOT.rglob("*")):
    if not p.is_file():
        continue
    if set(p.parts) & SKIP_DIRS:
        continue
    if p.suffix not in (".ts", ".tsx", ".mts", ".rs", ".sh", ".py"):
        continue
    try:
        raw = p.read_text()
    except (UnicodeDecodeError, OSError):
        continue
    if NEEDLE not in raw:
        continue
    prod = production_ts(raw) if p.suffix != ".rs" else raw
    if NEEDLE in prod:
        hits.append((str(p.relative_to(ROOT)), prod.count(NEEDLE)))
for f, n in hits:
    print(f"   {f}  {n}")
print(f"   命中文件数 {len(hits)}（⚠ 含测试/夹具/注释里提到它的 —— 这是**粗**读数，逐处要人看）")

print()
print("══ 尺子 C：那 5 个 builder 的**生产调用方**（PM 单子上没有这把）══")
BUILDERS = [
    "buildResumeDirectCmd",
    "buildResumeTmuxCmd",
    "buildResumeIntoExistingTmuxCmd",
    "buildLauncherCmd",
    "buildAttachCmd",
]
DEF = re.compile(r"export\s+function\s+(\w+)")
for b in BUILDERS:
    callers = []
    for p in sorted(list(ts_files("src", keep_tests=True)) + list(ts_files("e2e", keep_tests=True))):
        prod = production_ts(p.read_text())
        if f"{b}(" not in prod:
            continue
        rel = str(p.relative_to(ROOT))
        # 定义处不算调用方
        only_def = all(
            m.group(1) == b for m in DEF.finditer(prod)
        ) and prod.count(f"{b}(") == 1 and f"export function {b}(" in prod
        kind = "定义" if only_def else ("测试/e2e" if (".test." in rel or ".vitest." in rel or rel.startswith("e2e/")) else "生产")
        callers.append(f"{rel}[{kind}]")
    print(f"   {b:32s} {callers}")
