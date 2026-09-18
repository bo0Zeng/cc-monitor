#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`K-R120` 的量具 —— 七处版本号 ＋ `CHANGELOG.md` 最上一节，**现算，不抄**。

## 它答两问

    --places     「七处现在各是多少」。🔴 **六处不自己数** —— 转调同一棵树里的
                 `evidence/K-R118-ruler.py --places`（它解析
                 `doc_claim_registry::the_release_version_is_the_same_in_all_six_places`
                 的函数体，「六」= 那条判据里 `pick()` 的调用点数）。
                 本量具只补**第七处** `src-tauri/Cargo.lock` 的 monitor version，
                 以及一句「七处一不一致」的裁决。
    --changelog  「`CHANGELOG.md` 最上面那一节」的读数：节标题逐字 · 抠出来的版本号 ·
                 它与 `src-tauri/Cargo.toml` 的 version 相不相等 · 那一节的子标题顺序 ·
                 breaking 段排第几 · 全文里带约定词的 `###` 标题有几处。

## ⚠ 它买得到什么、买不到什么（写死，别读宽）

**买得到**
  · `--places` 的六处**跟着判据走**（它是转调，不是第二份手抄表）；第七处逐字给锚点。
  · `--changelog` 用的段界读法与那条新判据
    （`doc_claim_registry::the_changelog_top_section_is_the_version_we_ship`）**同形**：
    最上面那一行 `## [` 打头的标题 ＋ 到下一条 `## ` 之前的正文。
  · 每一处都带**锚点命中数**；命中不是 1 一律当场点名，不静默取第一个。

**买不到**
  · 它**不是判据**，不进门禁、不 `exit 1` 报违例（除非自己解析不动 / 转调失败）。
    守这几件事的是 `winchk` 那一格的 `cargo check --locked`、那条「六处一致」的 `#[test]`、
    以及本轮新加的那条 `the_changelog_top_section_is_the_version_we_ship`。
  · 它**不判 CHANGELOG 里写的内容对不对**（六条是不是真六条、有没有漏）——
    那一层是人裁的，`K-R118` 交回时逐字写过「**用户可见**那一层给不出判别式」。
  · `--changelog` 认 breaking 段靠的是一个**约定词**（与那条判据同一个词，见下面的
    `BREAKING_MARK`）。换一种说法另起一节 ⇒ 本量具与那条判据**一起**看不见。

## 跑法（从工作树根起跑）

    python3 evidence/K-R120-ruler.py --places
    python3 evidence/K-R120-ruler.py --changelog
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SIX_RULER = ROOT / "evidence" / "K-R118-ruler.py"
LOCK = "src-tauri/Cargo.lock"
CARGO = "src-tauri/Cargo.toml"
CHANGELOG = "CHANGELOG.md"

# 与 `doc_claim_registry` 那条新判据里的 `BREAKING_MARK` 是**同一个词**。
# ⚠ 这里是第二份字面量，写下来是因为量具与判据是两棵树上的东西（py / rs）——
#    真相源是判据那一份；本量具**从判据里读它**，下面那行只是读不到时的兜底并会出声。
BREAKING_MARK_FALLBACK = "会改变已有行为"
REGISTRY = "src-tauri/src/doc_claim_registry.rs"


def breaking_mark() -> "tuple[str, str]":
    """从判据里读那个约定词 —— 读不到就退回兜底值，**并把这件事印出来**。"""
    text = (ROOT / REGISTRY).read_text(encoding="utf-8")
    m = re.search(r'const BREAKING_MARK: &str = "([^"]+)";', text)
    if m:
        return m.group(1), f"从 `{REGISTRY}::BREAKING_MARK` 读到"
    return BREAKING_MARK_FALLBACK, "🔴 判据里那个常量抠不到，退回本量具的兜底值（真相源变了，先修这里）"


def lock_version() -> "tuple[str | None, int]":
    """第七处：`Cargo.lock` 里 `name = "monitor"` 紧跟的 `version`。"""
    text = (ROOT / LOCK).read_text(encoding="utf-8")
    needle = 'name = "monitor"\nversion = "'
    hits = text.count(needle)
    if hits != 1:
        return None, hits
    at = text.index(needle) + len(needle)
    return text[at:text.index('"', at)], hits


def cargo_toml_version() -> "tuple[str | None, int]":
    """`src-tauri/Cargo.toml` 的 `version` —— 那条新判据用的就是它（编译期 `CARGO_PKG_VERSION`）。"""
    text = (ROOT / CARGO).read_text(encoding="utf-8")
    needle = '\nversion = "'
    hits = text.count(needle)
    if hits != 1:
        return None, hits
    at = text.index(needle) + len(needle)
    return text[at:text.index('"', at)], hits


def places() -> int:
    if not SIX_RULER.exists():
        print(f"🔴 转调不动：`{SIX_RULER.relative_to(ROOT)}` 不在盘上")
        return 3
    r = subprocess.run([sys.executable, str(SIX_RULER), "--places"],
                       cwd=str(ROOT), capture_output=True, text=True, encoding="utf-8")
    print("# ── 六处：转调 `evidence/K-R118-ruler.py --places`（它从判据的函数体里读）──")
    print(r.stdout.rstrip())
    if r.returncode != 0:
        print(f"🔴 转调退出码 {r.returncode}；stderr：\n{r.stderr.rstrip()}")
        return 3
    six = sorted(set(re.findall(r"\|\s*(\d+\.\d+\.\d+)\s*\|\s*1\s*\|", r.stdout)))
    lock_v, lock_hits = lock_version()
    print()
    print("# ── 第七处（判据够不着，`winchk` 的 `cargo check --locked` 与 "
          "`release.yml` 的四处对账在守）──")
    print("| # | 文件 | 逐字锚点 | 现值 | 命中 |")
    print("|---|---|---|---|---|")
    print(f"| 7 | `{LOCK}` | `name = \"monitor\"\\nversion = \"` | {lock_v} | {lock_hits} |")
    print()
    allv = sorted(set(six) | ({lock_v} if lock_v else set()))
    verdict = "（七处一致）" if len(allv) == 1 else " 🔴 **七处不一致**"
    print(f"现打：六处取到 {six} · 第七处 {lock_v} ⇒ 合起来 {len(allv)} 个不同的值 {allv}{verdict}")
    return 0


def changelog() -> int:
    mark, how = breaking_mark()
    text = (ROOT / CHANGELOG).read_text(encoding="utf-8")
    lines = text.split("\n")
    TOP, SECTION, SUB = "## ", "## [", "### "

    at = next((i for i, l in enumerate(lines) if l.startswith(SECTION)), None)
    if at is None:
        print(f"🔴 `{CHANGELOG}` 里一行 `{SECTION}…` 都找不到 —— 段界读法坏了")
        return 3
    heading = lines[at]
    m = re.match(r"^#+\s*\[([^\]]*)\]", heading)
    top_v = m.group(1) if m else None

    end = next((i for i in range(at + 1, len(lines)) if lines[i].startswith(TOP)), len(lines))
    body = lines[at + 1:end]
    subs = [l for l in body if l.startswith(SUB)]
    pos = next((i for i, l in enumerate(subs) if mark in l), None)
    floor_hits = sum(1 for l in lines if l.startswith(SUB) and mark in l)
    cargo_v, cargo_hits = cargo_toml_version()

    print(f"# `{CHANGELOG}` 最上面那一节 —— 段界读法与判据 "
          f"`doc_claim_registry::the_changelog_top_section_is_the_version_we_ship` 同形")
    print(f"# breaking 段的约定词：{mark!r}（{how}）")
    print()
    print(f"- 节标题逐字：`{heading}`（在第 {at + 1} 行）")
    print(f"- 抠出来的版本号：**{top_v}**")
    print(f"- `{CARGO}` 的 version：**{cargo_v}**（锚点命中 {cargo_hits} 次）"
          f" —— 判据比的就是这两个")
    print(f"- 相等吗：{'✅ 相等' if top_v == cargo_v else '🔴 **不相等**'}")
    print(f"- 那一节的正文 {len(body)} 行 · 子标题 {len(subs)} 个，顺序是：")
    for i, l in enumerate(subs, 1):
        tag = "  ← breaking 段" if mark in l else ""
        print(f"    {i}. {l}{tag}")
    if pos is None:
        print(f"- breaking 段：**这一节里没有**（判据对这一形静默 —— "
              f"「该不该有」是人裁的，登记为不在射程）")
    else:
        print(f"- breaking 段排第 **{pos + 1}** 个"
              f"{'（在最前 ✅）' if pos == 0 else ' 🔴 **不是第一个**'}")
    print(f"- 地板：全文里带 {mark!r} 的 `{SUB}` 标题 **{floor_hits}** 处"
          f"（分母 = `{CHANGELOG}` 全部 {len(lines)} 行；判据只要求 ≥ 1）")
    return 0


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    if sys.argv[1] == "--places":
        return places()
    if sys.argv[1] == "--changelog":
        return changelog()
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
