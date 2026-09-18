#!/usr/bin/env python3
"""`K-R105` 的尺子 —— **两把**，而本件的正题就是「它们不是同一把」。

住址：本文件住在 `.claude/worktrees/k-r105/evidence/`；`ROOT` = **它所在的那棵树的仓根**
（不是 `cwd`，不是主树）——`brief` 第 12 条：量具住址要能唯一定位到那一份被测对象。

  尺子A  剥完注释的生产段里，那个**标识符出现几次**（`import` 那行算一处）。
         = `launch_wire.rs::TS_FALLBACK_KEEPERS` 的口径。它答的是「盘上还有谁提到它」。
  尺子B  那个消费者文件**有没有生产调用方**。= `launch_wire.rs::TS_FALLBACK_REACH` 的口径。
         它答的是「这条路今天还站不站在生产上」。

🔴 两把尺子的读数差着一个数量级，而 08-14 那句散文（「N 个生产消费者」）
   **用尺子A 的数说了尺子B 的话**，还传抄了好几份。本文件把两个读数分开印。

跑法：`python3 evidence/K-R105-ruler.py`（只读，不写盘，不需要沙箱）。
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# ── 剥法 ────────────────────────────────────────────────────────────────────
# 与 `launch_wire.rs::production_ts` 同口径：块注释 → 整行 `//` / `*` / `/*` → 行尾 `//`。
# ⚠ 如实写它剥不掉的：跨行模板串里的 `//`、raw 串。本组语料里没有（下面 self_check 量它）。


def strip_block(s: str) -> str:
    """块注释等长抹空格（行数不变）。

    ⚠ **不配平就整份原样返回** —— 与 `guard_core::strip_block_comments` 逐字同一条兜底
    （`try_… .unwrap_or_else(|| src.to_string())`）。那条兜底是「宁可留洞」，
    而留下的洞正是它要治的那一个 ⇒ 触发时**出声**，别静默。
    """
    out, i, depth = [], 0, 0
    while i < len(s):
        if s.startswith("/*", i):
            depth += 1
            out.append("  ")
            i += 2
            continue
        if s.startswith("*/", i) and depth > 0:
            depth -= 1
            out.append("  ")
            i += 2
            continue
        c = s[i]
        out.append(" " if (depth and c != "\n") else c)
        i += 1
    if depth != 0:
        return s
    return "".join(out)


def production_ts(src: str) -> str:
    lines = []
    for line in strip_block(src).split("\n"):
        t = line.lstrip()
        if t.startswith("//") or t.startswith("*") or t.startswith("/*"):
            lines.append("")
            continue
        j = line.find("//")
        lines.append(line[:j] if j >= 0 else line)
    return "\n".join(lines)


def strip_cfg_test(src: str) -> str:
    """Rust：剥掉 `#[cfg(test)]` 修饰的 item（按花括号配平），行数保留。"""
    lines = src.split("\n")
    out = list(lines)
    i = 0
    while i < len(lines):
        if re.match(r"\s*#\[cfg\(test\)\]", lines[i]):
            j, depth, started = i, 0, False
            while j < len(lines):
                depth += lines[j].count("{") - lines[j].count("}")
                if "{" in lines[j]:
                    started = True
                out[j] = ""
                if started and depth <= 0:
                    break
                if not started and lines[j].rstrip().endswith(";") and j > i:
                    break
                j += 1
            i = j + 1
            continue
        i += 1
    return "\n".join(out)


def production_rs(src: str) -> str:
    return production_ts(strip_cfg_test(src))


def prod_ts_files():
    for p in sorted((ROOT / "src").rglob("*.ts")):
        if p.name.endswith(".test.ts") or p.name.endswith(".vitest.ts"):
            continue
        yield p


def rel(p: Path) -> str:
    return str(p.relative_to(ROOT)).replace("\\", "/")


# ── 自检：剥法真的在剥，且行尾截断在本组语料上安全 ──────────────────────────
def self_check() -> None:
    # 与 `launch_wire.rs::the_ts_comment_stripper_actually_strips` **逐字同一组语料**：
    # 整行 `//` · 块注释续行 `*` · 不配平的 `/*`（走兜底）· 行尾 `//`。
    assert production_ts("// a\n * b\n/* c\nd // e\nf") == "\n\n\nd \nf", "剥法坏了"
    # 配平的块注释被等长抹成空格（**行数不变**，这是 guard-core 刻意的：留住行号）。
    assert production_ts("/* c */\nd") == "       \nd", "块注释没抹成等长空格"
    # 不配平 ⇒ 兜底原样返回（与 guard-core 同）；这一条把那条兜底钉成读数，不让它静默。
    assert strip_block("/* 没收口") == "/* 没收口", "块注释兜底没了"
    assert production_ts("let x = 1;") == "let x = 1;", "剥法把好行也剥了"
    mark = ":" + "//"
    for p in prod_ts_files():
        if mark in p.read_text(encoding="utf8"):
            print(f"  ⚠ {rel(p)} 里有 `{mark}` 字面量 —— 行尾截断会造假阴性", file=sys.stderr)
    n = len(list(prod_ts_files()))
    assert n >= 100, f"只收到 {n} 份生产 TS —— 人群缩水了，下面全是零命中地绿"


def ruler_a() -> None:
    print("═══ 尺子A：剥完注释的生产段里，标识符出现几次（`src/**` 去 `*.test.ts`/`*.vitest.ts`）")
    for sym in ("renderFallback", "SESSION_BACKEND"):
        rows = []
        for p in prod_ts_files():
            n = production_ts(p.read_text(encoding="utf8")).count(sym)
            if n:
                rows.append((rel(p), n))
        print(f"\n  {sym}：{len(rows)} 个文件 / 共 {sum(n for _, n in rows)} 处")
        for f, n in rows:
            print(f"      {f:42s} {n}")


# `(消费者文件, 把兜底那条路接出去的导出符号)` —— 与 `TS_FALLBACK_REACH` 同一份挑法。
REACH_TARGETS = [
    ("src/launch-payload-golden.ts", ["renderGoldenFixture", "GOLDEN_CASES"]),
    (
        "src/remote-launch.ts",
        [
            "buildResumeDirectCmd",
            "buildResumeTmuxCmd",
            "buildResumeIntoExistingTmuxCmd",
            "buildLauncherCmd",
            "buildAttachCmd",
        ],
    ),
    ("src/remote-launch-run.ts", ["runRemoteResume", "runRemoteResumeTmux", "runRemoteLauncher"]),
    ("src/launch-render-fallback.ts", ["renderFallback"]),
]


IDENT = re.compile(r"[A-Za-z0-9_]")


def contains_word(hay: str, needle: str) -> bool:
    """整词命中（与 `guard_core::contains_word` 同口径：两侧不许是标识符字符）。

    🔴 **裸子串会骗人**：建这张表当天实测，`CLI_GOLDEN_CASES` 命中了 `GOLDEN_CASES`
    ⇒ 一个夹具发生器被读成「生产调用方」，判定当场从 Off 翻成 On。
    """
    i = hay.find(needle)
    while i >= 0:
        before_ok = i == 0 or not IDENT.match(hay[i - 1])
        j = i + len(needle)
        after_ok = j >= len(hay) or not IDENT.match(hay[j])
        if before_ok and after_ok:
            return True
        i = hay.find(needle, i + 1)
    return False


# 已登记为 Off 的那几家不算「生产调用方」—— 否则一群互相引用的死代码会集体读成 On。
OFF_FILES = {"src/launch-payload-golden.ts", "src/remote-launch.ts"}


def ruler_b() -> None:
    print("\n═══ 尺子B：这个消费者文件有没有**生产调用方**（整词 · 同一份剥法 · 去掉它自己 · 跳过 Off 家）")
    for target, exports in REACH_TARGETS:
        callers, off_callers = [], []
        for p in prod_ts_files():
            if rel(p) == target:
                continue
            code = production_ts(p.read_text(encoding="utf8"))
            if any(contains_word(code, s) for s in exports):
                (off_callers if rel(p) in OFF_FILES else callers).append(rel(p))
        verdict = "OnProductionPath" if callers else "OffProductionPath"
        print(f"\n  {target}")
        print(f"      挑的导出：{exports}")
        print(f"      生产调用方 {len(callers)} 处 ⇒ {verdict}")
        for c in callers:
            print(f"        · {c}")
        if off_callers:
            print(f"      已登记为 Off、因而不计入的调用方：{off_callers}")
        # 非生产人群单列 —— 「生产调用方 0」不等于「没人调」。
        others = []
        for base in ("src", "e2e"):
            d = ROOT / base
            if not d.is_dir():
                continue
            for p in sorted(d.rglob("*")):
                if p.is_dir() or p.suffix not in (".ts", ".mts", ".mjs"):
                    continue
                if rel(p) == target or p in set(prod_ts_files()):
                    continue
                code = production_ts(p.read_text(encoding="utf8", errors="replace"))
                if any(contains_word(code, s) for s in exports):
                    others.append(rel(p))
        if others:
            print(f"      非生产调用方（测试 / e2e / 夹具链）{len(others)} 处：{others}")


def creation_paths() -> None:
    print("\n═══ `daemon_kill.rs::CREATION_PATHS` 现打：谁的生产段真在产 `tmux new-session`")
    verb = "new-" + "session"
    wide, argv = f"tmux {verb}", f'"{verb}", "-d"'
    found, scanned = set(), 0
    for base in ("src-tauri/src", "remote-daemon-proto/src", "src", "shared"):
        d = ROOT / base
        if not d.is_dir():
            continue
        for p in sorted(d.rglob("*")):
            if p.is_dir() or ".test." in p.name or ".vitest." in p.name:
                continue
            if p.suffix not in (".rs", ".ts", ".sh", ""):
                continue
            try:
                raw = p.read_text(encoding="utf8")
            except (UnicodeDecodeError, OSError):
                continue
            scanned += 1
            body = production_rs(raw) if p.suffix == ".rs" else raw
            for line in body.split("\n"):
                t = line.lstrip()
                if t.startswith(("//", "#", "*")):
                    continue
                if wide in line or argv in line:
                    found.add(rel(p))
                    break
    assert scanned >= 300, f"只扫到 {scanned} 个文件 —— 遍历坏了"
    print(f"  扫了 {scanned} 个文件 ⇒ **{len(found)} 条**：")
    for f in sorted(found):
        print(f"      {f}")


def three_questions() -> None:
    print("\n═══ `INVARIANTS §33b` 三问的现场量法")
    mode = '"create-or-' + 'attach"'
    word = "create-or-" + "attach"
    monitor, scanned = [], 0
    for p in sorted((ROOT / "src-tauri/src").rglob("*.rs")):
        if p.name == "launch_wire.rs":
            continue
        scanned += 1
        if mode in production_rs(p.read_text(encoding="utf8")):
            monitor.append(rel(p))
    assert scanned >= 50, f"只扫到 {scanned} 个 monitor `.rs`"
    ccm = "\n".join(
        production_rs((ROOT / "remote-daemon-proto/src/control/ccm" / f).read_text(encoding="utf8"))
        for f in ("mod.rs", "argv.rs", "plan.rs")
    )
    ccm_hits = len(re.findall(r"(?<![A-Za-z0-9_-])" + re.escape(word) + r"(?![A-Za-z0-9_-])", ccm))
    print(f"  ① monitor 树发 `{word}`：{len(monitor)} 处 {monitor}")
    print(f"     `control/ccm/` 发：{ccm_hits} 处")
    launch = production_rs((ROOT / "remote-daemon-proto/src/control/launch.rs").read_text(encoding="utf8"))
    print(f"     daemon 侧承接方 `Mode::CreateOrAttach` 分支在：{'Mode::CreateOrAttach =>' in launch}")

    seat_attach = "SESSION_BACKEND." + "attach"
    askers = []
    for p in prod_ts_files():
        if p.name == "session-backend.ts":
            continue
        code = production_ts(p.read_text(encoding="utf8"))
        for i, line in enumerate(code.split("\n"), 1):
            if seat_attach in line:
                askers.append(f"{rel(p)}:{i}")
    print(f"  ② 生产 TS 里还问座要 attach 的：{len(askers)} 处 {askers}")

    carriers = {
        "REMOTE_HOST_FIELDS 那一项": '"daemonless",'
        in production_ts((ROOT / "src/remote-config.ts").read_text(encoding="utf8")),
        "机器卡片那个 input": "daemonlessInput"
        in production_ts((ROOT / "src/settings/machine-card.ts").read_text(encoding="utf8")),
        "数据源那条轮询回落": "daemonless_stream_loop"
        in production_rs((ROOT / "src-tauri/src/ssh_source.rs").read_text(encoding="utf8")),
    }
    print(f"  ③ daemonless 那一档的三个载体：{carriers}")


def copies() -> None:
    print("\n═══ 「N 个生产消费者」那句话在盘上的副本（`K-R105` 要一次找全的那个人群）")
    pat = re.compile(r"个\*{0,2}生产消费者|生产消费者")
    hit = 0
    for base in ("src", "src-tauri/src", "doc", "remote-daemon-proto/src"):
        d = ROOT / base
        if not d.is_dir():
            continue
        for p in sorted(d.rglob("*")):
            if p.is_dir() or p.suffix not in (".ts", ".rs", ".md"):
                continue
            try:
                txt = p.read_text(encoding="utf8")
            except (UnicodeDecodeError, OSError):
                continue
            for i, line in enumerate(txt.split("\n"), 1):
                if pat.search(line):
                    hit += 1
                    print(f"      {rel(p)}:{i}  {line.strip()[:100]}")
    print(f"  合计 {hit} 处")


if __name__ == "__main__":
    print(f"ROOT = {ROOT}")
    self_check()
    ruler_a()
    ruler_b()
    creation_paths()
    three_questions()
    copies()
