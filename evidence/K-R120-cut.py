#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`K-R120` 死值验的刀 —— 每一刀先断言锚点命中数，再落刀，再印「变异已落地」。

## 纪律（照 `references/brief.md` 第 7 · 11 · 12c 条，与 `K-R118-cut.py` 同形）

- 🔴 **还原不许 `copy2` / `cp -a`**：`--revert` 是**重写原文**（`write_text` ＋ `os.utime`），
  mtime 必变 ⇒ 下一趟 `cargo` / `tsc` 一定重算。门禁 `copy2` 那一格正在数这件事，
  本文件自己不许犯它。
- 每一刀落刀前 `assert 锚点命中 == want`，对不上**一个字节都不改**、整趟放弃。
- 一趟只许有一把刀在盘上：`--apply` 之前若备份还在，拒绝落刀。
- 🔴 **刀刻意不写死目标版本号**：全部按「补丁位 +1」造**形状**，与「这次发几号」无关。

## 刀

    d1   `KR120D1` ① —— 七处里**只漏 `Cargo.lock`**（把它按补丁位 -1 退回上一档，
                       另外六处不动）⇒ 门禁 `winchk`（`cargo check --locked`）必须红。
                       **这是已知答案的回测**（`K-R118` `§0a` 就是这么判的）。
    d2   `KR120D1` ② —— 「只改五处」那一形：**只把权威源 `package.json` 再 bump 一档**，
                       另外五处不动 ⇒ `the_release_version_is_the_same_in_all_six_places`
                       必须红。锚点与 `K-R118` 的 `d5` **逐字相同**。
    d3   `KR120D2` ① —— **复刻 `K-R118` 的 `d7`**：七处一起 bump 一档、
                       `CHANGELOG.md` 一个字不动 ⇒ 本轮新加的那条
                       `the_changelog_top_section_is_the_version_we_ship` 必须红。
                       （`K-R118` 实打那一趟：16 格全绿、一条没红 —— 那就是本条存在的证据。）
    d4   `KR120D2` ② —— 最上面那一节**有，但没有 breaking 段**：把那一节的 breaking
                       子标题连同它的正文整块删掉 ⇒ 问「有没有东西红」。
                       **这一刀的答案本身就是读数**：不红 ⇒ 如实登记「这一形不在射程」。
    d5   `KR120D2` ②b —— breaking 段**被埋在列表里**：把最上面那一节的头两个 `###`
                       整块对调 ⇒ 新判据第二条判定必须红。
    d6   `KR120D2` ③ —— 阴性对照：把新那道闸**整个拿掉**（`#[ignore]`）＋ 刀 `d3`
                       ⇒ 一条都不红。

## 跑法（从工作树根起跑）

    python3 evidence/K-R120-cut.py --list
    python3 evidence/K-R120-cut.py --apply <刀名>
    python3 evidence/K-R120-cut.py --revert
"""

import json
import os
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BACKUP = ROOT / ".k-r120-cut-backup.json"

PKG = "package.json"
CARGO = "src-tauri/Cargo.toml"
CONF = "src-tauri/tauri.conf.json"
README = "README.md"
README_EN = "README.en.md"
LOCK = "src-tauri/Cargo.lock"
CHANGELOG = "CHANGELOG.md"
REGISTRY = "src-tauri/src/doc_claim_registry.rs"

TOUCHED = [PKG, CARGO, CONF, README, README_EN, LOCK, CHANGELOG, REGISTRY]

SUB = "### "
TOP = "## "
SECTION = "## ["
# 与判据里 `BREAKING_MARK` 同一个词 —— 🔴 **从判据里读**，不在本文件写死。
MARK_DECL = 'const BREAKING_MARK: &str = "'

GATE_FN = "    fn the_changelog_top_section_is_the_version_we_ship() {"
GATE_ATTR = "    #[test]\n" + GATE_FN


def breaking_mark(files) -> str:
    text = files[REGISTRY]
    n = text.count(MARK_DECL)
    if n != 1:
        raise SystemExit(f"🔴 拒绝落刀：`{REGISTRY}` 里 `BREAKING_MARK` 声明命中 {n} 次，应 1 次")
    at = text.index(MARK_DECL) + len(MARK_DECL)
    return text[at:text.index('"', at)]


def _patch(cur: str, delta: int) -> str:
    parts = cur.split(".")
    if len(parts) != 3:
        raise SystemExit(f"🔴 拒绝落刀：现值 {cur!r} 形状不像 X.Y.Z")
    return f"{parts[0]}.{parts[1]}.{int(parts[2]) + delta}"


def _current(files) -> str:
    """当前版本号 = 权威源 `package.json` 那一处。"""
    needle = '\n  "version": "'
    text = files[PKG]
    n = text.count(needle)
    if n != 1:
        raise SystemExit(f"🔴 拒绝落刀：`{PKG}` 里锚点命中 {n} 次，应当 1 次")
    at = text.index(needle) + len(needle)
    return text[at:text.index('"', at)]


def _edits(cur: str, nxt: str):
    """七处的锚点表 —— 与 `K-R118-cut.py::bump_all_but_changelog` 逐字同形。"""
    return [
        (PKG, f'\n  "version": "{cur}"', f'\n  "version": "{nxt}"'),
        (CARGO, f'\nversion = "{cur}"', f'\nversion = "{nxt}"'),
        (CONF, f'\n  "version": "{cur}"', f'\n  "version": "{nxt}"'),
        (README, f"当前版本: v{cur}", f"当前版本: v{nxt}"),
        (README, f"- **版本**：v{cur}", f"- **版本**：v{nxt}"),
        (README_EN, f"| Current: v{cur}", f"| Current: v{nxt}"),
        (LOCK, f'name = "monitor"\nversion = "{cur}"', f'name = "monitor"\nversion = "{nxt}"'),
    ]


def _apply_edits(files, edits, label):
    for path, old, new in edits:
        n = files[path].count(old)
        if n != 1:
            raise SystemExit(f"🔴 拒绝落刀：`{path}` 里锚点 {old!r} 命中 {n} 次，应当 1 次")
    for path, old, new in edits:
        files[path] = files[path].replace(old, new)
    return f"{label}：{len(edits)} 处锚点各命中 1 次"


# ── d1：只漏 Cargo.lock ──────────────────────────────────────────────────────
def lock_falls_behind(files):
    cur = _current(files)
    prev = _patch(cur, -1)
    old = f'name = "monitor"\nversion = "{cur}"'
    new = f'name = "monitor"\nversion = "{prev}"'
    note = _apply_edits(files, [(LOCK, old, new)], f"`Cargo.lock` 的 monitor {cur} → {prev}")
    return note + f"；另外六处仍是 {cur}，一个字节没动"


# ── d2：只改五处（＝ 权威源单独往前一档）────────────────────────────────────
def authority_runs_ahead(files):
    cur = _current(files)
    nxt = _patch(cur, +1)
    note = _apply_edits(files, [(PKG, f'\n  "version": "{cur}"', f'\n  "version": "{nxt}"')],
                        f"权威源 `{PKG}` {cur} → {nxt}")
    return note + f"；另外五处仍是 {cur} ⇒ 「六处一致」应当逐处点名它们"


# ── d3：复刻 K-R118 的 d7 ───────────────────────────────────────────────────
def bump_all_but_changelog(files):
    cur = _current(files)
    nxt = _patch(cur, +1)
    note = _apply_edits(files, _edits(cur, nxt), f"七处一起 {cur} → {nxt}")
    return note + f"；`{CHANGELOG}` 一个字节没动（最上一节仍是 {cur}）"


# ── CHANGELOG 段界（与判据同形）──────────────────────────────────────────────
def _top_section(text):
    lines = text.split("\n")
    at = next((i for i, l in enumerate(lines) if l.startswith(SECTION)), None)
    if at is None:
        raise SystemExit(f"🔴 拒绝落刀：`{CHANGELOG}` 里找不到 `{SECTION}…` 标题")
    end = next((i for i in range(at + 1, len(lines)) if lines[i].startswith(TOP)), len(lines))
    return lines, at, end


def _sub_spans(lines, at, end):
    """那一节里每个 `### ` 块的 [起, 止) 区间。"""
    heads = [i for i in range(at + 1, end) if lines[i].startswith(SUB)]
    spans = []
    for k, h in enumerate(heads):
        stop = heads[k + 1] if k + 1 < len(heads) else end
        spans.append((h, stop))
    return spans


# ── d4：那一节里没有 breaking 段 ────────────────────────────────────────────
def drop_breaking_block(files):
    mark = breaking_mark(files)
    lines, at, end = _top_section(files[CHANGELOG])
    spans = _sub_spans(lines, at, end)
    hit = [s for s in spans if mark in lines[s[0]]]
    if len(hit) != 1:
        raise SystemExit(f"🔴 拒绝落刀：最上面那一节里带 {mark!r} 的 `{SUB}` 块有 {len(hit)} 个，应 1 个")
    lo, hi = hit[0]
    files[CHANGELOG] = "\n".join(lines[:lo] + lines[hi:])
    return (f"删掉最上面那一节里带 {mark!r} 的那一整块（第 {lo + 1}–{hi} 行，{hi - lo} 行）；"
            f"锚点命中 1 次")


# ── d5：breaking 段被埋在列表里 ─────────────────────────────────────────────
def bury_breaking_block(files):
    mark = breaking_mark(files)
    lines, at, end = _top_section(files[CHANGELOG])
    spans = _sub_spans(lines, at, end)
    if len(spans) < 2:
        raise SystemExit(f"🔴 拒绝落刀：最上面那一节只有 {len(spans)} 个 `{SUB}` 块，对调不了")
    if mark not in lines[spans[0][0]]:
        raise SystemExit(f"🔴 拒绝落刀：头一个 `{SUB}` 块不带 {mark!r}，本刀要打的那一形不在盘上")
    (a0, a1), (b0, b1) = spans[0], spans[1]
    if a1 != b0:
        raise SystemExit("🔴 拒绝落刀：头两块不相邻，段界读法坏了")
    files[CHANGELOG] = "\n".join(lines[:a0] + lines[b0:b1] + lines[a0:a1] + lines[b1:])
    return (f"把最上面那一节的头两个 `{SUB}` 块整块对调："
            f"`{lines[a0]}`（{a1 - a0} 行）↔ `{lines[b0]}`（{b1 - b0} 行）；锚点命中 1 次")


# ── d6：把新那道闸整个拿掉 ＋ d3 ────────────────────────────────────────────
def mute_the_new_gate(files):
    text = files[REGISTRY]
    n = text.count(GATE_ATTR)
    if n != 1:
        raise SystemExit(f"🔴 拒绝落刀：`{REGISTRY}` 里锚点命中 {n} 次，应当 1 次")
    files[REGISTRY] = text.replace(GATE_ATTR, "    #[test]\n    #[ignore]\n" + GATE_FN)
    return "新那道闸挂上 `#[ignore]`（编译照旧、判据不跑）；锚点命中 1 次"


def compose(*steps):
    def run(files):
        return " ｜ ".join(s(files) for s in steps)
    return run


CUTS = {
    "d1": ("KR120D1 ① 七处里只漏 Cargo.lock ⇒ winchk（--locked）必须红", lock_falls_behind),
    "d2": ("KR120D1 ② 只改五处（权威源单独往前一档）⇒ 「六处一致」必须红", authority_runs_ahead),
    "d3": ("KR120D2 ① 复刻 K-R118 的 d7：七处一起 bump、CHANGELOG 不动 ⇒ 新闸必须红",
           bump_all_but_changelog),
    "d4": ("KR120D2 ② 最上一节有、但没有 breaking 段 ⇒ 有没有东西红（答案即读数）",
           drop_breaking_block),
    "d5": ("KR120D2 ②b breaking 段被埋到第二位 ⇒ 新闸第二条判定必须红", bury_breaking_block),
    "d6": ("KR120D2 ③ 阴性对照：新闸整个拿掉 ＋ d3 ⇒ 一条都不红",
           compose(mute_the_new_gate, bump_all_but_changelog)),
}


def do_apply(name: str) -> int:
    if BACKUP.exists():
        print(f"🔴 拒绝落刀：`{BACKUP.name}` 还在 —— 上一刀没还原。先 `--revert`。")
        return 3
    if name not in CUTS:
        print(f"🔴 没有这把刀：{name}")
        return 2
    why, step = CUTS[name]
    files = {f: (ROOT / f).read_text(encoding="utf-8") for f in TOUCHED}
    before = dict(files)
    note = step(files)
    changed = {f: t for f, t in files.items() if before[f] != t}
    if not changed:
        print("🔴 一处都没改到 —— 刀空转了，不许当成落地")
        return 3
    BACKUP.write_text(
        json.dumps({"cut": name, "orig": {f: before[f] for f in changed}}, ensure_ascii=False),
        encoding="utf-8",
    )
    for f, t in changed.items():
        (ROOT / f).write_text(t, encoding="utf-8")
        os.utime(ROOT / f, None)          # 🔴 mtime 必须是现在
    print(f"变异已落地：{name} —— {why}")
    print(f"  {note}")
    print(f"  动到的文件（{len(changed)} 份）：{sorted(changed)}")
    return 0


def do_revert() -> int:
    if not BACKUP.exists():
        print("🔴 没有备份可还原 —— 盘上要么本来就是干净的，要么有人手改过。自己核。")
        return 3
    d = json.loads(BACKUP.read_text(encoding="utf-8"))
    for f, t in d["orig"].items():
        # 🔴 **重写原文**，不是 `copy2` / `cp -a`：mtime 必变。
        (ROOT / f).write_text(t, encoding="utf-8")
        os.utime(ROOT / f, None)
        print(f"已还原：{f}（重写原文，mtime 已推到现在）")
    BACKUP.unlink()
    print(f"（这一趟退掉的是：{d['cut']}）")
    return 0


def main() -> int:
    # 🔴 占位：本文件**刻意不用** `shutil` 的复制族 —— `--revert` 是重写原文
    #    （`write_text` ＋ `os.utime`），mtime 必变。同 `K-R118-cut.py` 那一行的先例。
    _ = shutil
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    a = sys.argv[1]
    if a == "--list":
        for k, (why, _s) in CUTS.items():
            print(f"{k:4s} {why}")
        return 0
    if a == "--revert":
        return do_revert()
    if a == "--apply" and len(sys.argv) == 3:
        return do_apply(sys.argv[2])
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
