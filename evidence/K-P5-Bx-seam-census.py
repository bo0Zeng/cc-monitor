#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-P5 `Bx` 摸底的量具 —— **接缝在哪** 的三把尺子。

住址：`evidence/K-P5-Bx-seam-census.py`（**只属于 K-P5 这一拍**，别的角色别复用这个名字）。
被测对象：**本文件所在的那棵工作树**（`Path(__file__).resolve().parents[1]`）——
不是主工作树、也不是别的 worktree。跑之前先看它印出来的第一行「被测树」与「量于哪个提交」。

跑法：`python3 evidence/K-P5-Bx-seam-census.py`（零依赖，只读，不改盘上任何东西）。

---------------------------------------------------------------------------
🔴 **每把尺子先声明它枚举的到底是什么** —— 这是 `K-R6` 那一拍裁完的那个病
   （量具枚举的集合 ≠ 标签命名的集合）的处置。三把尺子各有一段 `WHAT_IT_ENUMERATES`，
   打印在它自己的读数前面。别把任何一个数读成「身份落点有 N 处」。
---------------------------------------------------------------------------

尺子 1（`needles`）：**复打 PM 现打的那一格**（三个针 `ccm-rbind` / `set-titles` / `@ccm_sid`），
  并把同一把针在**四个不同作用域**上各量一次 —— 用来证明「105 / 16」是一个**作用域数**，
  不是「身份落点」的人群。

尺子 2（`launch_capability`）：**机器可枚举**的那一半「起会话方」——
  `src-tauri/src/parity_ledger.rs::LEDGER` 里 capability 恰好等于 `session.launch` 的行。
  它枚举的是「**在平价账本里被登记成 `session.launch` 的 tauri 命令**」，
  **不是**「所有会让一个 agent 进程出生的地方」。差在哪由尺子 3 点名。

尺子 3（`carriers`）：**身份载体普查**。人群口径写在 `CARRIERS` 里，逐条给：
  载体名 · 谁写 · 谁读 · 本机/跨机器 · 生产段命中处数（去注释、去 `#[cfg(test)] mod tests` 之后）。
  它枚举的是「**这几个写死的字面针在生产段的出现处**」，
  **不是**「所有与身份有关的代码」——后者没有可判定的谓词，我做不出来。
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def sh(*args: str) -> str:
    return subprocess.run(
        args, cwd=ROOT, capture_output=True, text=True, check=False
    ).stdout.strip()


def tracked_files() -> list[Path]:
    out = sh("git", "ls-files", "-z")
    return [ROOT / p for p in out.split("\0") if p]


# ═══════════════════════════════════════════════════════════════════════════
# 剥法（说清它做得到什么、做不到什么 —— 铁律 6：docstring 说的话也要核）
# ═══════════════════════════════════════════════════════════════════════════

_RS_TESTS = re.compile(r"^#\[cfg\(test\)\]\s*$", re.M)


def strip_rust(text: str) -> str:
    """去掉 Rust 的 `//`/`///`/`//!` 行注释与**第一处顶格 `#[cfg(test)]` 之后的全部内容**。

    ⚠ 它**做不到**：块注释 `/* … */`（除非那几行以 `*` 起头）· 行尾注释（`x(); // 说明`
    只会保留整行）· 测试模块**不在文件末尾**的那些文件。
    ⇒ 这把尺子给的是「生产段的**上界**」，不是精确值。报数时一律带这句话。
    """
    m = _RS_TESTS.search(text)
    if m:
        text = text[: m.start()]
    out = []
    for line in text.splitlines():
        t = line.lstrip()
        if t.startswith("//") or t.startswith("*") or t.startswith("/*"):
            continue
        out.append(line)
    return "\n".join(out)


def strip_sh(text: str) -> str:
    """去掉 shell / PowerShell 的整行 `#` 注释（含 shebang）。行尾注释保留。"""
    return "\n".join(l for l in text.splitlines() if not l.lstrip().startswith("#"))


def strip_for(path: Path, text: str) -> str:
    if path.suffix == ".rs":
        return strip_rust(text)
    if path.suffix in (".sh", ".tpl", ".ps1") or path.name in ("ccm", "ccm-aliases.sh"):
        return strip_sh(text)
    return text


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def is_test_file(rel: str) -> bool:
    """判据 / 测试 / e2e / 文档 / 量具 —— **不是生产段**。

    ⚠ 这条谓词按**文件名与目录名**认，不按内容认：`.test.ts` · `.vitest.ts` ·
    `e2e/` · `doc/` · `evidence/` · `*.md` · Rust 的 `*_parity.rs`/`*_registry.rs`
    /`*_guard.rs`/`*_contract.rs`（本仓的判据文件命名族）。
    ⇒ 它会**误伤**那几个 `_registry.rs` 里真正的生产函数（本仓的判据与生产同住一文件是常态），
      所以下面**两个数都印**：含判据文件的、与去掉之后的。
    """
    if rel.endswith((".md", ".test.ts", ".vitest.ts", ".tsv")):
        return True
    if rel.startswith(("e2e/", "doc/", "evidence/")):
        return True
    return rel.endswith(
        ("_parity.rs", "_registry.rs", "_guard.rs", "_contract.rs")
    )


# ═══════════════════════════════════════════════════════════════════════════
# 尺子 1：三个针 × 四个作用域
# ═══════════════════════════════════════════════════════════════════════════

NEEDLES = ("ccm-rbind", "set-titles", "@ccm_sid")

SCOPES: dict[str, callable] = {
    "PM 那一格（src-tauri/src，只 .rs）": lambda p: (
        "src-tauri/src/" in p.as_posix() and p.suffix == ".rs"
    ),
    "src-tauri/src 全部文件（含夹具）": lambda p: "src-tauri/src/" in p.as_posix(),
    "全仓跟踪文件": lambda p: True,
    "全仓、去注释去测试段之后": lambda p: True,  # 特殊：见下
}


def count_needles(files: list[Path]) -> None:
    print("=" * 78)
    print("尺子 1 · 三个针的并集")
    print("  ▸ 它枚举的是：**三个写死的字面串在文本里的出现行数**。")
    print("    针 =", NEEDLES)
    print("  ▸ 它**不是**「身份落点」的人群 —— 三个针里 `set-titles` 与 `ccm-rbind`")
    print("    都只长在 tmux/窗口标题那条路上，而身份还住在别的载体里（尺子 3）。")
    print("=" * 78)

    for label, pred in SCOPES.items():
        strip = label.endswith("去注释去测试段之后")
        hits = 0
        per_file: dict[str, int] = {}
        for f in files:
            if not pred(f.relative_to(ROOT)):
                continue
            txt = read(f)
            if strip:
                txt = strip_for(f, txt)
            n = sum(1 for line in txt.splitlines() if any(k in line for k in NEEDLES))
            if n:
                per_file[f.relative_to(ROOT).as_posix()] = n
                hits += n
        print(f"\n  【{label}】 {hits} 处 / {len(per_file)} 份文件")
        for name, n in sorted(per_file.items(), key=lambda kv: -kv[1])[:12]:
            print(f"      {n:>4}  {name}")
        if len(per_file) > 12:
            print(f"      … 另有 {len(per_file) - 12} 份，各 ≤ "
                  f"{sorted(per_file.values(), reverse=True)[12]} 处")


# ═══════════════════════════════════════════════════════════════════════════
# 尺子 2：`session.launch` 能力（机器可枚举的那一半）
# ═══════════════════════════════════════════════════════════════════════════

LEDGER_ROW = re.compile(r'^\s*\("([a-z_0-9]+)",\s*"([a-z0-9.\-]+)",\s*Side::(\w+)\)', re.M)


def launch_capability() -> None:
    print("\n" + "=" * 78)
    print("尺子 2 · 账本里 capability == `session.launch` 的行")
    print("  ▸ 它枚举的是：**`parity_ledger.rs::LEDGER` 里那张表的行**（正则抠三元组）。")
    print("  ▸ 它**不是**「所有起会话方」—— 账本只登记 **tauri 命令**；")
    print("    终端那两条起会话方（`shared/ccm` · `cc.ps1.tpl` 里的 `function cc`）")
    print("    **不是 tauri 命令，所以结构上进不了这张表**。")
    print("=" * 78)
    txt = read(ROOT / "src-tauri/src/parity_ledger.rs")
    rows = LEDGER_ROW.findall(txt)
    # 🔴 自查：本正则**只认单行三元组** ⇒ 它抠到的数是**下界**，不是账本大小。
    #    仓里自己的断言是 `assert_eq!(LEDGER.len(), 145)`；裸行首 `("` 现打 141 行。
    #    ⇒ 下面那个数**不许当成「账本有多少条命令」用**，它只用来说明这把尺子的作用域。
    raw_open = sum(1 for l in txt.splitlines() if l.lstrip().startswith('("'))
    declared = re.search(r"assert_eq!\(LEDGER\.len\(\),\s*(\d+)", txt)
    print(f"\n  ⚠ 本正则抠到的三元组：{len(rows)}（**只认单行形态** ⇒ 下界）")
    print(f"     行首裸 `(\"` 现打：{raw_open}；仓里自己的断言："
          f"LEDGER.len() == {declared.group(1) if declared else '?'}")
    print("     ⇒ 三个数不相等**不是漂移，是三把尺子作用域不同**。别拿本行的数当账本大小。")
    hit = [r for r in rows if r[1] == "session.launch"]
    # 非空对照：直接按字面串数一遍，两条路必须给同一个答案。
    direct = sum(1 for l in txt.splitlines() if '"session.launch"' in l)
    print(f"\n  capability == 'session.launch'：{len(hit)} 行"
          f"（字面串直数对照：{direct} —— 两条路{'一致' if direct == len(hit) else '**不一致，先修尺子**'}）")
    for name, cap, side in hit:
        print(f"      {side:<7} {name}")
    others = sorted({r[1] for r in rows if r[1].startswith("launch.")})
    print(f"\n  ⚠ 另有 {len(others)} 个 `launch.*` 能力（**渲染**，不是起会话）：{others}")


# ═══════════════════════════════════════════════════════════════════════════
# 尺子 3：身份载体普查
# ═══════════════════════════════════════════════════════════════════════════

# 每条：(载体名, 本机/跨机器/两者, 针列表, 只在这些路径下数)
CARRIERS: list[tuple[str, str, tuple[str, ...], tuple[str, ...]]] = [
    # ⚠ C1 用**正则**（`re:` 前缀）：`@ccm_sid` 后面不许紧跟 `_`，否则会把 C2 的
    #   `@ccm_sid_expect` 一起数进来。第一版用的是一组字面针（`@ccm_sid ` / `"@ccm_sid"` / …），
    #   它**漏掉了 tmux 格式串里的 `#{@ccm_sid}`**（`tmux.rs:22` 的 `TMUX_LS_FMT` 正是这一形）
    #   ⇒ 那一版的 38 是**漏数**。留下这条注释，别再回到字面针。
    ("C1 `@ccm_sid`（tmux 会话级 option · 事实通道 B）", "跨机器",
     ("re:@ccm_sid(?!_)",),
     ("src-tauri/src/", "remote-daemon-proto/src/", "shared/", "src/")),
    ("C2 `@ccm_sid_expect`（tmux 会话级 option · 意图通道 A）", "跨机器",
     ("@ccm_sid_expect",),
     ("src-tauri/src/", "remote-daemon-proto/src/", "shared/", "src/")),
    ("C3 窗口标题 marker `ccm-rbind-<sid>`", "跨机器",
     ("ccm-rbind",),
     ("src-tauri/src/", "remote-daemon-proto/src/", "shared/", "src/")),
    ("C4 窗口标题 marker `ccm-bind-<PID>-<nonce>`（Windows 本机握手）", "本机",
     ("ccm-bind-", "ps-await", "ps-registry"),
     ("src-tauri/src/", "src-tauri/scripts/", "src/")),
    ("C5 pidfile `sessions/<pid>.json` 的 sessionId（claude 自己写）", "两者",
     ("sessions/", "parse_session_id", "sessionId"),
     ("remote-daemon-proto/src/", "src-tauri/src/session_map.rs")),
    ("C6 `/proc/<pid>/environ` 的 `TMUX_PANE`（tmux 写 · daemon 读）", "跨机器",
     ("TMUX_PANE",),
     ("remote-daemon-proto/src/", "shared/", "src-tauri/src/")),
    ("C7 `CC_BUS_ID`（**已在环境里的会话身份** · ccm 写 · cc-bus 读）", "两者",
     ("CC_BUS_ID",),
     ("shared/", "src-tauri/src/", "src/")),
    ("C8 `ANTHROPIC_BASE_URL` 路由键的 `<account>/<key>` 段（K-H2b 现成的注入）", "本机",
     ("ANTHROPIC_BASE_URL", "route_key_for_session", "relay_route_path"),
     ("src-tauri/src/", "remote-daemon-proto/src/", "shared/")),
    ("C9 tmux 会话名里编码的 sid8（`cc-<sid8>` / `mintTmuxName`）", "两者",
     ("mintTmuxName", "pickFreshTmuxName"),
     ("src/", "src-tauri/src/")),
]


def carriers(files: list[Path]) -> None:
    print("\n" + "=" * 78)
    print("尺子 3 · 身份载体普查")
    print("  ▸ **我给「一处身份落点」下的口径**（这是我自己定的，不是仓里现成的）：")
    print("    一处身份落点 = 生产代码里**把「这条会话是谁」这个事实写进某个载体、")
    print("    或从那个载体里读回来**的一处。人群按**载体**切，不按主题词切 ——")
    print("    因为本件问的正是「身份该住哪个载体」。")
    print("  ▸ 每条的数字枚举的是：**该载体的字面针在生产段（去行注释、去第一处顶格")
    print("    `#[cfg(test)]` 之后）出现的行数**。它是**上界**（剥法的边界见 `strip_rust`）。")
    print("  ▸ 它**不枚举**「所有与身份有关的代码」—— 那没有可判定的谓词。")
    print("=" * 78)
    for name, machine, needles, prefixes in CARRIERS:
        total = 0
        per: dict[str, int] = {}
        prod: dict[str, int] = {}
        # 针分两类：`re:` 前缀 = 正则；其余 = 字面子串。**两类都印在 CARRIERS 里，可复核。**
        pats = [re.compile(k[3:]) for k in needles if k.startswith("re:")]
        lits = [k for k in needles if not k.startswith("re:")]

        def hit(line: str) -> bool:
            return any(k in line for k in lits) or any(p.search(line) for p in pats)

        for f in files:
            rel = f.relative_to(ROOT).as_posix()
            if not any(rel.startswith(p) for p in prefixes):
                continue
            txt = strip_for(f, read(f))
            n = sum(1 for line in txt.splitlines() if hit(line))
            if n:
                per[rel] = n
                total += n
                if not is_test_file(rel):
                    prod[rel] = n
        print(f"\n  {name}")
        print(f"    面：{machine}")
        print(f"    含判据/测试文件：{total} 处 / {len(per)} 份")
        print(f"    **去判据/测试文件**：{sum(prod.values())} 处 / {len(prod)} 份")
        for rel, n in sorted(prod.items(), key=lambda kv: -kv[1])[:6]:
            print(f"        {n:>4}  {rel}")
        if len(prod) > 6:
            print(f"        … 另有 {len(prod) - 6} 份")

    print("\n  ── 按面小计（**本机与跨机器分开数**，`§2` 要的那一刀）──")
    for want in ("本机", "跨机器", "两者"):
        names = [c[0].split("（")[0] for c in CARRIERS if c[1] == want]
        print(f"    {want}：{len(names)} 个载体 — {names}")


def main() -> int:
    head = sh("git", "rev-parse", "--short", "HEAD")
    branch = sh("git", "rev-parse", "--abbrev-ref", "HEAD")
    print(f"被测树：{ROOT}")
    print(f"量于：{branch} @ {head}")
    files = tracked_files()
    print(f"跟踪文件分母：{len(files)} 份\n")
    count_needles(files)
    launch_capability()
    carriers(files)
    return 0


if __name__ == "__main__":
    sys.exit(main())
