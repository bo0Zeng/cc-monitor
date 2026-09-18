#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R118 死值验的刀 —— 每一刀先断言锚点命中数，再落刀，再印「变异已落地」。

## 纪律（照 `references/brief.md` 第 7 · 11 · 12c 条 · `K-R115` 那一族）

- 🔴 **还原不许 `copy2` / `cp -a`**：`--revert` 是**重写原文**（`write_text` ＋ `os.utime`），
  mtime 必变 ⇒ 下一趟 `tsc` / `cargo` 一定重算。门禁 `copy2` 那一格（`K-R115` 第 14 格）
  正在数这件事，本文件自己不许犯它。
- 每一刀落刀前 `assert 锚点命中 == want`，对不上**一个字节都不改**、整趟放弃。
- 一趟只许有一把刀在盘上：`--apply` 之前若备份还在，拒绝落刀。

## 刀

    d1   `KR118D1` ① —— 把 `bumpCounted` 的**返回位**改回 `Counted<number>`
                       （＝ 把那 6 条 `TS2322` 原样放回去）⇒ 门禁 `tsc` 那一格必须红
    d2   `KR118D1` ② —— 门禁自述格数不跟（`16 格` → `15 格`）
                       ⇒ `evidence/K-R80-gate-cell-coverage.py` 必须红（那把尺子**不在门禁里跑**）
    d3   `KR118D1` ③ —— 阴性对照：`d1` ＋ 把 `tsc` 那一格整格拿掉 ⇒ 门禁**一条都不红**
    d4   `KR118D1` ④ —— 第二条判定（「程序面没被掏空」）的第一刀：把 `tsconfig.json` 的
                       `include` 收窄到一个子目录。
                       🔴 **实测这一刀射程过粗，没打中它想打的那一支** —— 收窄同时丢掉了
                       `src/vite-env.d.ts` 那份环境声明 ⇒ `tsc` 直接 `rc=2`，红在**退出码**
                       那一支上；顺带 `npm` 也红（`node-suite-registry-guard.vitest.ts`
                       早就有一条判据在守「include 里还有 `e2e`」）。读数留着，见留档。
    d4b  `KR118D1` ④b —— 换最小面再打一次：`exclude` 掉 `src/**/*.vitest.ts`。
                       程序面小一大截而**一条类型错都没有** ⇒ **`tsc` 退出码是 0**，
                       只能靠「份数对账」那一支拦 —— 那一支必须红
    d5   `KR118D2` ① —— 六处只改五处（`package.json` 单独 bump）
                       ⇒ `the_release_version_is_the_same_in_all_six_places` 必须红
    d6   `KR118D1` ⑤ —— 假红方向：往 `src/` 加一份**类型正确**的新 `.ts`
                       ⇒ 必须**不红**，而那一格的读数 +1（证明它数的是真程序面）
    d7   `KR118D2` ② —— 六处（＋ `Cargo.lock`）**一起** bump，而 `CHANGELOG.md` 最上一节
                       仍是旧版本号 ⇒ 问「有没有东西红」。**这一刀的答案本身就是读数**：
                       没东西红 ⇒ 如实登记「这一形没人守」。

## 跑法

    python3 evidence/K-R118-cut.py --list
    python3 evidence/K-R118-cut.py --apply <刀名>
    python3 evidence/K-R118-cut.py --revert
"""

import json
import os
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BACKUP = ROOT / ".k-r118-cut-backup.json"

COUNTED = "src/views/counted.ts"
GATE = "scripts/gate.sh"
TSCONFIG = "tsconfig.json"
PKG = "package.json"
PROBE = "src/views/kr118-probe-neutral.ts"

CARGO = "src-tauri/Cargo.toml"
CONF = "src-tauri/tauri.conf.json"
README = "README.md"
README_EN = "README.en.md"
LOCK = "src-tauri/Cargo.lock"

TOUCHED = [COUNTED, GATE, TSCONFIG, PKG, CARGO, CONF, README, README_EN, LOCK]

# 一份**类型完全正确**的新 `.ts`：判据判的是「这一趟真读进程序的份数 == 盘上现打的份数」，
# 不是「有没有新文件」⇒ 加一份合法文件必须**不红**，只让那一格的读数 +1。
PROBE_SRC = '''/**
 * `K-R118` 死值验第 ⑥ 刀（假红方向）用的中性探针：一份**类型完全正确**的模块。
 *
 * 门禁 `tsc` 那一格判的是「这一趟真读进程序的仓内 `.ts` 份数 == 盘上现打的份数」，
 * 不是「有没有人加文件」⇒ 本文件必须**不红**，只让那一格的读数 +1。
 */
export function kr118ProbeIdentity(n: number): number {
  return n;
}
'''


def sub(path, old, new, want=1):
    """文本替换刀：锚点必须恰好命中 `want` 次。"""
    def run(files):
        txt = files[path]
        n = txt.count(old)
        if n != want:
            raise SystemExit(f"🔴 拒绝落刀：`{path}` 里锚点命中 {n} 次，应当 {want} 次 —— 一个字节都不改")
        files[path] = txt.replace(old, new)
        return f"{path}：锚点命中 {n} 次（应 {want}）· 替换 1 处"
    return run


def create(path, text):
    def run(files):
        files[path] = text
        return f"{path}：**新建**（{len(text)} B）"
    return run


def compose(*steps):
    def run(files):
        return " ｜ ".join(s(files) for s in steps)
    return run


# ── 刀表 ────────────────────────────────────────────────────────────────────
# `d1`：把返回位改回去。**只动返回位那一行**（`Counted<number>` 这个名字本身不动）——
#       射程最小：入参位、`isKnown` / `liveRank` / `starRank` 一个字节都不碰。
RETURN_BACK = sub(
    COUNTED,
    "): CountedOut<number> {",
    "): Counted<number> {",
)
DROP_CELL = sub(
    GATE,
    "run_gate tsc '不是「几条断言过了」",
    ": skip-cell '不是「几条断言过了」",
)
SELF_COUNT_BACK = sub(GATE, "# │ 〔自述·格数〕16 格", "# │ 〔自述·格数〕15 格")
NARROW_INCLUDE = sub(
    TSCONFIG,
    '"include": ["src", "e2e"]',
    '"include": ["src/views"]',
)
# `d4b`：**最小面**那一刀。`exclude` 掉那 127 份 `.vitest.ts`（没有任何生产代码 import 它们，
# `src/vite-env.d.ts` 也还在 ⇒ 程序面小了一大截而**一条类型错都没有**）
# ⇒ `tsc` 退出码是 **0**，只有「真读进程序的份数 != 盘上现打的份数」那一支拦得住它。
DROP_VITEST_FROM_PROGRAM = sub(
    TSCONFIG,
    '"include": ["src", "e2e"]',
    '"include": ["src", "e2e"],\n  "exclude": ["src/**/*.vitest.ts"]',
)

CUTS = {
    "d1": ("KR118D1 ① bumpCounted 的返回位改回 Counted<number>（6 条 TS2322 原样回来）"
           " ⇒ 门禁 tsc 那一格必须红", RETURN_BACK),
    "d2": ("KR118D1 ② 自述格数不跟（16 → 15）⇒ K-R80 那把尺子必须红", SELF_COUNT_BACK),
    "d3": ("KR118D1 ③ 阴性对照：d1 ＋ 把 tsc 那一格整格拿掉 ⇒ 一条都不红",
           compose(RETURN_BACK, DROP_CELL)),
    "d4": ("KR118D1 ④ tsconfig 的 include 收窄到一个子目录 —— 🔴 实测射程过粗（丢掉 "
           "src/vite-env.d.ts ⇒ rc=2，红在退出码那一支；npm 另有一条判据也红）。读数留档，"
           "最小面那一刀是 d4b",
           NARROW_INCLUDE),
    "d4b": ("KR118D1 ④b exclude 掉全部 .vitest.ts ⇒ tsc 退出码是 0，而「份数对账」那一支必须红",
            DROP_VITEST_FROM_PROGRAM),
    "d5": ("KR118D2 ① 六处只改五处（package.json 单独 bump）⇒ 「六处一致」那条判据必须红",
           None),          # 版本号未定之前不落刀，见 do_apply 里的守卫
    "d6": ("KR118D1 ⑤ 假红方向：加一份类型正确的新 .ts ⇒ 必须不红，读数 +1",
           create(PROBE, PROBE_SRC)),
    "d7": ("KR118D2 ② 六处＋Cargo.lock 一起 bump 而 CHANGELOG 不动 ⇒ 有没有东西红（答案即读数）",
           None),
}


def bump_pkg_only(files):
    """`d5`：只把 `package.json` 那一处 bump 掉，别的五处不动。

    🔴 **本刀刻意不写死目标版本号** —— 「改成几」是 `KR118D2` 里 PM 要去问用户的那一问，
    量具不许替它裁。这里只把**权威源**那一处的补丁位 +1（`3.7.0` → `3.7.1`），
    造出「六处只对上五处」这个**形状**；它证的是那条判据认不认这个形状，
    与最终定几号无关。
    """
    txt = files[PKG]
    needle = '\n  "version": "'
    n = txt.count(needle)
    if n != 1:
        raise SystemExit(f"🔴 拒绝落刀：`{PKG}` 里锚点命中 {n} 次，应当 1 次")
    at = txt.index(needle) + len(needle)
    end = at
    while txt[end] != '"':
        end += 1
    cur = txt[at:end]
    parts = cur.split(".")
    if len(parts) != 3:
        raise SystemExit(f"🔴 拒绝落刀：`{PKG}` 现值 {cur!r} 形状不像 X.Y.Z")
    nxt = f"{parts[0]}.{parts[1]}.{int(parts[2]) + 1}"
    files[PKG] = txt[:at] + nxt + txt[end:]
    return f"{PKG}：权威源那一处 {cur} → {nxt}（只此一处；别的五处一个字节没动）"


CUTS["d5"] = (CUTS["d5"][0], bump_pkg_only)


def _next_patch(cur: str) -> str:
    parts = cur.split(".")
    if len(parts) != 3:
        raise SystemExit(f"🔴 拒绝落刀：现值 {cur!r} 形状不像 X.Y.Z")
    return f"{parts[0]}.{parts[1]}.{int(parts[2]) + 1}"


def bump_all_but_changelog(files):
    """`d7`：把判据点名的**六处** ＋ `Cargo.lock` 一起 bump，`CHANGELOG.md` 一个字不动。

    🔴 **同样刻意不写死目标版本号**（理由同 `d5`）：只把补丁位 +1，造出
    「版本已 bump 而 CHANGELOG 最上一节还是旧号」这个**形状**。
    ⚠ `Cargo.lock` 必须同拍改 —— 门禁 `winchk` 那一格跑的是 `cargo check --locked`，
      只改 `Cargo.toml` 会红在「lock 陈了」上，那就不是本刀要问的那件事了。
    """
    pkg = files[PKG]
    at = pkg.index('\n  "version": "') + len('\n  "version": "')
    cur = pkg[at:pkg.index('"', at)]
    nxt = _next_patch(cur)
    edits = [
        (PKG, f'\n  "version": "{cur}"', f'\n  "version": "{nxt}"'),
        (CARGO, f'\nversion = "{cur}"', f'\nversion = "{nxt}"'),
        (CONF, f'\n  "version": "{cur}"', f'\n  "version": "{nxt}"'),
        (README, f"当前版本: v{cur}", f"当前版本: v{nxt}"),
        (README, f"- **版本**：v{cur}", f"- **版本**：v{nxt}"),
        (README_EN, f"| Current: v{cur}", f"| Current: v{nxt}"),
        (LOCK, f'name = "monitor"\nversion = "{cur}"', f'name = "monitor"\nversion = "{nxt}"'),
    ]
    for path, old, new in edits:
        n = files[path].count(old)
        if n != 1:
            raise SystemExit(f"🔴 拒绝落刀：`{path}` 里锚点 {old!r} 命中 {n} 次，应当 1 次")
        files[path] = files[path].replace(old, new)
    return (f"{cur} → {nxt}，七处（判据点名的六处 ＋ Cargo.lock）各命中 1 次；"
            f"CHANGELOG.md 一个字节没动")


CUTS["d7"] = (CUTS["d7"][0], bump_all_but_changelog)


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
    changed = {f: t for f, t in files.items() if before.get(f) != t}
    if not changed:
        print("🔴 一处都没改到 —— 刀空转了，不许当成落地")
        return 3
    created = [f for f in changed if f not in before]
    BACKUP.write_text(
        json.dumps({"cut": name,
                    "orig": {f: before[f] for f in changed if f in before},
                    "created": created},
                   ensure_ascii=False),
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
    for f in d.get("created", []):
        p = ROOT / f
        if p.exists():
            p.unlink()
            print(f"已删掉：{f}（本刀新建的）")
    BACKUP.unlink()
    print(f"（这一趟退掉的是：{d['cut']}）")
    return 0


def main() -> int:
    # 🔴 占位：本文件**刻意不用** `shutil` 的复制族 —— `--revert` 是重写原文
    #    （`write_text` ＋ `os.utime`），mtime 必变。同 `K-R115-cut.py` 那一行的先例。
    _ = shutil
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    a = sys.argv[1]
    if a == "--list":
        for k, (why, _) in CUTS.items():
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
