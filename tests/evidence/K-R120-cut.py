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

    d1   `KR120D1` ① —— 七处里**只漏 `Cargo.lock`**（把它退回上一档，另外六处不动）
                       ⇒ 门禁 `winchk`（`cargo check --locked`）必须红。
                       **这是已知答案的回测**（`K-R118` `§0a` 就是这么判的）。
                       🔴 **第一版是 CRASH，不是读数**：`_patch(cur, -1)` 在 `patch == 0`
                       时算出 `3.8.-1`，`cargo` 当场 `failed to parse lock file`，
                       winchk / cargo / deadcode 三格一起红在**解析**上。
                       现在 `_patch` 会借位，读数与病历都留在
                       `evidence/K-R120-deathvalue.md` 里。
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
    d6   `KR120D2` ③ —— 阴性对照：把本轮插进去的那**几块整个删掉** ＋ 刀 `d3`
                       （`BLOCK_HEAD..BLOCK_TAIL` 框的是**本轮插进去的全部**，
                       收窗口补了第二块之后它一起摘；md5 自证仍然成立 —— 摘完就是基点那一份）
                       ⇒ 一条都不红。🔴 **第一版用 `#[ignore]`，那一刀打中了台子自己**
                       （`shared_crate_registry::every_ignored_test_still_has_someone_who_triggers_it`
                       当场红）—— 读数与病历留在 `evidence/K-R120-deathvalue.md`。
    d7   反方向 —— 七处一起**退回**上一档、`CHANGELOG.md` 留在新号上 ⇒ 新闸必须红。
                       它证的是这道闸**两个方向都有牙**（`d3` 是另一个方向）。
    d8   「把实现整个退掉」那一问 —— 七处退回 ＋ 删掉 `CHANGELOG.md` 最上面那一整节，
                       只留那道新闸 ⇒ **预期全绿**（它守的是一致，不是某个具体的号）。
    d9   收窗口 ① —— 只把两份 README 的「**此刻自称的版本**」退回一档、**其余七处不动**
                       ⇒ 那条新判据必须红。**这是已知答案的回测**：这一形收窗口那一刻
                       真的在盘上，而当时 16 格全绿（人群里没有它们）。
    d10  收窗口 ② —— 阴性对照：把收窗口补的那一段人群**整块摘掉** ＋ 刀 `d9`
                       ⇒ 一条都不红。

## 跑法（从工作树根起跑）

    python3 evidence/K-R120-cut.py --list
    python3 evidence/K-R120-cut.py --apply <刀名>
    python3 evidence/K-R120-cut.py --revert
"""

import hashlib
import json
import os
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
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

# 本轮插进 `doc_claim_registry.rs` 的那两块的边界。
# `BLOCK_HEAD .. BLOCK_TAIL` 把**两块一起**框住（`d6` 用它，并用 md5 自证退回了基点那一份）；
# `BLOCK2_HEAD .. BLOCK_TAIL` 只框**收窗口补的那第二块**（`d10` 用它）。
BLOCK_HEAD = "\n    /// 读仓根的一份文本。"
BLOCK2_HEAD = "\n    /// 〔`K-R120` 收窗口补，09-14〕"
BLOCK_TAIL = "\n    /// 〔audit-0805 08-06〕**文档里写成 `CONST = 数` 的"
# 第二块那条判据的名字 —— `d10` 摘完自证「摘干净了」用。
GATE2_FN = "the_docs_self_reported_release_is_the_version_we_ship"
# 两份 README 里「这份文档此刻自称的版本」那一处的逐字锚点（与判据里那张 `places` 同源）。
SELF_REPORT = [
    (README, "当前发布 **v"),
    (README_EN, "current release **v"),
]
# 基点 `0a92892` 上那份 `doc_claim_registry.rs` 的整份 md5 —— `d6` 摘完自证用。
REGISTRY_MD5_AT_BASE = "3f2e8c3cf441eb67a3c64437d46fcac2"


def breaking_mark(files) -> str:
    text = files[REGISTRY]
    n = text.count(MARK_DECL)
    if n != 1:
        raise SystemExit(f"🔴 拒绝落刀：`{REGISTRY}` 里 `BREAKING_MARK` 声明命中 {n} 次，应 1 次")
    at = text.index(MARK_DECL) + len(MARK_DECL)
    return text[at:text.index('"', at)]


def _patch(cur: str, delta: int) -> str:
    """往前 / 往后一档 —— 🔴 **算出来的每一位都必须 >= 0**。

    第一版只写了 `patch + delta`，`patch == 0` 时算出 `3.8.-1` ——
    那不是「落后一档」，那是一份**语法都不合法**的版本号：`cargo` 当场
    `failed to parse lock file`，winchk / cargo / deadcode 三格一起红在解析上。
    按 `brief` 第 7 条，那是**类型契约破了 ⇒ CRASH，不是读数**。
    ⇒ 借位：`patch` 到 0 就退 `minor`，`minor` 到 0 就退 `major`。
    """
    parts = cur.split(".")
    if len(parts) != 3:
        raise SystemExit(f"🔴 拒绝落刀：现值 {cur!r} 形状不像 X.Y.Z")
    a, b, c = (int(x) for x in parts)
    if delta >= 0:
        return f"{a}.{b}.{c + delta}"
    if c > 0:
        return f"{a}.{b}.{c - 1}"
    if b > 0:
        return f"{a}.{b - 1}.0"
    if a > 0:
        return f"{a - 1}.0.0"
    raise SystemExit(f"🔴 拒绝落刀：{cur!r} 已经退无可退")


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
def remove_the_new_gate(files):
    """把本轮插进 `doc_claim_registry.rs` 的那一整块**整块删掉**。

    🔴 **第一版不是这么切的，而它打中了台子自己**：那一版给判据挂 `#[ignore]`，
    结果 `shared_crate_registry::every_ignored_test_still_has_someone_who_triggers_it`
    当场红（逐字「这些 `#[ignore]` 测试没有任何 e2e 脚本会点名它们」）——
    阴性对照要的是「一条都不红」，而那一条红**与被测对象无关**，是刀自己招来的。
    ⇒ 换成整块摘除，并用**整份文件的 md5** 自证「退回基点那一份，一个字节不差」。
    """
    text = files[REGISTRY]
    for mark in (BLOCK_HEAD, BLOCK_TAIL):
        n = text.count(mark)
        if n != 1:
            raise SystemExit(f"🔴 拒绝落刀：`{REGISTRY}` 里锚点 {mark[:24]!r}… 命中 {n} 次，应当 1 次")
    lo, hi = text.index(BLOCK_HEAD), text.index(BLOCK_TAIL)
    if lo >= hi:
        raise SystemExit("🔴 拒绝落刀：两端次序反了，段界读法坏了")
    carved = text[:lo] + text[hi:]
    got = hashlib.md5(carved.encode("utf-8")).hexdigest()
    if got != REGISTRY_MD5_AT_BASE:
        raise SystemExit(
            f"🔴 拒绝落刀：摘完的 md5 是 {got}，而基点那份是 {REGISTRY_MD5_AT_BASE} ——\n"
            f"   说明这一块之外还有别的改动，这一刀会把它一起带走。先核清楚再切。"
        )
    files[REGISTRY] = carved
    return (f"整块摘掉本轮插进 `{REGISTRY}` 的那 {text[lo:hi].count(chr(10))} 行；"
            f"两端锚点各命中 1 次；摘完整份 md5 == 基点那份（{REGISTRY_MD5_AT_BASE}）")


# ── d7：反方向 —— 七处退回，CHANGELOG 留在新号上 ────────────────────────────
def unbump_all_but_changelog(files):
    """把版本 bump 这一半**整个退掉**，而 `CHANGELOG.md` 那一节留着。

    它答的是固定项那一问「把实现整个退掉，还有多少条新断言仍绿」的**一半**：
    退掉七处而留下 CHANGELOG ⇒ 新闸必须红（方向与 `d3` 相反）。
    ⚠ 两半**一起**退掉时新闸是**绿**的，而那是对的 —— 它守的是「一致」，
      不是「必须是某个具体的号」。这一点在交回时单列，别读成「退掉也不红」。
    """
    cur = _current(files)
    prev = _patch(cur, -1)
    note = _apply_edits(files, _edits(cur, prev), f"七处一起 {cur} → {prev}")
    return note + f"；`{CHANGELOG}` 一个字节没动（最上一节仍是 {cur}）"


# ── d8：把本件的实现整个退掉，只留那道新闸 ──────────────────────────────────
def revert_the_whole_implementation(files):
    """七处一起退回上一档 **并且** 把 `CHANGELOG.md` 最上面那一整节删掉。

    它答的是固定项那一问：「**把实现整个退掉，还有多少条新断言仍绿**」。
    预期是**绿** —— 而那是对的：本轮那道闸守的是「最上一节 == 那七处」这个**一致性**，
    不是「必须是某个具体的号」。退回去之后两侧仍然一致 ⇒ 它当然不该红。
    它的牙由 `d3`（CHANGELOG 落后）与 `d7`（CHANGELOG 超前）两个方向分别证。
    """
    cur = _current(files)
    prev = _patch(cur, -1)
    note = _apply_edits(files, _edits(cur, prev), f"七处一起 {cur} → {prev}")
    lines, at, end = _top_section(files[CHANGELOG])
    if f"[{cur}]" not in lines[at]:
        raise SystemExit(f"🔴 拒绝落刀：最上面那一节的标题里没有 `[{cur}]`，逐字是 {lines[at]!r}")
    files[CHANGELOG] = "\n".join(lines[:at] + lines[end:])
    return (note + f"；并删掉 `{CHANGELOG}` 最上面那一整节 "
            f"`{lines[at]}`（第 {at + 1}–{end} 行，{end - at} 行），段界锚点命中 1 次")


# ── d9：只把两份 README 的「此刻自称的版本」退回一档 ────────────────────────
def self_report_falls_behind(files):
    """两处 README 自称的版本退回上一档，**另外七处留在新号上**。

    这是 `K-R120` 收窗口那一刀的**已知答案回测**：这一形今天真的发生过 ——
    七处都改齐了 `3.8.0`，而这两处还写着 `当前发布 **v3.7.0**`，
    **而当时没有任何判据的人群含它们**（16 格全绿）。
    """
    cur = _current(files)
    prev = _patch(cur, -1)
    edits = [(f, f"{n}{cur}", f"{n}{prev}") for f, n in SELF_REPORT]
    note = _apply_edits(files, edits, f"两处「此刻自称的版本」{cur} → {prev}")
    return note + f"；另外七处仍是 {cur}，一个字节没动"


# ── d10：阴性对照 —— 把收窗口补的那一段人群整块摘掉 ＋ d9 ───────────────────
def remove_the_self_report_gate(files):
    """把收窗口补进去的那**一整块**（那条 README 自称版本的判据）摘掉。

    摘完自证：那条判据的名字在整份文件里出现 **0** 次。
    ⚠ 与 `d6` 的区别写清楚：`d6` 摘的是 `BLOCK_HEAD..BLOCK_TAIL`（**两块一起**，
      并用整份 md5 自证退回基点那一份）；本刀只摘第二块，第一块（CHANGELOG 那道闸）留着。
    """
    text = files[REGISTRY]
    for mark in (BLOCK2_HEAD, BLOCK_TAIL):
        n = text.count(mark)
        if n != 1:
            raise SystemExit(f"🔴 拒绝落刀：`{REGISTRY}` 里锚点 {mark[:26]!r}… 命中 {n} 次，应当 1 次")
    lo, hi = text.index(BLOCK2_HEAD), text.index(BLOCK_TAIL)
    if lo >= hi:
        raise SystemExit("🔴 拒绝落刀：两端次序反了，段界读法坏了")
    carved = text[:lo] + text[hi:]
    left = carved.count(GATE2_FN)
    if left != 0:
        raise SystemExit(f"🔴 拒绝落刀：摘完 `{GATE2_FN}` 还剩 {left} 处，没摘干净")
    files[REGISTRY] = carved
    return (f"整块摘掉收窗口补进 `{REGISTRY}` 的那 {text[lo:hi].count(chr(10))} 行；"
            f"两端锚点各命中 1 次；摘完 `{GATE2_FN}` 在整份文件里 0 处")


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
           compose(remove_the_new_gate, bump_all_but_changelog)),
    "d7": ("KR120D2 反方向：七处一起退回上一档、CHANGELOG 留在新号上 ⇒ 新闸必须红",
           unbump_all_but_changelog),
    "d8": ("把实现整个退掉（七处退回 ＋ 删掉 CHANGELOG 最上一节），只留那道新闸 ⇒ 预期全绿",
           revert_the_whole_implementation),
    "d9": ("收窗口①：只把两份 README「此刻自称的版本」退回一档、其余七处不动 ⇒ 新判据必须红",
           self_report_falls_behind),
    "d10": ("收窗口②阴性对照：把那一段人群整块摘掉 ＋ 刀 d9 ⇒ 一条都不红",
            compose(remove_the_self_report_gate, self_report_falls_behind)),
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
