#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`K-R124` 的**刀具** —— 每一刀一份新副本，落刀前先断言锚点恰好命中 N 次。

# 纪律（`brief` 第 7 / 12c 条，写在这里不是装饰）

· **落刀前一律断言锚点命中数 == 期望**，对不上**一个字节都不改**，整刀记 `锚点不符`。
· **每一刀一份全新副本**（`--fresh`），不复用上一刀的目录：有些命令会落状态文件，
  第二趟就带着第一趟的痕。
· 副本里**没有 `.git`** —— 工作树的 `.git` 是一行指回原仓的指针，
  在副本里跑 `git` 会写进**原树的暂存区**。本工具**只拷需要的那几份文件**，根本不碰 `.git`。
· 还原不走 `copy2`（门禁 `copy2` 那一格正在数这件事）—— 本工具**不还原**，它换新副本。

# 跑法

    python3 evidence/K-R124-cut.py --list
    python3 evidence/K-R124-cut.py --run <刀>
    python3 evidence/K-R124-cut.py --all

副本落 `$K_R124_WORK`（缺省 `<仓根>/../k-r124-cuts`，**不进仓**）。
"""
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORK = Path(os.environ.get("K_R124_WORK") or (ROOT.parent / "k-r124-cuts"))

#: 副本里要有的那几份 —— 判据本体 ＋ 它的全部被测对象。
FILES = [
    ".github/workflows/release.yml",
    ".github/workflows/ci.yml",
    "package.json",
    "CHANGELOG.md",
    "scripts/release-notes.mjs",
    "evidence/K-R124-ruler.py",
    "evidence/K-R122-ruler.py",
]
#: 阴性对照那一刀要跑本仓**另一格**也读 `.github/` 的尺子（`K-R122`），它还要 `e2e/` 那些 `.sh`。
DIRS = ["e2e"]

RELEASE = ".github/workflows/release.yml"
RULER = "evidence/K-R124-ruler.py"
RENDERER = "scripts/release-notes.mjs"

ENV_LINE = "  PUBLISH: ${{ github.event_name == 'push' || inputs.publish == true }}"
CANON_LINE = 'CANON_ENV = "${{ github.event_name == \'push\' || inputs.publish == true }}"'

WIN_PUB = """      - name: Create / update GitHub Release
        if: env.PUBLISH == 'true'
        uses: softprops/action-gh-release@v2
        with:
          body_path: RELEASE_BODY.md
"""
LNX_PUB = """      - name: Append Linux artifacts to the release
        if: env.PUBLISH == 'true'
        uses: softprops/action-gh-release@v2
        with:
          body_path: RELEASE_BODY.md
"""
MARK6 = '    # ── ⑥ 每一处发布步骤都带正文来源（`KR124D2`）─────────────────────────────'

WIN_RENDER = """      - name: Render the Release body (CHANGELOG section for this version)
        if: env.PUBLISH == 'true'
        run: node scripts/release-notes.mjs RELEASE_BODY.md

      # 🔴 KR114D1：**本文件两处「往 GitHub Release 写」之一**"""
LNX_RENDER = """      - name: Render the Release body (CHANGELOG section for this version)
        if: env.PUBLISH == 'true'
        run: node scripts/release-notes.mjs RELEASE_BODY.md

      # 🔴 KR114D1：**本文件两处「往 GitHub Release 写」之二**"""


def sub(rel, old, new, want):
    """一处文本替换。`want` = 落刀前断言的锚点命中数。"""
    return ("sub", rel, old, new, want)


def rm(rel):
    return ("rm", rel, None, None, None)


# 每一刀：(说明, [动作…], 量哪一格)
CUTS = {
    "d1": (
        "`release.yml` 里 `env.PUBLISH` 那一行的字面改掉（`KR124D1` 刀①）",
        [sub(RELEASE, ENV_LINE, "  PUBLISH: ${{ github.event_name == 'push' }}", 1)],
        "ruler",
    ),
    "d2": (
        "**把守卫改回今天这个写法** —— 判据里那个字面换成 runner 渲染之后的值（`KR124D1` 刀②）",
        [sub(RULER, CANON_LINE, 'CANON_ENV = "false"', 1)],
        "ruler",
    ),
    "d3n": (
        "阴性对照：**守卫整步拿掉** ＋ 刀① —— 换成本仓另一格也读 `.github/` 的尺子来看",
        [sub(RELEASE, ENV_LINE, "  PUBLISH: ${{ github.event_name == 'push' }}", 1)],
        "other",
    ),
    "d4w": (
        "Windows 那处 `body_path` 指向一个**不存在也没人产出**的文件（`KR124D2` 刀①·单断）",
        [sub(RELEASE, WIN_PUB, WIN_PUB.replace("RELEASE_BODY.md", "RELEASE_NOTES_ABSENT.md"), 1)],
        "ruler",
    ),
    "d4l": (
        "Linux 那处 `body_path` 指向一个**不存在也没人产出**的文件（`KR124D2` 刀①·单断）",
        [sub(RELEASE, LNX_PUB, LNX_PUB.replace("RELEASE_BODY.md", "RELEASE_NOTES_ABSENT.md"), 1)],
        "ruler",
    ),
    "d5w": (
        "**摘掉 Windows 那处的 `body_path`**（`KR124D2` 刀②·单断 —— 失效方向逐字：只给 Windows 加）",
        [sub(RELEASE, WIN_PUB, WIN_PUB.replace("          body_path: RELEASE_BODY.md\n", ""), 1)],
        "ruler",
    ),
    "d5l": (
        "**摘掉 Linux 那处的 `body_path`**（`KR124D2` 刀②·单断 —— 这一刀就是那条失效方向本身）",
        [sub(RELEASE, LNX_PUB, LNX_PUB.replace("          body_path: RELEASE_BODY.md\n", ""), 1)],
        "ruler",
    ),
    "d5both": (
        "两处的 `body_path` 都摘掉（`KR124D2` 刀②·全断）",
        [
            sub(RELEASE, WIN_PUB, WIN_PUB.replace("          body_path: RELEASE_BODY.md\n", ""), 1),
            sub(RELEASE, LNX_PUB, LNX_PUB.replace("          body_path: RELEASE_BODY.md\n", ""), 1),
        ],
        "ruler",
    ),
    "d6": (
        "Windows 那处换回 `generate_release_notes: true`（＝ 本件开工前那一版的形状）",
        [sub(RELEASE, WIN_PUB,
             WIN_PUB.replace("          body_path: RELEASE_BODY.md\n",
                             "          generate_release_notes: true\n"), 1)],
        "ruler",
    ),
    "d6l": (
        "Linux 那处加上 `generate_release_notes: true`（`body_path` 仍在）—— 给「不回落」那一格单断",
        [sub(RELEASE, LNX_PUB,
             LNX_PUB.replace("          body_path: RELEASE_BODY.md\n",
                             "          body_path: RELEASE_BODY.md\n"
                             "          generate_release_notes: true\n"), 1)],
        "ruler",
    ),
    "d7": (
        "`CHANGELOG.md` 里本版那一段掏空（`KR124D3` 死值：造一处它该逮的东西）",
        [sub("CHANGELOG.md", "## [3.8.0] — 2026-09-14",
             "## [3.8.0] — 2026-09-14\n\n（待写）\n\n## [3.7.9] — 2026-09-14", 1)],
        "ruler",
    ),
    "d8": (
        "生成器整份删掉（`KR124D2` 地板）",
        [rm(RENDERER)],
        "ruler",
    ),
    "d9": (
        "**摘掉 Windows 那个渲染步骤** —— `body_path` 还在，但没人产出它了",
        [sub(RELEASE, WIN_RENDER,
             "      # 🔴 KR114D1：**本文件两处「往 GitHub Release 写」之一**", 1)],
        "ruler",
    ),
    "d10n": (
        "阴性对照：**本件新加的那几条判据（⑥⑦⑧）整块摘掉** ＋ 刀 `d4w`",
        [sub(RULER, MARK6, "    return (1 if fails else 0), passes[0], fails\n" + MARK6, 1),
         sub(RELEASE, WIN_PUB, WIN_PUB.replace("RELEASE_BODY.md", "RELEASE_NOTES_ABSENT.md"), 1)],
        "ruler",
    ),
    "d0": (
        "**把本件的实现整个退掉**（release.yml 退回开工前 ＋ 生成器删掉），判据一个字不动",
        [
            sub(RELEASE, WIN_RENDER,
                "      # 🔴 KR114D1：**本文件两处「往 GitHub Release 写」之一**", 1),
            sub(RELEASE, LNX_RENDER,
                "      # 🔴 KR114D1：**本文件两处「往 GitHub Release 写」之二**", 1),
            sub(RELEASE, WIN_PUB,
                WIN_PUB.replace("          body_path: RELEASE_BODY.md\n",
                                "          generate_release_notes: true\n"), 1),
            sub(RELEASE, LNX_PUB, LNX_PUB.replace("          body_path: RELEASE_BODY.md\n", ""), 1),
            rm(RENDERER),
        ],
        "ruler",
    ),
}


def fresh(name):
    d = WORK / name
    if d.exists():
        shutil.rmtree(d)
    for rel in FILES:
        src = ROOT / rel
        dst = d / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        # 🔴 刻意用 `write_bytes`，**不走 `shutil.copy2`** —— 门禁 `copy2` 那一格正在数
        #    `evidence/*.py` 里保元数据复制族的调用点。
        dst.write_bytes(src.read_bytes())
    for rel in DIRS:
        for src in sorted((ROOT / rel).rglob("*")):
            if src.is_file():
                dst = d / src.relative_to(ROOT)
                dst.parent.mkdir(parents=True, exist_ok=True)
                dst.write_bytes(src.read_bytes())
    return d


def apply(d, actions):
    notes = []
    for kind, rel, old, new, want in actions:
        p = d / rel
        if kind == "rm":
            if not p.exists():
                return None, f"锚点不符：`{rel}` 副本里就不存在"
            p.unlink()
            notes.append(f"删 `{rel}`")
            continue
        text = p.read_text(encoding="utf-8")
        hit = text.count(old)
        if hit != want:
            return None, f"锚点不符：`{rel}` 里锚点命中 {hit} 次（期望 {want}）—— 一个字节都没改"
        p.write_text(text.replace(old, new), encoding="utf-8")
        notes.append(f"`{rel}` 锚点命中 {hit}/{want}，变异已落地")
    return notes, None


def measure(d, which):
    env = dict(os.environ)
    if which == "ruler":
        env["K_R124_ROOT"] = str(d)
        cmd = ["python3", str(d / RULER)]
    else:
        env["K_R122_ROOT"] = str(d)
        cmd = ["python3", str(d / "evidence/K-R122-ruler.py")]
    proc = subprocess.run(cmd, cwd=str(d), env=env, capture_output=True, text=True)
    return proc.returncode, (proc.stdout + proc.stderr)


def interesting(out):
    keep = [l for l in out.splitlines() if l.startswith("FAIL") or "::error::" in l
            or l.startswith("---- 红") or "passed" in l]
    return keep[:6]


def run_one(name):
    why, actions, which = CUTS[name]
    d = fresh(name)
    notes, err = apply(d, actions)
    print(f"\n=== 刀 `{name}` —— {why}")
    if err:
        print("  " + err)
        return
    for n in notes:
        print("  · " + n)
    rc, out = measure(d, which)
    label = "本件的尺子 `K-R124-ruler.py`" if which == "ruler" else "另一格的尺子 `K-R122-ruler.py`"
    print(f"  量哪一格：{label} · rc={rc} · {'红' if rc else '不红'}")
    for l in interesting(out):
        print("    | " + l)


def main(argv):
    WORK.mkdir(parents=True, exist_ok=True)
    if "--list" in argv:
        for k, (why, _, _) in CUTS.items():
            print(f"{k:8s} {why}")
        return 0
    if "--all" in argv:
        for k in CUTS:
            run_one(k)
        return 0
    if "--run" in argv:
        run_one(argv[argv.index("--run") + 1])
        return 0
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
