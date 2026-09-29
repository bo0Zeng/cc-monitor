#!/usr/bin/env python3
# ruff: noqa: E501
"""条 78·S26-A1：**「有代码、但现在的设计里没有」全仓普查** —— 只产清单，不产删除。

住址：`<仓根>/tests/evidence/S26-A1-design-coverage-census.py`
服务的设计篇：`调研/设计/99-路线图与未覆盖面.md`（条 78；`§3.2`/`§3.3` 是它的前身，只列了一小批）
读数落点：`调研/真相源/03-不在设计里的代码-普查.md` ＋ `tests/evidence/S26-A1-readings.md`

跑法（仓根下）：
    python3 tests/evidence/S26-A1-design-coverage-census.py            # 主报告（四档）
    python3 tests/evidence/S26-A1-design-coverage-census.py --addresses # 逐项住址 + 判法
    python3 tests/evidence/S26-A1-design-coverage-census.py --json      # 机读
    python3 tests/evidence/S26-A1-design-coverage-census.py --selftest  # 反空真死值验
    python3 tests/evidence/S26-A1-design-coverage-census.py --backtest  # 条 67 那一刀的标定

退出码：
    0 = 每一格都切到了东西（地板全过）
    3 = **有格子空转**（触地板）—— 这是失败，不是 0 命中的绿
    4 = 标定失败（`--backtest`：已知答案没复现 ⇒ 尺子是坏的）

═══════════════════════════════════════════════════════════════════════════════
 🔴 一、为什么这一轮**只产清单** —— 写死在这里
═══════════════════════════════════════════════════════════════════════════════

条 67 那一刀（`src/backend/sidecars/` 2 008 行）**能一次做干净，是因为 `设计/15 §4.6`
逐条论证过那个交付方式不做了**。没有那层论证就删，等于拿「我没在设计里找到它」当
「它不该在」—— **那两句话不是一回事**。本量具只回答前一句，后一句要人去写论证。

⇒ 本脚本**不含任何删除动作**，也不输出「建议删除」这种措辞。它输出的是**待裁清单**。

═══════════════════════════════════════════════════════════════════════════════
 🔴 二、定义 —— 四个词各钉一次，改定义必须改这段注释
═══════════════════════════════════════════════════════════════════════════════

**(1) 人群（一项 = 一个生产源码文件）**

  住在 `src/bridge/src/` · `src/backend/` · `src/**/*.ts` 之下，且
    · 后缀是 `.ts` 或 `.rs`
    · 不在 `EXCLUDED_DIRS` / `EXCLUDED_FILE_RE` 里
    · **生产行数 > 0**（见 (2)）

  单位是**文件**，不是符号。理由：条 67 那一刀的单位就是文件族（`sidecars/` 四个文件），
  而「设计点名」这个动作在本仓的实际粒度也是文件/目录（22 篇里的住址几乎全是路径）。
  ⇒ 符号级的残留（某个 `pub fn` 没人用）**不在本量具射程内**，面⑧ 如实登记这个缺口。

**(2) 生产行数（`prod_lines`）**

  `.rs`：总行数 − 落在 `#[cfg(test)]` 块（含 `#[cfg(test)] mod x;` 声明行）里的行数。
  `.ts`：总行数（测试住 `tests/`，不与生产同文件）。

  🔴 **为什么必须这么算**：`src/bridge/src/` 里 39 个判据文件有 `98%` 的行在 `#[cfg(test)]`
  里（现打，见面⑦）。不剥就会把**判据**当成**生产代码**普查 —— 而判据「设计零提及 ＋
  生产零调用方」是**常态**，那会一次灌进来三万行假候选。⇒ `prod_lines == 0` 的文件
  **整个出人群**，单列进面⑦（「判据层」轨）。
  ⚠ 这一条与 `设计/99` 条 47（`16 §5d`：那 42 个判据文件全归 `tests/`）是**同一件事的两侧**
  —— 它们不是「不在设计里」，它们**已经在设计里、正在搬**。

**(3) 设计点名了它（①）**

  判法 = 在**去掉否定区**的 22 篇设计正文里，找这个文件的「住址键」。**五档，宁可宽**：

    T1 `path`     全相对路径逐字出现          `src/views/session-viewer.ts`
    T2 `tail`     父目录名 + 基名              `views/session-viewer.ts` · `control/launch.rs`
    T3 `base`     基名                        `session-viewer.ts` · `launch.rs`
    T4 `stem`     去后缀名，**且名字里含 `_` 或 `-`**（多词名才够特异）
                                              `session-viewer` · `profile_installer`
    T4b `stem`    单词 stem，**且长成代码 token**（反引号里 / 带 `::` / `/` / `.rs`）
                                              `` `panorama` `` · `adapter::` · `launch/`
    T5 `dir`      任一祖先目录被点名          `settings/` · `src/bridge/src/backend/`

  **候选 = T1..T5（含 T4b）全不命中**。这是刻意选的错误方向（纪律 2）：
  错要错成「漏报一项待裁」，不许错成「把一件设计过的东西列进删除候选」。
  ⇒ 只命中 T5（目录级）的**另列一档**（面⑥「弱覆盖」），让「宽」买到了什么看得见。

**(4) 生产调用方（②）**

  `.ts`：**别的生产 `.ts` 文件**里有一条 import 说明符解析到它。
         入口 `src/main.ts` 例外（`index.html` 的 `<script src>` 是它的调用方）。
  `.rs`：**别的生产 `.rs` 文件**的**生产文本**（遮注释 ＋ 剥 `#[cfg(test)]`）里
         引用了它的模块路径。
         🔴 **`mod x;` 声明行不算调用方** —— 它是「编不编它」，不是「谁用它」。
         条 67 的标定就卡在这一条上：`sidecars/` 当时有 `mod sidecars;`（在 `main.rs`），
         而**十几处别的提及全在注释里或判据的 `cfg(test)` 段里** ⇒ 真答案是**零调用方**。
         不遮注释、不剥 cfg(test)、把 `mod` 算成调用方 —— 三样里错任何一样，
         这个已知为「零调用方」的族都会显示成「有十个调用方」。`--backtest` 就验这一条。
         🔴 **`#[tauri::command]` 另算一路调用方**（`命令面`）：它的调用方是
         `lib.rs` 的 `generate_handler![…]` 注册表，不是某个 `use`。

  🔴 **单位是「族外调用方」，不是「文件调用方」** —— 这一条也是 `--backtest` 现打逼出来的。
     第一版按文件算，标定当场红：`sidecars/codepicture/fetch.rs` 报「调用方 1 个」，
     而那个调用方是**同族的兄弟** `acquire.rs`。可条 67 的已知答案是**整族零调用方**。
     ⇒ 一个「自己内部互相引、族外没人用」的子树，按文件算**永远算不出零** ——
     而那恰好是整族死代码的标准形状，也就是最该被普查抓住的那一种。
     族 = crate 根下第一段子目录（`src/backend/sidecars` · `src/views` · `src/settings`）；
     直接住根上的文件各自成族。四档按**族外**分，报告两个数都打。

═══════════════════════════════════════════════════════════════════════════════
 🔴 三、这个定义**排除了什么** —— 每条都是明写的取舍，不是忘了
═══════════════════════════════════════════════════════════════════════════════

  1. **`src/common/*`（8 个共享 crate）与 `src/bridge/vendor/*`（2 个）**。
     理由：用户给的人群逐字是「`src/bridge/src` · `src/backend` · `src/*.ts` · `src/**/*.ts`」。
     crates/ 与 vendor/ **不在里面**。⇒ 面⑨ 把它们的规模作为**已登记未扫面**打印出来，
     不假装扫过。（⚠ `codex-token-core` 属于丙那一族，面⑤ 会点它的名但不判。）
  2. **`src/generated/`**（ts-rs 生成物 81 个）、`src/bridge/gen/`、`src/bridge/embedded-daemons/`、
     `src/bridge/scripts/`、`src/bridge/icons/`、`src/bridge/capabilities/`、`src/doc/`。
     理由：生成物 / 内嵌资产 / 文档，不是人写的生产逻辑；改它们要改生成器或上游。
  3. **`tests/`** —— 用户逐字排除。
  4. **`*.d.ts` · `*.vitest.ts` · `*.test.ts` · `*.spec.ts` · `*-golden.ts` · `build.rs` ·
     `guard_support.rs`**。理由：类型声明 / 测试 / 入库夹具 / 构建脚本 / 判据支撑。
  5. **`.css` / `.json` / `.yml` / `.html`**。理由：本量具的两问②③ 只对 `.ts`/`.rs` 成立
     （CSS 没有 import 图可言）。⇒ `设计/40`/`41` 管的 CSS 面**已有自己的量具**
     （`css-ledger.vitest.ts`），本篇不重复；面⑨ 登记这个缺口。
     ⚠ 现打逮到一处 CSS/JSON 侧的残留（`src/bridge/tauri.sidecar.conf.json`），
     面⑨ 点名但**不判**，因为它出本量具的射程。
  6. **符号级残留**（模块在用、里面某个 `pub fn` 没人用）。理由见定义 (1)。
  7. **`调研/真相源/` 的提及不算「设计点名」**。理由：`99 §3.2` 逐字把
     「只在普查里被点过名」记作**无设计**。普查是「有什么」，设计是「该是什么」。
  8. **`99` 的 `§3`「还没有任何文档碰过的面」整节 = 否定区**，其中的住址**不算点名**。
     理由：那张表的语义就是「这些没有设计」。把它算成点名 ⇒ 每一项都变成丁，
     而这张表越全，普查结果就越空 —— **一个会自我抹平的判据**。
     ⇒ `NEGATION_SECTIONS` 按**标题原文**定位（不按行号，行号会腐）。

═══════════════════════════════════════════════════════════════════════════════
 🔴 四、四档怎么分
═══════════════════════════════════════════════════════════════════════════════

  甲  设计零提及（T1..T5 全不中） ＋ **生产零调用方**       ← 最像条 67 那一刀
  乙  设计零提及 ＋ **有生产调用方**                        ← 活的，但没人写过它该是什么
  丙  设计提过、判成「不做 / 搁置」                          ← 从设计里**读**出来的裁定（见 SHELVED）
  丁  设计提过、也在做                                       ← 不进清单，只报计数

  🔴 丙这一档**不许由脚本猜**（不许从「❌」「搁置」这些字面量去认）。
  它从 `SHELVED` 表来，而那张表**只存「哪一篇哪一节下了裁定」＋「射程怎么量」**，
  **不存裁定内容的副本** —— 同 `doc_claim_registry` 那条手法：
  那句裁定只有一个家（设计文档），脚本负责验「它还在那儿」＋「射程里今天有多少行」。
  ⇒ 裁定被改/删 ⇒ 本脚本红，而不是安静地拿一份过期副本报数。
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve()
REPO = HERE.parents[2]
DESIGN_DIR = REPO.parent / "调研" / "设计"

# ── 人群边界 ────────────────────────────────────────────────────────────────
POP_ROOTS = ("src",)
EXCLUDED_DIRS = (
    "src/generated",
    "src/doc",
    "src/common",
    "src/bridge/vendor",
    "src/bridge/gen",
    "src/bridge/embedded-daemons",
    "src/bridge/scripts",
    "src/bridge/icons",
    "src/bridge/capabilities",
    "src/bridge/target",
    "src/backend/target",
    "tests",
    "node_modules",
)
EXCLUDED_FILE_RE = re.compile(
    r"(\.vitest\.ts|\.test\.ts|\.spec\.ts|\.d\.ts)$"
    r"|-golden\.ts$"
    r"|/build\.rs$"
    r"|/guard_support\.rs$"
    r"|/tests?/"
    r"|/fixtures?/"
    r"|/__fixtures__/"
)

# ── 设计篇的否定区：标题原文（不按行号） ────────────────────────────────────
NEGATION_SECTIONS = {
    # 文件名 → 该文件里「这一节的住址不算点名」的标题原文（到下一个同级/更高级标题为止）
    "99-路线图与未覆盖面.md": ["## 3. 🔴 还没有任何文档碰过的面"],
}

# ── 丙档：设计里下过「不做 / 搁置」裁定的族 ──────────────────────────────────
#  每条 = (档名, 裁定住址-文件, 裁定住址-锚文本, 射程 glob 列表)
#  ⚠ **不存裁定内容**。脚本只验「锚文本还在那篇里」＋ 量射程今天多少行。
# ── 活体标定：仓里**自陈**了答案的那一处，本量具必须同意 ─────────────────────
#  (被测文件, 自陈住在哪, 自陈锚文本, 期望本量具的判定)
#  🔴 为什么要有它：`甲` 档今天是 **0 项**，而「0 项」与「尺子卡住了」长得一模一样。
#     `--backtest` 用的是历史语料（`git show`），万一 git 不可用就跑不了。
#     这一条用**今天的语料**、拿**代码自己写的那句话**当标准答案 ⇒
#     只要 `甲` 空得可疑，先看这条过不过。
#     ⚠ 同 `doc_claim_registry` 的手法：**不存那句话的副本**，从源文件里读。
LIVE_CALIBRATION = [
    ("src/backend/plugin/probe.rs", "src/backend/plugin/mod.rs",
     "今天**零生产调用方**", "unreachable"),
]

# ── 丙档：设计里下过「不做 / 搁置」裁定的族 ──────────────────────────────────
#  每条 = (档名, 裁定住址-文件, 裁定住址-锚文本, 射程 glob 列表)
#  ⚠ **不存裁定内容**。脚本只验「锚文本还在那篇里」＋ 量射程今天多少行。
SHELVED = [
    (
        "codex 适配面",
        "99-路线图与未覆盖面.md",
        "用户 09-17 明确搁置，暂不设计",
        [
            "src/backend/agents/codex/*.rs",
            "src/bridge/src/adapter/codex.rs",
            "src/bridge/src/codex_record.rs",
        ],
    ),
]

# ── 太泛的目录，不作 T5 依据 ────────────────────────────────────────────────
#  理由：它们覆盖整棵树，命中等于「没判」。
TOO_GENERIC_DIRS = {
    "src", "src/bridge", "src/bridge/src", "src/backend", "tests",
    "src/views", "doc", "调研", "设计", "真相源",
}
# ⚠ `src/views` 上榜是因为它被当「前端视图那一族」泛指过；它的成员改用 T1..T4 判。

CJK_DOC_RE = re.compile(r"\.md$")

# ── 地板：低于它说明尺子没切到东西 ──────────────────────────────────────────
FLOORS = {
    "design_docs": 20,          # 扫到的设计篇数
    "design_chars": 300_000,    # 设计正文字符数（去否定区后）
    "design_addr_paths": 150,   # 从设计里抽出的带后缀住址数
    "design_addr_dirs": 20,     # 从设计里抽出的目录住址数
    "pop_files": 200,           # 人群文件数
    "pop_prod_lines": 45_000,   # 人群生产行数
    "judged_files": 25,         # 判据层轨（prod_lines==0）文件数
    "ts_edges": 300,            # TS import 图的边数
    "rs_refs": 400,             # Rust 生产引用边数
    "matched_t1": 40,           # T1（全路径逐字）命中数 —— 它空了说明住址抽取坏了
    "bucket_ding": 100,         # 丁档（设计提过也在做）不能空
    "cand_total": 5,            # 甲+乙 不能是 0（0 项 = 尺子没切到，不是「全都设计过了」）
}


# ═══════════════════════════════════════════════════════════════════════════
#  文本处理：遮注释 · 剥 cfg(test)
# ═══════════════════════════════════════════════════════════════════════════
def mask_comments(src: str, lang: str, blank_strings: bool = False) -> str:
    """把注释换成空格（长度、行号不变）。`blank_strings=True` 时**连字符串内容一起抹**。

    照 `K-T68-A1-outward-copy-census.py` 的同名函数（同族量具，别重新发明）。
    必须遮的理由见头注定义 (4)：本仓注释里逐字引用代码住址，不遮 ⇒
    `sidecars/` 这种「注释里被提十几次、真调用方 0 个」的族会显示成有调用方。

    🔴 `blank_strings` 这个参数是**现打逮出来的 bug 修的**，不是装饰：
    `#[cfg(test)]` 的块界靠花括号配平找，而 `remote_write_registry.rs` 的测块里有
    **字符串里的花括号**（`"{}"` 之类）。只遮注释、不遮串 ⇒ 配平在半路就归零，
    **测块的后 120 行被当成生产代码**（这一处现打：该文件真生产行 0，误算成 120）。
    ⇒ 找块界用「连串一起抹」的骨架文本，抹的却是只遮注释的那一份。
    """
    n = len(src)
    out = list(src)
    i = 0
    prev_sig = ""

    def blank(a: int, b: int) -> None:
        for k in range(a, min(b, n)):
            if out[k] != "\n":
                out[k] = " "

    while i < n:
        c = src[i]
        nxt = src[i + 1] if i + 1 < n else ""

        if lang == "rs" and c == "r" and (nxt == '"' or nxt == "#"):
            j = i + 1
            hashes = 0
            while j < n and src[j] == "#":
                hashes += 1
                j += 1
            if j < n and src[j] == '"':
                term = '"' + "#" * hashes
                k = src.find(term, j + 1)
                k = n if k < 0 else k + len(term)
                if blank_strings:
                    blank(j + 1, k - len(term))
                i = k
                prev_sig = '"'
                continue

        if c in ('"', "'") or (lang == "ts" and c == "`"):
            q = c
            j = i + 1
            while j < n:
                ch = src[j]
                if ch == "\\":
                    j += 2
                    continue
                if ch == q:
                    j += 1
                    break
                if q == "`" and ch == "$" and j + 1 < n and src[j + 1] == "{":
                    d = 0
                    k = j + 1
                    while k < n:
                        if src[k] == "{":
                            d += 1
                        elif src[k] == "}":
                            d -= 1
                            if d == 0:
                                break
                        k += 1
                    j = k + 1
                    continue
                if q in ('"', "'") and ch == "\n":
                    break
                j += 1
            if blank_strings and j - 1 > i:
                blank(i + 1, j - 1)
            i = j
            prev_sig = q
            continue

        if c == "/" and nxt == "/":
            j = src.find("\n", i)
            j = n if j < 0 else j
            blank(i, j)
            i = j
            continue

        if c == "/" and nxt == "*":
            j = src.find("*/", i + 2)
            j = n if j < 0 else j + 2
            blank(i, j)
            i = j
            continue

        if (lang == "ts" and c == "/" and prev_sig in "(,=:[!&|?{;+\n") or (
            lang == "ts" and c == "/" and prev_sig == ""
        ):
            eol = src.find("\n", i)
            eol = n if eol < 0 else eol
            j = i + 1
            in_cls = False
            ok = False
            while j < eol:
                ch = src[j]
                if ch == "\\":
                    j += 2
                    continue
                if ch == "[":
                    in_cls = True
                elif ch == "]":
                    in_cls = False
                elif ch == "/" and not in_cls:
                    ok = True
                    j += 1
                    break
                j += 1
            if ok:
                i = j
                prev_sig = "/"
                continue

        if not c.isspace():
            prev_sig = c
        i += 1

    return "".join(out)


CFG_TEST_ATTR = re.compile(r"#\[cfg\((?:test|any\([^()]*test[^()]*\)|all\([^()]*test[^()]*\))\)\]")


CFG_TEST_MOD_RE = re.compile(
    CFG_TEST_ATTR.pattern + r"\s*(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+([A-Za-z_]\w*)\s*;"
)


def cfg_test_spans(skel: str) -> list:
    """在**骨架文本**（注释与字符串内容都已抹掉）上找出 `#[cfg(test)]` 管的每一段。

    🔴 两条与仓里那份旧实现不同、且都是现打逼出来的：

    ① **必须在骨架上配平**，不能在「只遮了注释」的文本上配平。
       `remote_write_registry.rs` 的测块里有**字符串里的花括号**，在只遮注释的文本上
       配平会在半路归零 ⇒ 该文件真生产行 0，误算成 **120**（现打）。

    ② **`#[cfg(test)] mod foo;` 没有 `{`**。旧版无条件去找下一个 `{`，会一路找到
       文件里下一个无关的 `{`，把一大片生产代码误当测试段抹掉。本仓 `lib.rs` 里有
       **18 条**这种声明（现打），所以这个差别不是理论上的。
       ⇒ 先看 `{` 与 `;` 谁先到：`;` 先到 ⇒ 只抹到 `;`。
    """
    spans = []
    n = len(skel)
    for m in CFG_TEST_ATTR.finditer(skel):
        brace = skel.find("{", m.end())
        semi = skel.find(";", m.end())
        if semi >= 0 and (brace < 0 or semi < brace):
            end = semi
        elif brace < 0:
            continue
        else:
            d = 0
            j = brace
            while j < n:
                if skel[j] == "{":
                    d += 1
                elif skel[j] == "}":
                    d -= 1
                    if d == 0:
                        break
                j += 1
            end = j
        spans.append((m.start(), min(end + 1, n)))
    return spans


def blank_spans(text: str, spans) -> str:
    out = list(text)
    for a, b in spans:
        for k in range(a, b):
            if out[k] != "\n":
                out[k] = " "
    return "".join(out)


def prod_text(raw: str, lang: str) -> str:
    masked = raw if MUT["no_comment_mask"] else mask_comments(raw, lang)
    if lang != "rs" or MUT["no_cfg_strip"]:
        return masked
    skel = mask_comments(raw, lang, blank_strings=True)
    return blank_spans(masked, cfg_test_spans(skel))


def test_only_mods(raw: str) -> set:
    """这个文件里 **`#[cfg(test)] mod x;`** 声明的子模块名集合。

    它们只在测试构建里被编进去 ⇒ 那些文件**不是生产代码**，整个出人群（面⑦）。
    这一条比「`prod_lines == 0`」硬：`ccm_cli_contract.rs` 的 `prod_lines` 是 **1**
    （一行结构性残余），靠行数阈值判会把它当生产代码，靠声明判才对。
    """
    skel = mask_comments(raw, "rs", blank_strings=True)
    return set(CFG_TEST_MOD_RE.findall(skel))


def nonblank(text: str) -> int:
    return sum(1 for ln in text.splitlines() if ln.strip())


# ═══════════════════════════════════════════════════════════════════════════
#  人群
# ═══════════════════════════════════════════════════════════════════════════
class Rec:
    __slots__ = ("rel", "lang", "total", "prod", "raw", "ptext", "keys", "modpath",
                 "design", "callers", "caller_kinds", "gates", "bucket", "notes", "ext")

    def __init__(self, rel, lang, raw):
        self.rel = rel
        self.lang = lang
        self.raw = raw
        self.total = raw.count("\n") + (1 if raw and not raw.endswith("\n") else 0)
        self.ptext = prod_text(raw, lang)
        self.prod = nonblank(self.ptext)
        self.keys = {}
        self.modpath = None
        self.design = []
        self.callers = set()
        self.caller_kinds = set()
        self.gates = set()
        self.bucket = None
        self.notes = []
        self.ext = set()


def excluded(rel: str) -> bool:
    if any(rel == d or rel.startswith(d + "/") for d in EXCLUDED_DIRS):
        return True
    return bool(EXCLUDED_FILE_RE.search("/" + rel))


def collect_files(reader, lister) -> dict:
    """lister() → 相对路径迭代；reader(rel) → 文本。抽出来是为了 `--backtest` 能换语料。"""
    out = {}
    for rel in lister():
        if not (rel.endswith(".ts") or rel.endswith(".rs")):
            continue
        if not any(rel == r or rel.startswith(r + "/") for r in POP_ROOTS):
            continue
        if excluded(rel):
            continue
        try:
            raw = reader(rel)
        except Exception:
            continue
        out[rel] = Rec(rel, "ts" if rel.endswith(".ts") else "rs", raw)
    return out


def disk_lister():
    """走源码树。**走的时候就剪掉排除目录**，不是走完再筛。

    ⚠ 第一版用 `REPO.rglob("*")` 再筛，于是每跑一次都要走一遍 `node_modules`
    （现打：那一棵比整个源码树大两个量级）—— `--selftest` 要跑六遍 `run()`，
    直接把自检拖到超时。剪枝是**性能**修的，不改人群定义（剪的正是 EXCLUDED_DIRS）。
    """
    import os
    for dirpath, dirnames, filenames in os.walk(REPO):
        rel_dir = Path(dirpath).relative_to(REPO).as_posix()
        rel_dir = "" if rel_dir == "." else rel_dir
        dirnames[:] = [
            d for d in dirnames
            if not (d in (".git", "node_modules", "target", ".build", "coverage")
                    or any((f"{rel_dir}/{d}" if rel_dir else d) == x
                           for x in EXCLUDED_DIRS))
        ]
        for f in sorted(filenames):
            yield f"{rel_dir}/{f}" if rel_dir else f


def disk_reader(rel):
    return (REPO / rel).read_text(encoding="utf-8", errors="replace")


# ═══════════════════════════════════════════════════════════════════════════
#  住址键
# ═══════════════════════════════════════════════════════════════════════════
def build_keys(rec: Rec) -> None:
    rel = rec.rel
    parts = rel.split("/")
    base = parts[-1]
    stem = base.rsplit(".", 1)[0]
    rec.keys["path"] = [rel, rel.removeprefix("src/")]
    rec.keys["tail"] = ["/".join(parts[-2:])] if len(parts) >= 2 else []
    rec.keys["base"] = [base]
    # T4：只有多词名（含 `_` 或 `-`）才够特异，散文里不会撞。
    rec.keys["stem"] = [stem] if ("_" in stem or "-" in stem) and len(stem) >= 4 else []
    # T4b：**单词 stem**（`panorama` / `adapter` / `launch`）在散文里会撞，所以
    # 不按裸文本找，只认它以**代码 token** 的样子出现：反引号包着、或带 `::` / `/` / `.rs`。
    # ⇒ 既够宽（`设计/15` 里的 `` `panorama` `` 会算点名），又不至于被散文里的
    # 「启动」「适配」这类词命中。`len >= 4` 是为了挡掉 `mod` / `fs` / `os`。
    rec.keys["stem_code"] = [stem] if stem not in rec.keys["stem"] and len(stem) >= 4 else []
    # `mod.rs` 的名字是它的目录名
    if base == "mod.rs" and len(parts) >= 2:
        d = parts[-2]
        rec.keys["base"].append(d + "/")
        if "_" in d or "-" in d:
            rec.keys["stem"].append(d)
        elif len(d) >= 4:
            rec.keys["stem_code"].append(d)
    ancestors = []
    for i in range(1, len(parts)):
        d = "/".join(parts[:i])
        if d in TOO_GENERIC_DIRS:
            continue
        ancestors.append(d)
        ancestors.append(d.removeprefix("src/"))
    rec.keys["dir"] = [a for a in ancestors if a and a not in TOO_GENERIC_DIRS]

    if rec.lang == "rs":
        if rel.startswith("src/bridge/src/"):
            sub = rel[len("src/bridge/src/"):]
        elif rel.startswith("src/backend/"):
            sub = rel[len("src/backend/"):]
        else:
            sub = rel
        sub = sub.removesuffix(".rs")
        segs = [s for s in sub.split("/") if s not in ("mod",)]
        if segs and segs[-1] in ("lib", "main"):
            segs = segs[:-1]
        rec.modpath = segs


# ═══════════════════════════════════════════════════════════════════════════
#  设计索引
# ═══════════════════════════════════════════════════════════════════════════
HEADING_RE = re.compile(r"^(#+)\s", re.M)


def cut_negation(text: str, anchors) -> tuple:
    """把否定区整节剪掉，返回 (剩下的正文, 被剪掉的正文)。"""
    cut = []
    for anchor in anchors:
        i = text.find(anchor)
        if i < 0:
            raise SystemExit(
                f"🔴 否定区锚文本找不到了：{anchor!r}\n"
                "   ⇒ 设计文档的标题被改过。**这是要人看的红**，不许自动放过：\n"
                "      改了标题就得同拍改 NEGATION_SECTIONS，否则那一节会被算成「点名」。"
            )
        level = len(text[i:].split(" ", 1)[0])
        j = i + len(anchor)
        while True:
            m = HEADING_RE.search(text, j)
            if not m:
                j = len(text)
                break
            if len(m.group(1)) <= level:
                j = m.start()
                break
            j = m.end()
        cut.append((i, j))
    keep = []
    removed = []
    prev = 0
    for a, b in sorted(cut):
        keep.append(text[prev:a])
        removed.append(text[a:b])
        prev = b
    keep.append(text[prev:])
    return "".join(keep), "".join(removed)


def load_design():
    docs = {}
    negated = {}
    for p in sorted(DESIGN_DIR.glob("*.md")):
        text = p.read_text(encoding="utf-8", errors="replace")
        anchors = NEGATION_SECTIONS.get(p.name)
        if anchors:
            text, cutpart = cut_negation(text, anchors)
            negated[p.name] = cutpart
        docs[p.name] = text
    return docs, negated


ADDR_PATH_RE = re.compile(r"[\w./-]+\.(?:ts|rs|css|json|md|html|py|toml|yml|sh|rc)\b")
ADDR_DIR_RE = re.compile(r"[\w][\w./-]*/")


def design_addresses(docs):
    """从设计正文里抽住址集合（带后缀的 / 目录的）。仅用于面②的分母与地板。"""
    paths, dirs = set(), set()
    for text in docs.values():
        paths.update(ADDR_PATH_RE.findall(text))
        for d in ADDR_DIR_RE.findall(text):
            dirs.add(d.rstrip("/"))
    return paths, dirs


def match_design(rec: Rec, docs) -> list:
    """返回 [(tier, doc, 证据)]，按 T1→T5。**宁可宽**：任一档命中即出候选。"""
    hits = []
    for tier, keys in (("T1", rec.keys["path"]), ("T2", rec.keys["tail"]),
                       ("T3", rec.keys["base"]), ("T4", rec.keys["stem"])):
        for k in keys:
            if not k:
                continue
            for name, text in docs.items():
                if k in text:
                    hits.append((tier, name, k))
        if hits:
            break
    if hits:
        return hits
    # T4b：单词 stem 只认「长成代码 token」的那几种出现形态
    #
    # 🔴 那个 `[^`\n]{0,40}` 的**长度与换行限制是必须的**，现打逮出来的：
    #    第一版写的是 `` `[^`]*\bstem\b[^`]*` ``。Markdown 里的**三反引号代码围栏**
    #    让「两个反引号之间」可以是**几千字的一整段**（围栏正文里没有反引号），
    #    于是 `store` / `paths` / `format` / `slash` / `compact` 这些词只要
    #    出现在任何围栏块里就算「被点名」—— 12 项 T4b 里有 9 项是这么来的假命中。
    #    ⇒ 宽是刻意的（纪律 2），但**宽到把整篇文档算成一个 token 不是宽，是没在判**：
    #      那会把真待裁项藏起来，而藏起来正是这一族最不该犯的错。
    #
    # ⚠ 后缀那一支**按语言分**：一个 `.ts` 文件永远不会被 `foo.rs` 点名。
    #   第一版不分，于是 `设计/00` 里的 `platform/paths.rs` 把 `src/paths.ts` 也算成了点名。
    # ⚠ 剩下的歧义**不修、明写**：`设计/00` 那句「`paths`+`records`+…五块」是**裸词**，
    #   它说的是 `claudecode/paths.rs`，但 `src/paths.ts` 的 stem 一样。
    #   按纪律 2 取**偏保守**（算成点名、不进候选），代价是 T4b 这一档要人核 ——
    #   所以报告把 T4b 全列出来，不藏。
    suffix = r"\.ts\b" if rec.lang == "ts" else r"\.rs\b|::"
    for k in rec.keys["stem_code"]:
        pat = re.compile(r"`[^`\n]{0,40}\b" + re.escape(k) + r"\b[^`\n]{0,40}`"
                         r"|\b" + re.escape(k) + r"(?:" + suffix + r"|/)")
        for name, text in docs.items():
            if pat.search(text):
                hits.append(("T4b", name, k))
    if hits:
        return hits
    for k in rec.keys["dir"]:
        for name, text in docs.items():
            if k + "/" in text or f"`{k}`" in text:
                hits.append(("T5", name, k))
    return hits


# ═══════════════════════════════════════════════════════════════════════════
#  ②：生产调用方
# ═══════════════════════════════════════════════════════════════════════════
TS_IMPORT_RE = re.compile(
    r"""(?:^|[\s;}])(?:import|export)\b[^'"()]*?from\s*['"]([^'"]+)['"]"""
    r"""|(?:^|[\s;}])import\s*['"]([^'"]+)['"]"""
    r"""|\bimport\s*\(\s*['"]([^'"]+)['"]\s*\)"""
    r"""|\brequire\s*\(\s*['"]([^'"]+)['"]\s*\)""",
    re.M,
)
TS_DYNAMIC_COMPUTED_RE = re.compile(r"\bimport\s*\(\s*(?!['\"])")


def ts_resolve(from_rel: str, spec: str, pop: dict):
    if not spec.startswith("."):
        return None
    base = (Path(from_rel).parent / spec).as_posix()
    base = re.sub(r"/\./", "/", base)
    while "/../" in base:
        base = re.sub(r"[^/]+/\.\./", "", base, count=1)
    for cand in (base, base + ".ts", base + "/index.ts", base.removesuffix(".js") + ".ts"):
        if cand in pop:
            return cand
    return None


def wire_ts(pop: dict, out_edges: dict) -> int:
    edges = 0
    for rel, rec in pop.items():
        if rec.lang != "ts":
            continue
        if TS_DYNAMIC_COMPUTED_RE.search(rec.ptext):
            rec.notes.append("本文件有**计算出来的** `import(...)` ⇒ 它的出边静态不全")
        for m in TS_IMPORT_RE.finditer(rec.ptext):
            spec = next(g for g in m.groups() if g)
            tgt = ts_resolve(rel, spec, pop)
            if tgt and tgt != rel:
                pop[tgt].callers.add(rel)
                pop[tgt].caller_kinds.add("import")
                out_edges.setdefault(rel, set()).add(tgt)
                edges += 1
    if "src/main.ts" in pop:
        pop["src/main.ts"].callers.add("index.html:<script src>")
        pop["src/main.ts"].caller_kinds.add("入口")
    return edges


MOD_DECL_RE = re.compile(r"^\s*(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+([A-Za-z_]\w*)\s*;", re.M)
TAURI_CMD_RE = re.compile(r"#\[tauri::command[^\]]*\]\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_]\w*)")


def rust_crate_of(rel: str):
    if rel.startswith("src/bridge/src/"):
        return "bridge"
    if rel.startswith("src/backend/"):
        return "backend"
    return None


CRATE_ROOTS = ("src/bridge/src", "src/backend")

# ── 变异开关：`--selftest` 用它们逐个关掉一条承重步骤 ────────────────────────
#  🔴 每个开关对准**定义里的一个取舍**，关掉它必须有一条判据当场红。
#     这比「三样一起关」强：一起关只能证明「它们合起来有用」，
#     逐个关才能证明「**每一条都在承重**」——第一版就是一起关，结果三样全关标定照样过，
#     那说明那个变异根本没碰到承重的那一步（现打）。
MUT = {"mod_as_edge": False, "no_cfg_strip": False, "no_comment_mask": False}

# ── 入口 —— 「可达」这一问的起点 ────────────────────────────────────────────
#  TS 只有一个入口：`index.html` 的 `<script type="module" src="/src/main.ts">`（现打）。
#  Rust 两个 crate 各一个根。
ENTRIES = (
    "src/main.ts",
    "src/bridge/src/lib.rs",
    "src/bridge/src/main.rs",
    "src/backend/main.rs",
)


def reachable(pop: dict, edges: dict) -> set:
    """从入口出发、**只走生产引用边**能走到的文件集合。

    🔴 **为什么最终用「可达」而不是「调用方计数」** —— 两次都是 `--backtest` 现打逼的：

      · 按**文件**算调用方：`sidecars/codepicture/fetch.rs` 报「1 个」，
        而那个调用方是同族兄弟 `acquire.rs` ⇒ 整族死代码算不出零。
      · 按**族外**算调用方：`sidecars/` 过了，但 `src/views/history-prefs.ts` 当场假红 ——
        它只被同族的 `views/history.ts` import，而 `history.ts` 是活的
        （`main.ts` → `history.ts` → `history-prefs.ts`）。「族外零调用方」把
        **一个活模块的内部帮手**判成了死代码。

    ⇒ 真正要问的不是「有几个人引它」，是「**从入口走不走得到它**」。
      `sidecars/` 走不到（`main.rs` 只有 `mod sidecars;`，**声明不是边**）；
      `history-prefs.ts` 走得到。同一条判据把两个案例都判对了。

    ⚠ `mod x;` **不是边**（定义 (4)）。`#[tauri::command]` 注册**是**边：
      命令面进来的那一跳不经过任何 `use`。
    """
    seen = set()
    stack = [e for e in ENTRIES if e in pop]
    # 命令面：被 `generate_handler!` 注册的模块自成入口
    stack += [r for r, rec in pop.items() if "命令面" in rec.caller_kinds]
    while stack:
        cur = stack.pop()
        if cur in seen:
            continue
        seen.add(cur)
        stack.extend(edges.get(cur, ()))
    return seen


def family_of(rel: str) -> str:
    """一项的**族** —— 「零调用方」这一问的真正单位。

    🔴 **这一条是 `--backtest` 现打逼出来的，不是设计出来的。**
    第一版按**文件**算调用方，标定当场红：`sidecars/codepicture/fetch.rs` 显示「调用方 1 个」，
    而那个调用方是**同族的兄弟** `acquire.rs`。可条 67 的已知答案是**整族零调用方** ——
    一个自己内部互相引、外面没人用的子树，按文件算永远算不出「零」，
    于是**整族死代码恰好是最容易漏的那一种**。

    ⇒ 族 = crate 根下的**第一段子目录**（`src/backend/sidecars` · `src/views` · `src/settings`）；
       直接住在根上的文件（`src/bridge/src/history.rs` · `src/tabs.ts`）各自成族。
    ⇒ 「外部调用方」= 族外的调用方。四档用**外部调用方**分，报告同时打印文件级的数。
    """
    for root in CRATE_ROOTS:
        if rel.startswith(root + "/"):
            sub = rel[len(root) + 1:]
            return root + "/" + sub.split("/")[0] if "/" in sub else rel
    if rel.startswith("src/"):
        sub = rel[4:]
        return "src/" + sub.split("/")[0] if "/" in sub else rel
    return rel


def external_callers(rec: "Rec") -> set:
    fam = family_of(rec.rel)
    out = set()
    for c in rec.callers:
        if not c.startswith("src/"):
            out.add(c)          # `lib.rs:generate_handler!` / `index.html:<script src>`
        elif family_of(c) != fam:
            out.add(c)
    return out


USE_STMT_RE = re.compile(r"\buse\s+([^;]{0,600});")
IDENT_RE = re.compile(r"[A-Za-z_]\w*")


def use_segments(text: str) -> set:
    """这段代码里每一条 `use` 语句用到的**所有路径段**（含 `{a, b}` 展开、含别名前的原名）。

    只在 `use` 里认「裸段名」，是为了把「模块引用」与「同名函数调用」分开 ——
    见 `wire_rust` 里那段注释记的两处现打假绿。
    """
    out = set()
    for m in USE_STMT_RE.finditer(text):
        out.update(IDENT_RE.findall(m.group(1)))
    return out


def wire_rust(pop: dict, out_edges: dict) -> int:
    """建 Rust 的「生产引用」边。`mod x;` 声明**不算**边（头注定义 (4)）。"""
    by_crate = {}
    for rel, rec in pop.items():
        if rec.lang != "rs":
            continue
        c = rust_crate_of(rel)
        if c:
            by_crate.setdefault(c, {})[tuple(rec.modpath)] = rel

    # `mod` 声明图（只为报「编不编它」，不作调用方）
    for rel, rec in pop.items():
        if rec.lang != "rs":
            continue
        decls = set(MOD_DECL_RE.findall(rec.ptext))
        parent = tuple(rec.modpath)
        c = rust_crate_of(rel)
        if not c:
            continue
        for d in decls:
            child = parent + (d,)
            tgt = by_crate.get(c, {}).get(child)
            if tgt:
                pop[tgt].caller_kinds.add("mod-decl")
                pop[tgt].notes.append(f"`mod {d};` 在 `{rel}`（**声明，不是调用方**）")
                if MUT["mod_as_edge"]:
                    out_edges.setdefault(rel, set()).add(tgt)

    # 命令面
    handler_text = ""
    lib = pop.get("src/bridge/src/lib.rs")
    if lib:
        m = re.search(r"generate_handler!\s*\[", lib.ptext)
        if m:
            d = 0
            j = m.end() - 1
            while j < len(lib.ptext):
                if lib.ptext[j] == "[":
                    d += 1
                elif lib.ptext[j] == "]":
                    d -= 1
                    if d == 0:
                        break
                j += 1
            handler_text = lib.ptext[m.end():j]
    for rel, rec in pop.items():
        if rec.lang != "rs":
            continue
        cmds = TAURI_CMD_RE.findall(rec.ptext)
        hit = [c for c in cmds if re.search(r"\b" + re.escape(c) + r"\b", handler_text)]
        if hit:
            rec.callers.add("lib.rs:generate_handler!")
            rec.caller_kinds.add("命令面")

    # 真正的引用边
    edges = 0
    for rel, rec in pop.items():
        if rec.lang != "rs":
            continue
        c = rust_crate_of(rel)
        if not c:
            continue
        text = MOD_DECL_RE.sub(" ", rec.ptext)   # ← 声明行剔掉
        useseg = use_segments(text)
        for modpath, tgt in by_crate.get(c, {}).items():
            if tgt == rel or not modpath:
                continue
            leaf = modpath[-1]
            # 🔴 两条路都要走，而**第二条是现打逮出来的漏报**：
            #   ① `leaf::item` 出现在任何位置 —— 最常见的模块引用形。
            #   ② `leaf` 作为一段**出现在某条 `use` 语句里** —— 治**别名 use**。
            #      `observe/accounts_query.rs` 写的是
            #      `use crate::agents::claudecode::accounts as cc_accounts;`，
            #      此后全程只出现 `cc_accounts::`，① 一次都不命中
            #      ⇒ 只认 ① 会把 `agents/claudecode/accounts.rs` 误判成「入口走不到」。
            #
            #   ⚠ ② **必须限定在 `use` 语句内**，不能放成「`::leaf` 出现过就算」：
            #      那样写会命中 `gate::probe(&pane)` / `perm::probe(path)` 这种**同名函数调用**，
            #      于是 `plugin/probe.rs` 被误判成可达 —— 而它是本量具的活体标定件
            #      （`plugin/mod.rs` 自陈「零生产调用方」）。现打两处假绿，标定当场红。
            if not (re.search(r"\b" + re.escape(leaf) + r"\s*::", text)
                    or leaf in useseg):
                continue
            # 是自己的祖先/后代？祖先引后代用 `self::`/裸名，都算调用方；但
            # **自己的父模块被自己引**不算（那是向上引，仍是真引用，保留）。
            pop[tgt].callers.add(rel)
            pop[tgt].caller_kinds.add("rust-ref")
            out_edges.setdefault(rel, set()).add(tgt)
            edges += 1
    return edges


# ═══════════════════════════════════════════════════════════════════════════
#  ③：删了会红哪几格
# ═══════════════════════════════════════════════════════════════════════════
GATE_GLOBS = ("tests/**/*.ts", "tests/**/*.rs", "tests/**/*.py", "tests/**/*.sh",
              ".github/workflows/*.yml", "src/doc/*.md", "package.json")


def gate_corpus():
    out = {}
    for g in GATE_GLOBS:
        for p in REPO.glob(g):
            # 🔴 把**本脚本自己**排掉：它的头注与 `SHELVED` / `LIVE_CALIBRATION` 表里
            #    逐字写着一批住址，不排就会给那几项凭空多算一格「删了会红」——
            #    一个量具把自己数进被测面，是这一族最容易犯的自指错。
            if p.name.startswith("S26-A1-"):
                continue
            if p.is_file():
                out[p.relative_to(REPO).as_posix()] = p.read_text(encoding="utf-8", errors="replace")
    # `src/bridge/src` 里判据的 cfg(test) 段（那 39 个还没搬走的判据文件）
    for p in sorted((REPO / "src/bridge/src").rglob("*.rs")):
        rel = p.relative_to(REPO).as_posix()
        raw = p.read_text(encoding="utf-8", errors="replace")
        skel = mask_comments(raw, "rs", blank_strings=True)
        spans = cfg_test_spans(skel)
        # 只留 cfg(test) 段（判据正文住这儿）
        testonly = "".join(
            raw[a:b] for a, b in spans
        )
        if testonly.strip():
            out[rel + " «cfg(test)段»"] = testonly
    return out


def wire_gates(pop: dict, gates: dict) -> None:
    for rec in pop.values():
        needles = [k for k in rec.keys["path"] + rec.keys["tail"] + rec.keys["base"] if k]
        for gname, gtext in gates.items():
            if gname.split(" ")[0] == rec.rel:
                continue
            if any(k in gtext for k in needles):
                rec.gates.add(gname)


# ═══════════════════════════════════════════════════════════════════════════
#  丙档
# ═══════════════════════════════════════════════════════════════════════════
def check_live_calibration(pop: dict, reach: set) -> list:
    """拿**代码自己写的那句话**当标准答案，验本量具的 ② 判定。

    ⚠ 不存那句话的副本 —— 从源文件里读。它被改/删 ⇒ 这条标定红。
    """
    out = []
    for target, claim_file, anchor, expect in LIVE_CALIBRATION:
        pth = REPO / claim_file
        if not pth.exists():
            out.append((target, claim_file, "自陈文件不在了", False))
            continue
        blob = pth.read_text(encoding="utf-8", errors="replace")
        if anchor not in blob:
            out.append((target, claim_file,
                        f"自陈锚文本找不到了：{anchor!r}（那句话被改过 ⇒ 标准答案没了）", False))
            continue
        if target not in pop:
            out.append((target, claim_file, "被测文件不在人群里", False))
            continue
        got = "unreachable" if target not in reach else "reachable"
        out.append((target, claim_file,
                    f"自陈「{anchor}」 ↔ 本量具量到「{got}」", got == expect))
    return out


def resolve_shelved(pop: dict, docs: dict, negated: dict):
    out = []
    for name, doc, anchor, globs in SHELVED:
        blob = docs.get(doc, "") + negated.get(doc, "")
        if anchor not in blob:
            raise SystemExit(
                f"🔴 丙档裁定锚文本找不到了：{doc} ← {anchor!r}\n"
                "   ⇒ 那条「搁置」的裁定被改过或删了。本脚本**刻意不存它的副本**，\n"
                "      所以只能红，不能拿一份过期副本报数（`doc_claim_registry` 同手法）。"
            )
        members = []
        for g in globs:
            pat = re.compile("^" + re.escape(g).replace(r"\*", "[^/]*") + "$")
            members += [r for r in pop if pat.match(r)]
        out.append((name, doc, anchor, sorted(set(members))))
    return out


# ═══════════════════════════════════════════════════════════════════════════
#  主流程
# ═══════════════════════════════════════════════════════════════════════════
def run(pop_lister=disk_lister, pop_reader=disk_reader, design_loader=load_design,
        want_gates=True):
    docs, negated = design_loader()
    allfiles = collect_files(pop_reader, pop_lister)

    # ── 判据层出人群：两条硬判据，不是行数阈值 ────────────────────────────
    #  (a) `#[cfg(test)] mod x;` 声明的模块 —— 只在测试构建里编进去，不是生产代码；
    #  (b) `prod_lines == 0` —— 整份住在 `#[cfg(test)]` 里。
    #  🔴 (a) 不可省：`ccm_cli_contract.rs` 的 `prod_lines` 是 1（结构性残余一行），
    #     只用 (b) 会把它当生产代码，于是一个纯判据文件进了「甲」档待裁清单。
    testonly_names = {"bridge": set(), "backend": set()}
    for rel, rec in allfiles.items():
        if rec.lang != "rs":
            continue
        c = rust_crate_of(rel)
        if c:
            testonly_names[c] |= test_only_mods(rec.raw)
    judged, pop = {}, {}
    for rel, rec in allfiles.items():
        why = None
        if rec.prod == 0:
            why = "prod_lines==0"
        elif rec.lang == "rs":
            c = rust_crate_of(rel)
            leaf = rel.rsplit("/", 1)[-1].removesuffix(".rs")
            if leaf == "mod":
                leaf = rel.split("/")[-2] if "/" in rel else leaf
            if c and leaf in testonly_names[c]:
                why = "`#[cfg(test)] mod` 声明"
        if why:
            rec.notes.append(f"出人群：{why}")
            judged[rel] = rec
        else:
            pop[rel] = rec

    for rec in pop.values():
        build_keys(rec)
    for rec in judged.values():
        build_keys(rec)

    edges = {}
    ts_edges = wire_ts(pop, edges)
    rs_edges = wire_rust(pop, edges)
    reach = reachable(pop, edges)
    for rel, rec in pop.items():
        if rel not in reach:
            rec.notes.append("**从任何入口都走不到**（只走生产引用边，`mod x;` 不算边）")

    for rec in pop.values():
        rec.design = match_design(rec, docs)

    gates = gate_corpus() if want_gates else {}
    if gates:
        wire_gates(pop, gates)

    calib = check_live_calibration(pop, reach)
    shelved = resolve_shelved(pop, docs, negated)
    shelved_members = {m for _, _, _, ms in shelved for m in ms}

    for rec in pop.values():
        rec.ext = external_callers(rec)
    for rec in pop.values():
        if rec.rel in shelved_members:
            rec.bucket = "丙"
        elif not rec.design:
            rec.bucket = "乙" if rec.rel in reach else "甲"
        else:
            rec.bucket = "丁"

    paths, dirs = design_addresses(docs)
    metrics = {
        "design_docs": len(docs),
        "design_chars": sum(len(t) for t in docs.values()),
        "design_addr_paths": len(paths),
        "design_addr_dirs": len(dirs),
        "pop_files": len(pop),
        "pop_prod_lines": sum(r.prod for r in pop.values()),
        "judged_files": len(judged),
        "ts_edges": ts_edges,
        "rs_refs": rs_edges,
        "matched_t1": sum(1 for r in pop.values() if r.design and r.design[0][0] == "T1"),
        "bucket_ding": sum(1 for r in pop.values() if r.bucket == "丁"),
        "cand_total": sum(1 for r in pop.values() if r.bucket in ("甲", "乙")),
    }
    return dict(pop=pop, judged=judged, docs=docs, negated=negated, shelved=shelved,
                gates=gates, metrics=metrics, reach=reach, edges=edges, calib=calib)


def P(*a):
    print(*a)


def fmt_gates(rec, limit=4):
    if not rec.gates:
        return "（静态扫不出登记它的判据 ⇒ **判不了**，见面⑧）"
    g = sorted(rec.gates)
    s = " · ".join(x.replace(" «cfg(test)段»", "†") for x in g[:limit])
    return s + (f" …共 {len(g)} 处" if len(g) > limit else "")


def main_report(args) -> int:
    st = run()
    pop, judged, metrics = st["pop"], st["judged"], st["metrics"]

    P("═" * 79)
    P(" 条 78 · S26-A1：「有代码、但现在的设计里没有」全仓普查")
    P(" 定义写死在本脚本头注。**只产清单，不产删除。**")
    P("═" * 79)

    # ── 面① 人群 ─────────────────────────────────────────────────────────
    P("\n【面①】人群 —— 谁进了这次普查")
    ts = [r for r in pop.values() if r.lang == "ts"]
    rs = [r for r in pop.values() if r.lang == "rs"]
    P(f"  生产文件 {len(pop)} 个 · 生产行（去空行 · Rust 已剥 cfg(test)）{metrics['pop_prod_lines']:,}")
    P(f"    · `.ts` {len(ts):>3} 个 / {sum(r.prod for r in ts):>6,} 行")
    P(f"    · `.rs` {len(rs):>3} 个 / {sum(r.prod for r in rs):>6,} 行")
    P(f"  出人群的「判据层」（`prod_lines == 0`）：{len(judged)} 个 / "
      f"{sum(r.total for r in judged.values()):,} 总行 —— 见面⑦")

    # ── 面② 设计索引 ─────────────────────────────────────────────────────
    P("\n【面②】设计覆盖了什么 —— 先把它变成可查的")
    P(f"  扫过 {metrics['design_docs']} 篇（`调研/设计/*.md`）· 正文 {metrics['design_chars']:,} 字符（已去否定区）")
    P(f"  从正文抽出带后缀住址 {metrics['design_addr_paths']} 个 · 目录住址 {metrics['design_addr_dirs']} 个")
    for name, cut in st["negated"].items():
        P(f"  🔴 否定区已剪：`{name}` 的「还没有任何文档碰过的面」整节（{len(cut):,} 字符）")
        P("     理由：那张表的语义是「这些没有设计」。算成点名 ⇒ 表越全、普查越空。")
    tiers = {}
    for r in pop.values():
        t = r.design[0][0] if r.design else "—"
        tiers[t] = tiers.get(t, 0) + 1
    P("  命中档位分布（每个文件取最强的一档）：")
    for t in ("T1", "T2", "T3", "T4", "T4b", "T5", "—"):
        if t in tiers:
            lbl = {"T1": "全路径", "T2": "父目录+基名", "T3": "基名", "T4": "多词 stem",
                   "T4b": "单词 stem（只认代码 token 形）",
                   "T5": "**只到目录级**", "—": "**一个字都没有**"}[t]
            P(f"    {t}  {tiers[t]:>4} 个   {lbl}")

    # ── 面③④ 甲乙 ───────────────────────────────────────────────────────
    jia = sorted([r for r in pop.values() if r.bucket == "甲"], key=lambda r: -r.prod)
    yi = sorted([r for r in pop.values() if r.bucket == "乙"], key=lambda r: -r.prod)

    P("\n" + "─" * 79)
    P(f"【面③】**甲 · 设计零提及 ＋ 生产零调用方** —— {len(jia)} 项 / {sum(r.prod for r in jia):,} 行")
    P("  （最像条 67 那一刀。⚠ 仍然**不等于该删** —— 那一刀有 `15 §4.6` 一整节论证，这里没有。）")
    P("─" * 79)
    unreach = sorted([r for r in pop.values() if r.rel not in st["reach"]],
                     key=lambda r: -r.prod)
    if not jia:
        P("  **0 项。** 而「0 项」与「尺子卡住了」长得一模一样 ⇒ 下面两条现打的对照，")
        P("  就是用来分开这两种情况的：")
        P(f"    · 活体标定（面⑪）：仓里自陈「零生产调用方」的那一处，本量具"
          f"{'**同意**' if all(ok for *_, ok in st['calib']) else '**不同意**'} ⇒ 它量得出「走不到」。")
        P(f"    · 今天全仓「入口走不到」的文件共 **{len(unreach)} 个**"
          f"（{sum(r.prod for r in unreach):,} 生产行）—— 指针在动，只是它们**设计都提过**：")
        for r in unreach:
            d = r.design[0] if r.design else None
            P(f"        · `{r.rel}` {r.prod} 行 · 档 {r.bucket}"
              + (f" · ① `设计/{d[1]}` {d[0]}:`{d[2]}`" if d else ""))
        P("  ⇒ 结论如实写：**今天没有「设计零提及 ＋ 入口走不到」的文件**。")
        P("     条 67 那一刀之后，这一格是真空的，不是量不出来。")
    for r in jia:
        P(f"\n  ▸ `{r.rel}`  ④ {r.prod} 生产行 / {r.total} 总行")
        P(f"      ① 设计：**零提及**（T1..T5 全不中；扫 {metrics['design_docs']} 篇，键="
          f"{', '.join(k for k in r.keys['path'] + r.keys['base'] + r.keys['stem'] if k)}）")
        P(f"      ② **从入口走不到**（族外调用方 {len(r.ext)} · 文件级 {len(r.callers)}）"
          + ("；" + " / ".join(r.notes) if r.notes else ""))
        P(f"      ③ 删了会红：{fmt_gates(r)}")

    P("\n" + "─" * 79)
    P(f"【面④】**乙 · 设计零提及 ＋ 有生产调用方** —— {len(yi)} 项 / {sum(r.prod for r in yi):,} 行")
    P("  （活的，但没人写过它该是什么。`99 §3.2` 已点过一批，这里是全量。）")
    P("─" * 79)
    for r in yi:
        callers = sorted(r.ext)
        P(f"\n  ▸ `{r.rel}`  ④ {r.prod} 生产行 / {r.total} 总行")
        P("      ① 设计：**零提及**（T1..T5 全不中）")
        P(f"      ② 生产调用方（族外）{len(callers)} 个（{'/'.join(sorted(r.caller_kinds))}）："
          f"{' · '.join('`'+c+'`' for c in callers[:4])}"
          + (f" …共 {len(callers)}" if len(callers) > 4 else ""))
        P(f"      ③ 删了会红：{fmt_gates(r)}")

    # ── 面⑤ 丙 ──────────────────────────────────────────────────────────
    P("\n" + "─" * 79)
    P("【面⑤】**丙 · 设计提过、但判成「不做 / 搁置」**")
    P("  （裁定从设计里**读**出来，脚本不存它的副本；锚文本没了就红。）")
    P("─" * 79)
    for name, doc, anchor, members in st["shelved"]:
        tot = sum(pop[m].prod for m in members if m in pop)
        P(f"\n  ▸ **{name}** —— 裁定住 `设计/{doc}`，锚：「{anchor}」")
        P(f"      射程内人群 {len(members)} 个文件 / {tot:,} 生产行")
        for m in members:
            r = pop[m]
            P(f"        · `{m}` {r.prod} 行 · 族外调用方 {len(r.ext)} 个"
              f"（{'/'.join(sorted(r.caller_kinds)) or '无'}）")

    # ── 面⑥ 丁 与弱覆盖 ────────────────────────────────────────────────
    weak = sorted([r for r in pop.values() if r.design and r.design[0][0] == "T5"],
                  key=lambda r: -r.prod)
    P("\n" + "─" * 79)
    P(f"【面⑥】**丁 · 设计提过也在做** —— {metrics['bucket_ding']} 项（不列清单，按用户要求）")
    P(f"  其中 **只到目录级**（T5）的 {len(weak)} 项 / {sum(r.prod for r in weak):,} 行 —— "
      "「宁可宽」买到的就是这一批：")
    P("  它们**没有被逐个点名**，只是住在一个被点名的目录里。要不要往下追由人决定。")
    P("─" * 79)
    for r in weak[:25]:
        d = r.design[0]
        P(f"  · `{r.rel}` {r.prod:>5} 行   ← 只因 `设计/{d[1]}` 提了目录 `{d[2]}/`")
    if len(weak) > 25:
        P(f"  …另 {len(weak) - 25} 项，`--addresses` 全列")

    # ── 面⑦ 判据层 ─────────────────────────────────────────────────────
    P("\n" + "─" * 79)
    P(f"【面⑦】出人群的「判据层」轨 —— {len(judged)} 个文件 / {sum(r.total for r in judged.values()):,} 总行")
    P("  `prod_lines == 0`：整份住在 `#[cfg(test)]` 里 ⇒ 不是生产代码，不进四档。")
    P("  🔴 它们**已经在设计里**：`设计/99` 条 47 / `16 §5d`「那 42 个判据文件全归 `tests/`」。")
    P("     ⇒ 不是「不在设计里的代码」，是**在搬的路上**。")
    P("─" * 79)
    jl = sorted(judged.values(), key=lambda r: -r.total)
    for r in jl[:12]:
        P(f"  · `{r.rel}` {r.total:>5} 行")
    if len(jl) > 12:
        P(f"  …另 {len(jl) - 12} 个")

    # ── 面⑧ 判不了 ─────────────────────────────────────────────────────
    P("\n" + "─" * 79)
    P("【面⑧】**判不了** —— 静态扫不出来的调用路径，如实登记（纪律 4）")
    P("─" * 79)
    P("  ② 这一问在下面这几族上**结论不可靠**，不许读成「零调用方」：")
    P("   1. **动态 `invoke(\"<名>\")`** —— `src/ipc/commands.ts` 头注自陈：")
    P("      「Rust 有而 TS 静态看不见的那 7 个动态名」。⇒ 带 `#[tauri::command]` 的模块，")
    P("      本量具只认「名字逐字出现在 `generate_handler![…]` 里」这一条，认不出动态构名。")
    P("   2. **`querySelector` / 事件委托 / `addEventListener`** —— DOM 侧的钩子不是 import 边。")
    P("      ⇒ 一个 `.ts` 只要没人 import 它就确实不会被加载（文件级结论**成立**），")
    P("      但**文件内某个导出**有没有人用，本量具不答（定义 (1) 的射程）。")
    P("   3. **计算出来的 `import(...)`** —— 下面这些文件的出边静态不全：")
    dyn = [r.rel for r in pop.values() if any("计算出来" in n for n in r.notes)]
    P(f"      {' · '.join('`'+d+'`' for d in dyn) if dyn else '（现打：0 个）'}")
    P("   4. **Rust 的 trait 分派 / `#[no_mangle]` / 宏里拼出来的路径** —— 引用边靠 `leaf::`")
    P("      正则认，认不出宏展开后才出现的路径。")
    P("  ⇒ 甲档里带 `mod-decl` 记号的项**尤其**要人看一眼：它被编进去了，只是没人 `use` 它。")

    # ── 面⑨ 已登记未扫面 ────────────────────────────────────────────────
    P("\n" + "─" * 79)
    P("【面⑨】**已登记未扫面** —— 明写，不假装扫过")
    P("─" * 79)
    for d in ("src/common", "src/bridge/vendor"):
        fs = [p for p in (REPO / d).rglob("*.rs") if "/target/" not in p.as_posix()]
        P(f"  · `{d}/`：{len(fs)} 个 `.rs` / "
          f"{sum(p.read_text(errors='replace').count(chr(10)) for p in fs):,} 行 —— "
          "不在用户给的人群里")
    css = REPO / "src/styles.css"
    if css.exists():
        P(f"  · `src/styles.css`：{css.stat().st_size:,} 字节 —— `.css` 出本量具射程"
          "（②③ 对 CSS 不成立），CSS 面另有 `css-ledger.vitest.ts`")
    sc = REPO / "src/bridge/tauri.sidecar.conf.json"
    P(f"  · `src/bridge/tauri.sidecar.conf.json`：{'**还在**' if sc.exists() else '已无'} —— "
      "条 67 删的是 `src/backend/sidecars/` 的 `.rs`，这个 `.json` 是同族残留。")
    P("    ⚠ 点名但**不判**（`.json` 出射程），也**不改**（本轮只读）。")

    # ── 面⑩ 反空真地板 ─────────────────────────────────────────────────
    # ── 面⑪ 活体标定 ───────────────────────────────────────────────────
    P("\n" + "─" * 79)
    P("【面⑪】活体标定 —— 拿**代码自己写的那句话**当标准答案，验 ② 的判定")
    P("  （`甲` 档今天是 0 项，而「0 项」与「尺子卡住了」长得一样 ⇒ 先看这条过不过）")
    P("─" * 79)
    for target, claim_file, msg, okk in st["calib"]:
        P(f"  {'✔' if okk else '✘'} `{target}`  ← 自陈住 `{claim_file}`")
        P(f"      {msg}")
    P("  ⚠ 本脚本**不存那句话的副本**，从源文件里读 ⇒ 它被改/删，这条当场红。")

    P("\n" + "─" * 79)
    P("【面⑩】反空真地板 —— 扫到 0 项是**失败**，不是静默绿")
    P("─" * 79)
    bad = [] if all(ok for *_, ok in st["calib"]) else ["live_calibration"]
    for k, floor in FLOORS.items():
        v = metrics[k]
        ok = v >= floor
        if not ok:
            bad.append(k)
        P(f"    {'✔' if ok else '✘'} {k:<20} = {v:>9,}   地板 {floor:,}")

    if args.json:
        blob = dict(metrics=metrics, floors_failed=bad,
                    jia=[dict(rel=r.rel, prod=r.prod, total=r.total,
                              gates=sorted(r.gates), notes=r.notes) for r in jia],
                    yi=[dict(rel=r.rel, prod=r.prod, total=r.total,
                             callers=sorted(r.ext), all_callers=sorted(r.callers),
                             kinds=sorted(r.caller_kinds),
                             gates=sorted(r.gates)) for r in yi],
                    bing=[dict(name=n, doc=d, anchor=a, members=m) for n, d, a, m in st["shelved"]],
                    weak=[dict(rel=r.rel, prod=r.prod, via=r.design[0][1], dir=r.design[0][2])
                          for r in weak],
                    judged=[dict(rel=r.rel, total=r.total) for r in jl])
        print("\n@@JSON@@")
        print(json.dumps(blob, ensure_ascii=False, indent=1))

    if args.addresses:
        P("\n" + "═" * 79)
        P(" 逐项住址 · 全量（含 T5 弱覆盖全列）")
        P("═" * 79)
        for r in sorted(pop.values(), key=lambda r: (r.bucket, -r.prod)):
            d = f"{r.design[0][0]}←{r.design[0][1]}:{r.design[0][2]}" if r.design else "零提及"
            P(f"  {r.bucket} {r.prod:>5}行 {r.rel:<52} ① {d:<44} ② 族外{len(r.ext)}/文件{len(r.callers)}")

    if bad:
        P(f"\n🔴 空转：{', '.join(bad)} 触地板 ⇒ 尺子没切到东西。退出码 3。")
        return 3
    P(f"\n✔ {len(FLOORS)} 条地板全过。")
    return 0


# ═══════════════════════════════════════════════════════════════════════════
#  反空真死值验
# ═══════════════════════════════════════════════════════════════════════════
def selftest() -> int:
    P("反空真死值验 —— 五刀，每刀点名它该让哪一条判据当场红")
    ok = True

    class A:
        json = False
        addresses = False

    base = run(want_gates=False)
    P(f"基线（供各刀对照）：人群 {base['metrics']['pop_files']} · 判据层 "
      f"{base['metrics']['judged_files']} · 甲+乙 {base['metrics']['cand_total']}")

    P("\n刀 1：语料换成空目录 ⇒ 必须报空转")
    try:
        st = run(pop_lister=lambda: iter(()), pop_reader=lambda r: "",
                 design_loader=load_design, want_gates=False)
        bad = [k for k, f in FLOORS.items() if st["metrics"][k] < f]
        P(f"   空语料下触地板 {len(bad)} 条（期望 > 0）")
        if not bad:
            P("   ✘ 空语料还能绿 ⇒ 地板没在决定读数。")
            ok = False
        else:
            P("   ✔")
    except SystemExit as e:
        P(f"   ✔（提前红：{str(e)[:60]}…）")

    P("\n刀 2a：设计正文清空 ⇒ 丙档那条「裁定锚文本」必须当场红（它不许有副本）")
    try:
        run(design_loader=lambda: ({"空.md": ""}, {}), want_gates=False)
        P("   ✘ 设计正文清空了，丙档还能报数 ⇒ 那条裁定在脚本里存了副本。")
        ok = False
    except SystemExit:
        P("   ✔ 当场红：丙档只认设计文档里那句原话，脚本里没有第二份")

    P("\n刀 2b：设计正文清空（丙档表暂时摘掉）⇒ 候选必须暴涨到「人群全体」")
    saved_shelved = SHELVED[:]
    SHELVED.clear()
    try:
        st2 = run(design_loader=lambda: ({"空.md": ""}, {}), want_gates=False)
    finally:
        SHELVED.extend(saved_shelved)
    P(f"   设计清空后 甲+乙 = {st2['metrics']['cand_total']}（人群 {st2['metrics']['pop_files']}）")
    if st2["metrics"]["cand_total"] < st2["metrics"]["pop_files"] * 0.9:
        P("   ✘ 设计正文清空了，候选却没暴涨 ⇒ ① 那一问不是靠设计正文在判。")
        ok = False
    else:
        P("   ✔ ① 确实由设计正文决定")
    if st2["metrics"]["bucket_ding"] >= FLOORS["bucket_ding"]:
        P("   ✘ 设计清空后「丁」仍过地板 ⇒ 地板抓不住这刀。")
        ok = False
    else:
        P("   ✔ `bucket_ding` 地板当场触底")

    # ── 刀 3~5：逐个关掉 ② 的一条承重步骤，各配一条必须当场红的判据 ──────────
    #  🔴 第一版是「三样一起关」，结果三样全关标定照样过 —— 那说明那一刀
    #     根本没碰到承重的那一步。⇒ 改成逐个关，每刀点名它该让谁红。
    def with_mut(name, fn):
        MUT[name] = True
        try:
            return fn()
        finally:
            MUT[name] = False

    P("\n刀 3：把 `mod x;` **当成一条边** ⇒ 条 67 的标定必须失真")
    P("   （定义 (4) 那条「声明不是调用方」就是这一刀在验）")
    rc = with_mut("mod_as_edge", lambda: backtest(quiet=True))
    if rc == 0:
        P("   ✘ `mod` 算成边了，标定还过 ⇒ 那条取舍没在决定读数。")
        ok = False
    else:
        P("   ✔ 标定当场失真：`main.rs` 一步走到 `sidecars/`，整族变「可达」")

    P("\n刀 4：**不剥 `#[cfg(test)]`** ⇒ 「判据层」轨必须塌掉（判据被当成生产代码）")
    st4 = with_mut("no_cfg_strip", lambda: run(want_gates=False))
    n4 = st4["metrics"]["judged_files"]
    P(f"   不剥之后判据层 = {n4} 个（基线 {base['metrics']['judged_files']}；"
      f"地板 {FLOORS['judged_files']}）")
    if n4 >= FLOORS["judged_files"]:
        P("   ✘ 不剥 cfg(test) 判据层还过地板 ⇒ 剥这一步没在承重。")
        ok = False
    else:
        P("   ✔ 判据层当场塌到地板下 —— 那三万行判据会被当成生产代码灌进候选")

    P("\n刀 5：**不遮注释** ⇒ 活体标定必须当场红")
    P("   （`control/ccm/mod.rs` 的文档注释里有 `crate::plugin::probe::tests::…`，")
    P("    不遮注释就会凭这一句把 `probe.rs` 判成可达 —— 而它自陈零生产调用方）")
    st5 = with_mut("no_comment_mask", lambda: run(want_gates=False))
    bad5 = [c for c in st5["calib"] if not c[3]]
    P(f"   不遮之后活体标定失败 {len(bad5)}/{len(st5['calib'])} 条")
    if not bad5:
        P("   ✘ 不遮注释活体标定还过 ⇒ 遮注释这一步没在承重。")
        ok = False
    else:
        P("   ✔ 活体标定当场红")

    P("\n" + ("✔ 五刀全过：本量具报得出空转、① 真由设计正文决定、"
              "② 的三条承重步骤**各自**都在承重。"
              if ok else "🔴 有刀没过 ⇒ 尺子是坏的，读数不可信。"))
    return 0 if ok else 3


# ═══════════════════════════════════════════════════════════════════════════
#  标定：条 67 那一刀（`--backtest`）
# ═══════════════════════════════════════════════════════════════════════════
BACKTEST_COMMIT = "4f472059"   # 「条 67：删掉不在现在设计里的那条交付路径（sidecars/，2 008 行）」
BACKTEST_TARGETS = (
    "src/backend/sidecars/mod.rs",
    "src/backend/sidecars/codepicture/mod.rs",
    "src/backend/sidecars/codepicture/acquire.rs",
    "src/backend/sidecars/codepicture/fetch.rs",
)


def git(*a) -> str:
    return subprocess.run(["git", "-C", str(REPO), *a], capture_output=True,
                          text=True, check=True).stdout


def backtest(quiet=False) -> int:
    """把语料换成 `4f472059^`（删 `sidecars/` 之前那一刻），验**已知答案**能复现。

    已知答案（用户逐字 · 条 67）：那一族「命令面 0 条、生产调用方 0 个」。
    ⚠ 它当时**有** `mod sidecars;`（在 `main.rs`）。⇒ 只要把 `mod x;` 当成一条边，
    `main.rs` 一步就走到 `sidecars/`，整族立刻显示成「可达」而逃过普查。
    `--selftest` 的刀 3 就是把这个开关打开，验标定会不会当场失真。
    """
    ref = BACKTEST_COMMIT + "^"

    def lister():
        for ln in git("ls-tree", "-r", "--name-only", ref).splitlines():
            yield ln.strip()

    cache = {}

    def reader(rel):
        if rel not in cache:
            cache[rel] = git("show", f"{ref}:{rel}")
        return cache[rel]

    st = run(pop_lister=lister, pop_reader=reader, want_gates=False)

    pop = st["pop"]
    reach = st["reach"]
    for r in pop.values():
        r.ext = external_callers(r)
    if not quiet:
        P("\n" + "═" * 79)
        tag = " ⟪变异：" + "+".join(k for k, v in MUT.items() if v) + "⟫" if any(MUT.values()) else ""
        P(f" 标定：条 67 那一刀（语料 = `{ref}`）{tag}")
        P("═" * 79)
    rc = 0
    missing = [t for t in BACKTEST_TARGETS if t not in pop]
    if missing:
        if not quiet:
            P(f"  ✘ 目标不在人群里：{missing} ⇒ 语料或人群定义坏了")
        return 4
    total = 0
    for t in BACKTEST_TARGETS:
        r = pop[t]
        total += r.prod
        n = len(r.ext)
        kinds = "/".join(sorted(r.caller_kinds)) or "无"
        good = t not in reach and "命令面" not in r.caller_kinds
        if not good:
            rc = 4
        if not quiet:
            P(f"  {'✔' if good else '✘'} `{t}` {r.prod:>4}行 · "
              f"{'**入口走不到**' if t not in reach else '🔴 入口走得到'}"
              f"（族外调用方 {n} · 文件级 {len(r.callers)} · {kinds}）"
              + ("" if good else f" ← 期望「走不到」；误算来自 {sorted(r.ext)[:3]}"))
    if not quiet:
        P(f"  该族生产行合计 {total}（条 67 逐字记的是「2 008 行」总行；"
          f"本量具的 `prod_lines` 是去空行 ＋ 剥 cfg(test) 后的数，两者不是同一个量）")
        P("  ✔ 标定通过：已知为「零生产调用方」的族，本量具也报「入口走不到」。" if rc == 0
          else "  🔴 标定失败：已知答案没复现 ⇒ ② 那一问的读数不可信。")
    return rc


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description="条 78 S26-A1：不在设计里的代码 · 全仓普查")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--addresses", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--backtest", action="store_true")
    a = ap.parse_args()
    if a.selftest:
        sys.exit(selftest())
    if a.backtest:
        sys.exit(backtest())
    sys.exit(main_report(a))
