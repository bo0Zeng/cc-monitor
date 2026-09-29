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

副本落 `$K_R124_WORK`（缺省**系统临时目录**下的 `k-r124-cuts`，**不进仓**）。
⚠ 〔`19c` 订正 09-19〕缺省值原来按 `ROOT.parent` 算 —— 那在 worktree 里**仍然落在主仓内**，
  理由与读数见下面 `WORK` 那一行的头注。
"""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
#: 🔴 〔`19c` 订正 09-19〕**副本落仓外，而「仓外」不等于 `ROOT.parent`。**
#: 原来这里是 `ROOT.parent / "k-r124-cuts"`，读起来像「仓的上一级」——
#: 而本仓的 agent 全跑在 `<主仓>/.claude/worktrees/<名>/` 里，那棵树的 `ROOT.parent`
#: 是 `<主仓>/.claude/worktrees/`，**仍然在主仓里面**。上一轮就这么把一份 32 MB 的副本
#: 落进了仓内，污染了一族「走一遍文件系统」的判据。
#: ⇒ 缺省改成系统临时目录，与仓根在哪**无关**；要换地方用 `K_R124_WORK`。
WORK = Path(os.environ.get("K_R124_WORK") or (Path(tempfile.gettempdir()) / "k-r124-cuts"))

#: 副本里要有的那几份 —— 判据本体 ＋ 它的全部被测对象。
#: 🔴 〔`19b` 订正 09-19〕**这张表在本拍之前整张指空**：仓库重组（09-18）把
#:   `scripts/` · `evidence/` · `e2e/` 全并进了 `tests/`，而本工具一份都没跟
#:   ⇒ `fresh()` 第一个 `read_bytes()` 就 `FileNotFoundError`，**这把刀具自己跑不起来**。
#:   坏的不是某一刀，是**整套死值验**：`release-gate` 那一格从重组那天起没法再切一刀验有没有牙。
#:   〔这正是本仓那句「坏尺子会把真缺陷一起藏起来」的又一例 —— 量具坏在**路径**上，
#:    不在判定上，所以它一声不吭。〕
#: ⚠ `19b` 起还多了四份被测对象（账本 · `gate.sh` · `build.rs` · 后端 `lib.rs`），
#:   因为那一拍给判据加了 ⑨⑩⑪⑫ 四组，它们读的就是这几份。
FILES = [
    ".github/workflows/release.yml",
    ".github/workflows/ci.yml",
    "package.json",
    "CHANGELOG.md",
    "tests/scripts/release-notes.mjs",
    "tests/scripts/gate.sh",
    "tests/evidence/K-R124-ruler.py",
    "tests/evidence/K-R122-ruler.py",
    "tests/evidence/K-G4-platform-ledger.py",
    "src/frontend/shell/build.rs",
    "src/backend/lib.rs",
    # 〔`19c` 09-19〕⑬ 那一组的两份新被测对象：re-embed 那条命令本体（⑬a–⑬c ＋ ⑬g 真跑它），
    # 与内嵌落点的 gitignore 住址（⑬d 两向对拍）。
    "tests/scripts/re-embed.sh",
    "src/frontend/shell/.gitignore",
]
#: 阴性对照那一刀要跑本仓**另一格**也读 `.github/` 的尺子（`K-R122`），它还要 `tests/e2e/` 那些 `.sh`。
DIRS = ["tests/e2e"]

#: 🔴 **本版版本号从 `package.json` 现读，不写死** —— 见 `d7` 那一刀的头注（原来写死成 `3.8.0`，
#: 版本走到 3.8.1 之后那一刀就切不动了，而它在表上仍然显示为「一刀」）。
CUR_VERSION = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))["version"]
_m = re.search(r"^## \[%s\].*$" % re.escape(CUR_VERSION),
               (ROOT / "CHANGELOG.md").read_text(encoding="utf-8"), re.M)
#: 本版那一段的标题行逐字；抠不到就留空 ⇒ `d7` 会以「锚点不符」整刀记，**不会静默变成一刀假绿**。
CUR_HEADING = _m.group(0) if _m else "<CHANGELOG.md 里没有 ## [%s] 这一段>" % CUR_VERSION

RELEASE = ".github/workflows/release.yml"
RULER = "tests/evidence/K-R124-ruler.py"
RENDERER = "tests/scripts/release-notes.mjs"
GATE = "tests/scripts/gate.sh"
LEDGER = "tests/evidence/K-G4-platform-ledger.py"
BUILD_RS = "src/frontend/shell/build.rs"
REEMBED = "tests/scripts/re-embed.sh"
GITIGNORE = "src/frontend/shell/.gitignore"

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
        run: node tests/scripts/release-notes.mjs RELEASE_BODY.md

      # 🔴 KR114D1：**本文件两处「往 GitHub Release 写」之一**"""
LNX_RENDER = """      - name: Render the Release body (CHANGELOG section for this version)
        if: env.PUBLISH == 'true'
        run: node tests/scripts/release-notes.mjs RELEASE_BODY.md

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
        "`CHANGELOG.md` 里**本版**那一段掏空（`KR124D3` 死值：造一处它该逮的东西）。"
        "🔴 〔`19b` 订正 09-19〕原来这一刀把版本号**写死成 `3.8.0`**，而 `package.json` "
        "早已走到 `%s` ⇒ 它割的是**别的版本**那一段，判据一个字不动、rc=0 —— "
        "**一把切不动的刀在死值验表上长得和一把好刀一模一样**。"
        "今天版本从 `package.json` 现读，刀跟着版本走。" % CUR_VERSION,
        [sub("CHANGELOG.md", CUR_HEADING, CUR_HEADING + "\n\n（待写）\n\n## [0.0.0-cut] — 掏空", 1)],
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
    # ══ `19b`（09-19）：产字节那条路 —— 六刀 ══════════════════════════════════
    "b1": (
        "🔴 **把身份住址指回 `main.rs`** —— 逐字重演步 9 那次漏改（`release.yml` 两处抽取"
        "只改了一处）。它该红在 ⑩b「住址实打」上，而**不是**红在别的地方",
        [sub(RELEASE, "  CCM_BACKEND_IDENTITY_SRC: src/backend/lib.rs",
             "  CCM_BACKEND_IDENTITY_SRC: src/backend/main.rs", 1)],
        "ruler",
    ),
    "b2": (
        "**摘掉一条产线** —— 把 `build-windows` 那步 `Build local backend (native)` 改个名"
        "（＝本机 Windows 那一格的字节没人产了）",
        [sub(RELEASE, "      - name: Build local backend (native)\n"
                      "        working-directory: src/backend\n"
                      "        run: cargo build --release --locked\n"
                      "      - name: Stage local backend for externalBin\n"
                      "        shell: pwsh",
             "      - name: Build local backend (renamed)\n"
                      "        working-directory: src/backend\n"
                      "        run: cargo build --release --locked\n"
                      "      - name: Stage local backend for externalBin\n"
                      "        shell: pwsh", 1)],
        "ruler",
    ),
    "b3": (
        "**给一个没承诺的格子悄悄开一条产线** —— musl 那步多编一个 macOS target"
        "（条 63 里 macOS 逐字是「目标里有，现在不做」）。该红在 ⑨b ＋ ⑨e",
        [sub(RELEASE, "          cargo zigbuild --release --locked --target aarch64-unknown-linux-musl",
             "          cargo zigbuild --release --locked --target aarch64-unknown-linux-musl\n"
             "          cargo zigbuild --release --locked --target x86_64-apple-darwin", 1)],
        "ruler",
    ),
    "b4": (
        "**把 `\"unknown\"` 兜底加回 `build.rs`** —— 逐字重演 `设计/16 §5.4a` 那次事故的形状",
        [sub(BUILD_RS, "    let build_id = backend_source_build_id();",
             "    let build_id = std::fs::read_to_string(backend_lib_rs())\n"
             "        .ok()\n"
             "        .and_then(|s| extract_build_id(&s))\n"
             "        .unwrap_or_else(|| \"unknown\".to_string());", 1)],
        "ruler",
    ),
    "b4b": (
        "**兜底值直接塞进 `backend_source_build_id` 里** —— 与 `b4` 的区别："
        "那一刀是**绕过**那个住址，这一刀是**污染**它。该红在 ⑪「没有兜底值」",
        [sub(BUILD_RS,
             '        .and_then(|s| extract_build_id(&s))\n        .unwrap_or_else(|| {',
             '        .and_then(|s| extract_build_id(&s))\n'
             '        .or_else(|| Some("unknown".to_string()))\n'
             '        .unwrap_or_else(|| {', 1)],
        "ruler",
    ),
    "b4c": (
        "**空串兜底塞回 `backend_stamp_marks`** —— 与 `unknown` 同族的那一形"
        "（运行期拿空界标去扫，对任何字节都答不出身份）",
        [sub(BUILD_RS,
             '    (mark("BUILD_STAMP_OPEN"), mark("BUILD_STAMP_CLOSE"))',
             '    let _ = &mark;\n'
             '    (\n'
             '        extract_str_const(&src, "BUILD_STAMP_OPEN").unwrap_or_default(),\n'
             '        extract_str_const(&src, "BUILD_STAMP_CLOSE").unwrap_or_default(),\n'
             '    )', 1)],
        "ruler",
    ),
    "b5": (
        "**门禁那一格的 zig 版本漂走** —— `muslbuild` 裁词改成跟宿主那版（0.16.0）。"
        "该红在 ⑫：本格的绿从此不代表发版那趟会绿",
        [sub(GATE, "（zig 0.14.0 / cargo-zigbuild 0.23.0）", "（zig 0.16.0 / cargo-zigbuild 0.23.0）", 1)],
        "ruler",
    ),
    "b6": (
        "**承诺面少一格** —— 账本里把「本机 Linux」那两行摘掉。该红在 ⑨a（两向集合相等）",
        [sub(LEDGER, '    ("本机 Linux", "cargo", "run_gate_sum cargo ",', '    ("XX 已摘", "cargo", "run_gate_sum cargo ",', 1),
         sub(LEDGER, '    ("本机 Linux", "backend", "run_gate backend ",', '    ("XX 已摘", "backend", "run_gate backend ",', 1)],
        "ruler",
    ),
    "b7n": (
        "阴性对照：**`19b` 新加的 ⑨⑩⑪⑫ 整块摘掉** ＋ 刀 `b1` —— 摘了就不该红",
        [sub(RULER, "    # ══ `19b`（09-19）：产字节那条路 ═══════════════════════════════════════════",
             "    return (1 if fails else 0), passes[0], fails\n"
             "    # ══ `19b`（09-19）：产字节那条路 ═══════════════════════════════════════════", 1),
         sub(RELEASE, "  CCM_BACKEND_IDENTITY_SRC: src/backend/lib.rs",
             "  CCM_BACKEND_IDENTITY_SRC: src/backend/main.rs", 1)],
        "ruler",
    ),
    # ══ `19c`（09-19）：`BUILD_ID` bump 的同拍债 —— re-embed ══════════════════
    "e1": (
        "🔴 **`.gitignore` 的落点行改回步 8 之前的旧名** —— 逐字重演今天盘上那个现物"
        "（改名只改了一边，两个内嵌落点从那天起没被挡住）。该红在 ⑬d（两向集合相等）",
        [sub(GITIGNORE, "\n/embedded-backends/\n", "\n/embedded-daemons/\n", 1)],
        "ruler",
    ),
    "e2": (
        "**摘掉一个落点的机检锚** —— `native-backend` 那一行上面那句 `⇐ 内嵌落点` 拿掉。"
        "盘侧少一格 ⇒ 该红在 ⑬d 的「登记了却没被挡」那一侧",
        [sub(GITIGNORE, "# ⇐ 内嵌落点\n# K-R42", "# K-R42", 1)],
        "ruler",
    ),
    "e2b": (
        "**只改 `build.rs` 那一侧的落点名** —— 与 `e1` 互为镜像：登记侧改名、gitignore 没跟。"
        "该红在 ⑬d 的另一侧（「挡着却没人登记」）",
        [sub(BUILD_RS, 'const EMBEDDED_BACKENDS_DIR: &str = "embedded-backends";',
             'const EMBEDDED_BACKENDS_DIR: &str = "embedded-daemons";', 1)],
        "ruler",
    ),
    "e3": (
        "**本机那条 re-embed 少编一个 arch** —— `REEMBED_TARGETS` 摘掉 aarch64。"
        "该红在 ⑬b（target 两向集合相等）＋ ⑬c（铺的 arch ↔ 吃的 arch）",
        [sub(REEMBED, "REEMBED_TARGETS=(x86_64-unknown-linux-musl aarch64-unknown-linux-musl)",
             "REEMBED_TARGETS=(x86_64-unknown-linux-musl)", 1)],
        "ruler",
    ),
    "e4": (
        "**配方漂了** —— `REEMBED_BUILD_FLAGS` 去掉 `--locked`（本机编出来的那份与发版那份"
        "依赖树可能不同）。该红在 ⑬b 的旗标那两条",
        [sub(REEMBED, "REEMBED_BUILD_FLAGS=(--release --locked)",
             "REEMBED_BUILD_FLAGS=(--release)", 1)],
        "ruler",
    ),
    "e5": (
        "**出路那个住址指空** —— `build.rs::REEMBED_CMD` 改成一个不存在的脚本名。"
        "该红在 ⑬a（与钉住的字面逐字相同）",
        [sub(BUILD_RS, 'const REEMBED_CMD: &str = "bash tests/scripts/re-embed.sh";',
             'const REEMBED_CMD: &str = "bash tests/scripts/reembed.sh";', 1)],
        "ruler",
    ),
    "e5b": (
        "**re-embed 那条命令整份删掉** —— ⑬ 的地板（出路指着一个不在盘上的东西，"
        "与「没有出路」在文案上一模一样）",
        [rm(REEMBED)],
        "ruler",
    ),
    "e6": (
        "**mtime 安全网改回只看一份源码** —— 步 9 那次搬家逼出来的那条改回去"
        "（改 `main.rs` 时它当场变瞎）。该红在 ⑬f",
        [sub(BUILD_RS, "let src_mtime = [backend_lib_rs(), backend_main_rs()]",
             "let src_mtime = [backend_lib_rs()]", 1)],
        "ruler",
    ),
    "e7": (
        "🔴 **把手抄的配方塞回出路里** —— 半 bump 那条 panic 的 ① 换成"
        "`cargo build --release --target …`（＝第二个住址回来了）。该红在 ⑬e 的禁词那一条，"
        "而**不该**红在「点名那条命令」那一条（还剩两处）",
        [sub(BUILD_RS, "                     ① `{REEMBED_CMD}`\\n\\\n",
             "                     ① `cargo build --release --target {arch}-unknown-linux-musl`\\n\\\n", 1)],
        "ruler",
    ),
    "e8n": (
        "阴性对照：**`19c` 新加的 ⑬ 整块摘掉** ＋ 刀 `e1` —— 摘了就不该红",
        [sub(RULER, "    # ══ `19c`（09-19）：`BUILD_ID` bump 的同拍债 —— re-embed ═══════════════════",
             "    return (1 if fails else 0), passes[0], fails\n"
             "    # ══ `19c`（09-19）：`BUILD_ID` bump 的同拍债 —— re-embed ═══════════════════", 1),
         sub(GITIGNORE, "\n/embedded-backends/\n", "\n/embedded-daemons/\n", 1)],
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
        cmd = ["python3", str(d / "tests/evidence/K-R122-ruler.py")]
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
