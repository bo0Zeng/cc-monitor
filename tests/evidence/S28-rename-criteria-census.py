#!/usr/bin/env python3
# ruff: noqa: E501
"""步 8（全仓改名）的**判据影响面**普查 —— 只产地图，不产改动。

住址：`<仓根>/tests/evidence/S28-rename-criteria-census.py`
服务的设计篇：`调研/设计/90-命名清理与前后端归属.md` `§1.1`/`§1.4`
读数落点：`调研/真相源/95-步8改名-判据影响面地图.md` ＋ `tests/evidence/S28-readings.md`

跑法（仓根下）：
    python3 tests/evidence/S28-rename-criteria-census.py             # 主报告（四面）
    python3 tests/evidence/S28-rename-criteria-census.py --addresses # 逐处住址
    python3 tests/evidence/S28-rename-criteria-census.py --silent    # 🔴 只印「可能静默」那一档
    python3 tests/evidence/S28-rename-criteria-census.py --json      # 机读
    python3 tests/evidence/S28-rename-criteria-census.py --selftest  # 反空真死值验（扫空树 ⇒ 失败）

退出码：
    0 = 每一格都切到了东西（地板全过）
    3 = **有格子空转**（触地板）—— 这是失败，不是「0 命中的绿」
    4 = 自检失败（`--selftest`：扫空树却没红 ⇒ 地板是坏的）

═══════════════════════════════════════════════════════════════════════════════
 🔴 一、这把尺子回答哪一句话 —— 写死在这里
═══════════════════════════════════════════════════════════════════════════════

改名会动两种东西，**只有一种编译器替你看着**：

  · **标识符**（`daemon_kill` → `backend_kill`）—— 改漏一处 ⇒ `cargo` / `tsc` 当场红。
    ⇒ **不在本量具射程内。** 编译器是比任何尺子都可靠的判据。

  · **字符串字面量**（`"daemon_kill::daemon_kill("` · `"embedded-daemons"` · `"中转没在跑"`）
    —— 改漏 / 改错 **编译器一个字都不说**。它们是本仓「登记表 ＋ 扫描型判据」这一族的
    全部承重面：针（needle）· 住址（path）· 数据行（row）· 报文（message）。
    ⇒ **这才是本量具的人群。**

⇒ 本脚本**不含任何改名动作**，也不输出「建议改成 X」。它输出的是**待判清单**，
  第四列（会不会静默）要人去判 —— 本脚本只**标出可疑形状**，不替人下结论。

═══════════════════════════════════════════════════════════════════════════════
 🔴 二、定义 —— 五个词各钉一次，改定义必须改这段注释
═══════════════════════════════════════════════════════════════════════════════

**(1) 六个旧名（`NAMES`）**

  `设计/90 §1.1` 那张表里**要改**的六行，逐字：
    `daemon` · `cc-monitor-remote` · `sidecar` · `relay` ＋ `中转` · `proto` · `rbind`
  **不含** `ccm`（用户敲的命令名）与 `monitor`（产品名）—— 那两行 `§1.1` 明写不改。

  ⚠ `proto` 用**带边界**的匹配（前后不是字母），否则 `protocol` / `prototype` /
  `PROTOCOL` 会把读数灌成几百。`remote-daemon-proto` 另立一条（它是整串）。
  ⚠ `relay` 不带边界（`relay_injection_for` / `RELAY_PORT` 都算）。

**(2) 人群（criteria surface）**

  一处 = **一个字符串字面量里的一次命中**（同一字面量命中两个名字算两处）。
  只在下面这些树里取 —— 理由逐条：
    · `tests/**` 的 `.rs`        —— 本仓的 Rust 判据全住这里（`设计/16` 步 7c 剖分之后）
    · `tests/**` 的 `.ts/.mts`   —— vitest / tsx 判据
    · `tests/**` 的 `.sh`        —— e2e 与闸门脚本
    · `tests/evidence/*.py`      —— 尺子（其中 5 把是**活门**，见 `LIVE_RULERS`）
    · `src/frontend/shell/src/**` 的 `.rs` ＋ `src/backend/**` 的 `.rs`
                                 —— 🔴 **登记表的数据行住在生产段**（`cross_half_edge_registry.rs`
                                    的 `CROSS_EDGES` · `tool_registry.rs` 的 `repo_path` 都在这里），
                                    判据本体在 `tests/` 里读它们
    · `.github/workflows/*.yml`  —— 云端门的 job 名 / 步骤名，被 `README.md` 与 `K-R122` 对拍
    · `package.json`             —— 套件名，被 `K-P2-F-suites.py` 与 `ci.yml` 对拍

  **不在人群里**：`src/**/*.ts` 前端生产源（那是被判的对象，不是判据）· `调研/**`（散文）·
  `node_modules/` · `.build/` · `coverage/` · `src/frontend/ui/generated/`（生成物，重生成即改）。

**(3) 四形（`WHAT`）—— 任务书那四档，逐处只落一档**

  | 档 | 判法（机械） | 改名后该怎么动 |
  |---|---|---|
  | `row`  数据行 | 字面量落在 `const … &[` / `= [` 表体内，且不是针也不是路径 | 跟着改字面量 |
  | `needle` 针  | 同一行上有匹配动词（见 `MATCHERS`）吃着它 | 改名会让它失配 |
  | `path` 住址  | 形如仓内相对路径（带已知后缀或以 `/` 收尾） | 跟着文件一起改 |
  | `prose` 散文 | 该行以注释记号开头，或字面量只出现在报文里 | 看语义 |

  ⚠ 一处只落一档，优先级 `path` > `needle` > `row` > `prose` ——
  理由：住址错了连文件都读不到（最硬的失效），针失配次之。

**(4) 🔴 静默嫌疑（`RISK`）—— 本量具唯一真正的产物**

  能红的那些不用地图也会被发现。本量具去标的是**改完之后悄悄不判了**的形状。
  五个标，**都是"嫌疑"不是"结论"**（要人去读那一处才能定）：

  | 标 | 机械判法 | 为什么可疑 |
  |---|---|---|
  | `neg`   负向针 | 同行有 `!x.contains(` / `not.toContain(` / `assert!(!` | 钉的词消失了 ⇒ 断言**更容易为真** ⇒ 恒绿 |
  | `slice` 切片针 | 同行有 `skip_while/take_while/find/rfind/indexOf/slice/split/splitn/partition` | 针失配 ⇒ 抽到 0 ⇒ 下游对拍两边都空 ⇒ 恒绿（`F23` 那一族） |
  | `gone`  住址已馊 | `path` 档，但该路径**今天盘上不存在** | 已经在空转，改名只会让它更馊 |
  | `xlang` 跨语言 | 同一字面量同时出现在 `.rs` 与（`.ts`/`.py`/`.sh`/`.yml`/`.json`）里 | 两侧同值常量，**只改一边不报错** |
  | `dead`  死尺子 | 落在 `tests/evidence/*.py` 里，而那把尺子**不是活门**（不在 `LIVE_RULERS`） | 改名后它更不会被跑到，读数会一直腐着 |

  **反标**（`guarded`）：同一个 `#[test]` / `it(` 块里 **20 行内**有反空真断言
  （`is_empty` / `toBeGreaterThan` / `len() >=` / `.expect(` / `找不到` / `抽取器坏了`）。
  有反标的 `slice` / `neg` 不是「不会静默」，是「**这个仓已经替它装过地板**」——
  仍要人看一眼地板钉的是不是同一件事。

**(5) 活门（`LIVE_RULERS`）**

  `tests/evidence/*.py` 里**真被跑到**的那几把，现打自 `tests/scripts/gate.sh` 与
  `.github/workflows/ci.yml` 与 `tests/e2e/*.sh` 的**真调用行**（不是注释里提到的名字）。
  本量具每趟**现算**这张表并印出来 —— 不许手抄。

═══════════════════════════════════════════════════════════════════════════════
 🔴 三、这把尺子**买不到**什么 —— 如实写
═══════════════════════════════════════════════════════════════════════════════

 · **它不读语义。** `prose` 那一档里哪些该改、哪些是引文/墓碑/外部契约名，要人去判
   （`tests/frontend/shell/doc_claim_registry_daemon_wording_registry.rs` 的 `EXEMPT` 表就是
   人判完之后的落档，本量具只把它当数据行数出来）。
 · **它只看单行。** 跨行的匹配表达式（针在上一行、字面量在下一行）会被判成 `row` 或
   `prose`。⇒ `needle` 这一档的数是**下界**。
 · **`xlang` 只认逐字相等的整串字面量。** 一侧写 `"daemon"`、另一侧写 `"daemon_kill"`
   这种「包含关系」的跨语言对拍它看不见。⇒ `xlang` 那一档也是**下界**。
 · **它不跑任何判据。** 「这一处改了会红」是**推断**，不是实测；真要落定得跑一趟
   `cargo test` / `npm run test:dom`。
 · **注释里的名字它默认不数**（`--prose-too` 才数）—— `//!` 头注里的 `daemon` 有几千处，
   混进来会把真正承重的字面量淹掉。
"""

from __future__ import annotations

import json
import os
import re
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(os.environ.get("S28_ROOT") or Path(__file__).resolve().parents[2])

# ── (1) 六个旧名 ────────────────────────────────────────────────────────────
NAMES: dict[str, re.Pattern[str]] = {
    "cc-monitor-remote": re.compile(r"cc[-_]monitor[-_]remote", re.I),
    "daemon": re.compile(r"daemon", re.I),
    "sidecar": re.compile(r"sidecar", re.I),
    "relay": re.compile(r"relay", re.I),
    "中转": re.compile(r"中转"),
    # 带边界 —— 否则 protocol / prototype / PROTOCOL 会灌进来
    "proto": re.compile(r"(?<![A-Za-z])proto(?![A-Za-z])", re.I),
    "rbind": re.compile(r"rbind", re.I),
}

# ── (2) 人群 ────────────────────────────────────────────────────────────────
POP: list[tuple[str, tuple[str, ...]]] = [
    ("tests", (".rs", ".ts", ".mts", ".mjs", ".sh", ".py")),
    ("src/frontend/shell/src", (".rs",)),
    ("src/backend", (".rs",)),
    (".github/workflows", (".yml",)),
]
EXTRA_FILES = ("package.json",)
EXCLUDE_DIR_PARTS = {
    "node_modules", ".build", "coverage", "target", "vendor", "gen", "generated",
    "__fixtures__", "snapshots",
}

# ── (3) 四形 ────────────────────────────────────────────────────────────────
MATCHERS = (
    "contains", "contains_word", "find_pinned", "pin_line", "starts_with", "ends_with",
    "rfind", "find(", "strip_prefix", "strip_suffix", "splitn", "split(", "split_once",
    "indexOf", "lastIndexOf", "includes(", "toContain", "toMatch", "match(", "matchAll",
    "re.search", "re.match", "re.findall", "re.compile", "grep ", "rg ", "skip_while",
    "take_while", "position(", "any(|", "filter(|", "startswith", "endswith", " in src",
    " in text", " in raw", " in body",
)
PATH_SUFFIX = (
    ".rs", ".ts", ".mts", ".mjs", ".md", ".sh", ".py", ".json", ".yml", ".yaml",
    ".toml", ".css", ".html", ".tsv", ".snapshot",
)
TABLE_OPEN = re.compile(r"(const\s+[A-Z_0-9]+\s*:|=\s*&?\[|:\s*&\[|= \[)")

# ── (4) 静默嫌疑 ────────────────────────────────────────────────────────────
NEG = re.compile(r"(!\s*[A-Za-z_0-9.()&\[\]:]*\.(contains|contains_word|starts_with|ends_with|includes|match)|not\.toContain|not\.toMatch|assert!\(\s*!|expect\(\s*!)")
SLICE = re.compile(r"(skip_while|take_while|\.find\(|\.rfind\(|indexOf|lastIndexOf|\.slice\(|splitn|split_once|\.split\(|partition|\.position\()")
GUARD = re.compile(
    r"(is_empty|isEmpty|toBeGreaterThan|toHaveLength|len\(\)\s*>=|len\(\)\s*>|\.len\(\)\s*,|"
    r"\.expect\(|unwrap_or_else|抽取器坏了|抽不到|找不到|零命中|空转|遍历坏了|解析坏了|空真)"
)
GUARD_WINDOW = 20

# 语言各自的字面量形状
LIT_RS = re.compile(r'r#"(?:.|\n)*?"#|"(?:[^"\\\n]|\\.)*"')
LIT_TS = re.compile(r'`(?:[^`\\]|\\.)*`|"(?:[^"\\\n]|\\.)*"|\'(?:[^\'\\\n]|\\.)*\'')
LIT_PY = re.compile(r'"""(?:.|\n)*?"""|\'\'\'(?:.|\n)*?\'\'\'|"(?:[^"\\\n]|\\.)*"|\'(?:[^\'\\\n]|\\.)*\'')
LIT_SH = re.compile(r'"(?:[^"\\\n]|\\.)*"|\'[^\']*\'')
LIT_YML = LIT_SH
LIT_JSON = re.compile(r'"(?:[^"\\\n]|\\.)*"')

COMMENT_HEAD = re.compile(r"^\s*(///|//!|//|#(?!\!\[)|\*|/\*)")


def lit_re(suffix: str) -> re.Pattern[str]:
    return {
        ".rs": LIT_RS, ".ts": LIT_TS, ".mts": LIT_TS, ".mjs": LIT_TS,
        ".py": LIT_PY, ".sh": LIT_SH, ".yml": LIT_YML, ".yaml": LIT_YML,
        ".json": LIT_JSON,
    }.get(suffix, LIT_RS)


@dataclass
class Hit:
    rel: str
    line: int
    name: str
    what: str
    lit: str
    src_line: str
    risks: list[str] = field(default_factory=list)
    guarded: bool = False

    def key(self) -> str:
        return f"{self.rel}:{self.line}"


# ── 活门：现算，不手抄 ──────────────────────────────────────────────────────
def live_rulers() -> set[str]:
    """真被跑到的 `tests/evidence/*.py`：从真调用行现读（注释里提到的名字不算）。"""
    out: set[str] = set()
    call = re.compile(r"(?<![#`])\bpython3?\s+(?:-\w+\s+)*(tests/evidence/[A-Za-z0-9_.-]+\.py)")
    srcs = [ROOT / "tests" / "scripts" / "gate.sh", ROOT / ".github" / "workflows" / "ci.yml"]
    srcs += sorted((ROOT / "tests" / "e2e").glob("*.sh")) if (ROOT / "tests" / "e2e").is_dir() else []
    for p in srcs:
        if not p.is_file():
            continue
        for raw in p.read_text(encoding="utf-8", errors="replace").splitlines():
            stripped = raw.lstrip()
            if stripped.startswith("#"):
                continue
            for m in call.finditer(raw):
                out.add(m.group(1))
    return out


_BASENAMES: dict[Path, set[str]] = {}


def basenames(root: Path) -> set[str]:
    """全仓（去掉 node_modules 等）所有文件的 basename —— 给「片段式住址」查存在性。"""
    if root in _BASENAMES:
        return _BASENAMES[root]
    out: set[str] = set()
    for p in root.rglob("*"):
        parts = set(p.relative_to(root).parts)
        if parts & EXCLUDE_DIR_PARTS or ".git" in parts:
            continue
        if p.is_file():
            out.add(p.name)
    _BASENAMES[root] = out
    return out


def population(root: Path) -> list[Path]:
    files: list[Path] = []
    for sub, suffixes in POP:
        base = root / sub
        if not base.is_dir():
            continue
        for p in base.rglob("*"):
            if not p.is_file() or p.suffix not in suffixes:
                continue
            if set(p.relative_to(root).parts[:-1]) & EXCLUDE_DIR_PARTS:
                continue
            files.append(p)
    for name in EXTRA_FILES:
        if (root / name).is_file():
            files.append(root / name)
    return sorted(files)


RUNNER = re.compile(r"^(bash|sh|python3?|node|npx\s+tsx|npx|tsx|cargo\s+\S+)\s+")
# 仓根锚：只有以这些段打头的路径才拿去盘上查存在性
ROOT_ANCHORS = ("src/", "tests/", "doc/", "scripts/", ".github/", "hooks/", "audits/",
                "e2e/", "调研/", "coverage/", "./", "crates/")
# 本仓的三个「根」—— 登记表里的相对住址按哪个根解读，要看它住在谁家
ALT_ROOTS = (".", "src/frontend/shell", "src/backend", "src/frontend/shell/src")


def path_body(body: str) -> str | None:
    """把字面量归一成「一条路径」；不是路径就回 None。"""
    b = body.strip()
    b = RUNNER.sub("", b).strip()
    b = b.split()[0] if b and " " in b else b
    if not b:
        return None
    if b.endswith(PATH_SUFFIX) or (("/" in b) and b.endswith("/")):
        return b
    return None


def looks_like_path(body: str) -> bool:
    return path_body(body) is not None


def classify(body: str, src_line: str) -> str:
    stripped = src_line.lstrip()
    if looks_like_path(body):
        return "path"
    if any(m in src_line for m in MATCHERS) and not COMMENT_HEAD.match(src_line):
        return "needle"
    if COMMENT_HEAD.match(src_line):
        return "prose"
    if TABLE_OPEN.search(src_line) or stripped.startswith(('"', "'", "(", "&[", "[")):
        return "row"
    return "prose"


def scan(root: Path, prose_too: bool = False) -> tuple[list[Hit], dict[str, int]]:
    hits: list[Hit] = []
    stats = {"files": 0, "files_with_hits": 0, "literals": 0}
    live = live_rulers() if root == ROOT else set()
    for p in population(root):
        rel = p.relative_to(root).as_posix()
        try:
            src = p.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        stats["files"] += 1
        lines = src.splitlines()
        rx = lit_re(p.suffix)
        before = len(hits)
        for m in rx.finditer(src):
            lit = m.group(0)
            stats["literals"] += 1
            line_no = src.count("\n", 0, m.start()) + 1
            src_line = lines[line_no - 1] if 0 < line_no <= len(lines) else ""
            if not prose_too and COMMENT_HEAD.match(src_line):
                continue
            body = lit.strip('"\'`')
            for name, pat in NAMES.items():
                if not pat.search(lit):
                    continue
                # daemon 命中时 cc-monitor-remote 已单列，避免双计
                if name == "daemon" and NAMES["cc-monitor-remote"].search(lit):
                    pass
                what = classify(body, src_line)
                h = Hit(rel=rel, line=line_no, name=name, what=what, lit=lit[:180],
                        src_line=src_line.strip()[:200])
                # ── 静默嫌疑 ──
                if NEG.search(src_line):
                    h.risks.append("neg")
                if SLICE.search(src_line) and what == "needle":
                    h.risks.append("slice")
                if what == "path":
                    cand = path_body(body) or ""
                    if cand.startswith(ROOT_ANCHORS):
                        # 仓根锚的路径：盘上查存在性。
                        # 🔴 本仓有**三个**「根」：仓根 · `src/frontend/shell/`（bridge crate 的
                        # 登记表写 `src/accounts.rs` 指的是它）· `src/backend/`。
                        # 只按仓根查会把 bridge 那一族的登记表全判成馊（09-19 现打 8 处假阳）。
                        # ⚠ 变量名**不许**叫 `rel` —— 09-19 第一版就是在这里把外层的
                        #   `rel`（文件住址）影子掉了，于是同一份文件里这一处之后的每一条
                        #   `Hit` 都挂上了**别人的住址**。它一个字都不红（类型对、值也像路径），
                        #   是 `--addresses` 印出「`doc/IPC-PROTOCOL.md` 里有 Python 代码行」
                        #   才看出来的。本条注释是那次的墓碑。
                        pcand = cand.lstrip("./")
                        if not any((root / pre / pcand).exists() for pre in ALT_ROOTS):
                            h.risks.append("gone")
                    else:
                        # 非仓根锚（只给了片段 / basename）：全仓找同名文件
                        base = cand.rsplit("/", 1)[-1]
                        if base and base not in basenames(root):
                            h.risks.append("gone")
                if rel.startswith("tests/evidence/") and rel.endswith(".py") and rel not in live:
                    h.risks.append("dead")
                lo, hi = max(0, line_no - 1 - GUARD_WINDOW), min(len(lines), line_no + GUARD_WINDOW)
                h.guarded = bool(GUARD.search("\n".join(lines[lo:hi])))
                hits.append(h)
        if len(hits) > before:
            stats["files_with_hits"] += 1
    # ── xlang：同一字面量横跨 .rs 与别的语言 ──
    by_lit: dict[str, set[str]] = {}
    for h in hits:
        by_lit.setdefault(h.lit.strip('"\'`'), set()).add(Path(h.rel).suffix)
    for h in hits:
        exts = by_lit.get(h.lit.strip('"\'`'), set())
        if ".rs" in exts and (exts - {".rs"}):
            h.risks.append("xlang")
    return hits, stats


# ── 地板（反空真）──────────────────────────────────────────────────────────
FLOORS = {
    "files": 200,
    "hits": 300,
    "names_covered": 5,        # 七个 key 里至少 5 个有命中
    "forms_covered": 4,        # 四形都得有
    "risky": 10,               # 「可能静默」那一档不许为空
    "live_rulers": 3,          # 活门表不许抽空
}


def floors_report(hits: list[Hit], stats: dict[str, int], live: set[str]) -> list[str]:
    fails: list[str] = []
    names = {h.name for h in hits}
    forms = {h.what for h in hits}
    risky = [h for h in hits if set(h.risks) - {"dead"}]
    if stats["files"] < FLOORS["files"]:
        fails.append(f"人群只扫到 {stats['files']} 份文件（地板 {FLOORS['files']}）—— 遍历坏了")
    if len(hits) < FLOORS["hits"]:
        fails.append(f"只抽到 {len(hits)} 处命中（地板 {FLOORS['hits']}）—— 抽取器坏了")
    if len(names) < FLOORS["names_covered"]:
        fails.append(f"七个名字里只有 {len(names)} 个有命中（地板 {FLOORS['names_covered']}）：{sorted(names)}")
    if len(forms) < FLOORS["forms_covered"]:
        fails.append(f"四形只切到 {sorted(forms)}（地板 {FLOORS['forms_covered']} 形）—— 分类器坏了")
    if len(risky) < FLOORS["risky"]:
        fails.append(f"「可能静默」那一档只有 {len(risky)} 处（地板 {FLOORS['risky']}）—— 嫌疑判法坏了")
    for r in ("neg", "slice", "gone", "xlang", "dead"):
        if not any(r in h.risks for h in hits):
            fails.append(f"嫌疑标 `{r}` 一处都没切到 —— 那条判法坏了（它在 09-19 现打都非空）")
    if len(live) < FLOORS["live_rulers"]:
        fails.append(f"活门表只现算出 {len(live)} 把尺子（地板 {FLOORS['live_rulers']}）：{sorted(live)}")
    return fails


def main() -> int:
    args = set(sys.argv[1:])

    if "--selftest" in args:
        # 🔴 反空真死值验：扫一棵**空树**，地板必须当场红。
        with tempfile.TemporaryDirectory() as td:
            empty = Path(td)
            for sub, _ in POP:
                (empty / sub).mkdir(parents=True, exist_ok=True)
            h, s = scan(empty)
            fails = floors_report(h, s, set())
            print("── `--selftest`：扫一棵空树 ──")
            print(f"  命中 {len(h)} 处 · 文件 {s['files']} 份 · 触地板 {len(fails)} 条")
            for f in fails:
                print(f"    × {f}")
            if not fails:
                print("🔴 SELFTEST FAIL：空树没触地板 —— 地板是假的")
                return 4
            print("✅ SELFTEST OK：空树当场红（0 命中**不是**绿）")
        # 第二刀：把名字表换成一个真空的名字，真树上也必须红
        saved = dict(NAMES)
        try:
            NAMES.clear()
            NAMES["绝无此名zzz"] = re.compile("绝无此名zzz")
            h2, s2 = scan(ROOT)
            f2 = floors_report(h2, s2, live_rulers())
            print(f"── `--selftest` 第二刀：真树 + 空名字表 ⇒ 命中 {len(h2)} 处 · 触地板 {len(f2)} 条")
            if not f2:
                print("🔴 SELFTEST FAIL：名字表掏空了却没红")
                return 4
            print("✅ SELFTEST OK：名字表掏空 ⇒ 当场红")
        finally:
            NAMES.clear()
            NAMES.update(saved)
        return 0

    live = live_rulers()
    hits, stats = scan(ROOT, prose_too="--prose-too" in args)
    fails = floors_report(hits, stats, live)

    if "--json" in args:
        print(json.dumps({
            "root": str(ROOT),
            "stats": stats,
            "live_rulers": sorted(live),
            "floors_failed": fails,
            "hits": [h.__dict__ for h in hits],
        }, ensure_ascii=False, indent=1))
        return 3 if fails else 0

    risky = [h for h in hits if set(h.risks) - {"dead"}]

    if "--silent" in args:
        print("🔴 「可能静默」档 —— 逐处（标 `guarded` 的是本仓已装过地板的）")
        for h in sorted(risky, key=lambda x: (x.rel, x.line)):
            g = " guarded" if h.guarded else ""
            print(f"{h.key():68} {h.name:18} {h.what:7} [{'+'.join(h.risks)}{g}]")
            print(f"    {h.src_line}")
        print(f"\n合计 {len(risky)} 处（其中已装地板 {sum(1 for h in risky if h.guarded)} 处）")
        return 3 if fails else 0

    if "--addresses" in args:
        for h in sorted(hits, key=lambda x: (x.rel, x.line)):
            r = f" [{'+'.join(h.risks)}]" if h.risks else ""
            print(f"{h.key():68} {h.name:18} {h.what:7}{r}  {h.lit[:90]}")
        return 3 if fails else 0

    # ── 主报告 ──
    print("═" * 78)
    print(f" S28 · 步 8 改名的判据影响面普查   仓根 {ROOT}")
    print("═" * 78)
    print(f"\n人群：{stats['files']} 份文件（{stats['files_with_hits']} 份有命中）· "
          f"扫过 {stats['literals']} 个字符串字面量 · 抽到 {len(hits)} 处命中\n")

    print("── 面①：按名字 ──")
    for name in NAMES:
        sel = [h for h in hits if h.name == name]
        if not sel:
            print(f"  {name:20} 0")
            continue
        forms = {}
        for h in sel:
            forms[h.what] = forms.get(h.what, 0) + 1
        print(f"  {name:20} {len(sel):5}   " + " · ".join(f"{k} {v}" for k, v in sorted(forms.items())))

    print("\n── 面②：按四形 ──")
    for what, desc in (("row", "登记表的数据行（跟着改字面量）"),
                       ("needle", "判据的针（改名会让它失配）"),
                       ("path", "住址（跟着文件一起改）"),
                       ("prose", "散文/报文（看语义）")):
        sel = [h for h in hits if h.what == what]
        print(f"  {what:8} {len(sel):5}  {desc}")

    print("\n── 面③：🔴 静默嫌疑（本量具唯一真正的产物）· 活判据档 ──")
    for risk, desc in (("neg", "负向针：钉的词消失 ⇒ 断言更容易为真 ⇒ 恒绿"),
                       ("slice", "切片针：失配 ⇒ 抽到 0 ⇒ 两边都空 ⇒ 恒绿"),
                       ("gone", "住址今天盘上就不存在 —— 已经在空转"),
                       ("xlang", "跨语言同值字面量 —— 只改一边不报错")):
        sel = [h for h in hits if risk in h.risks and "dead" not in h.risks]
        g = sum(1 for h in sel if h.guarded)
        print(f"  {risk:8} {len(sel):5}  (已装地板 {g:4})  {desc}")
    print(f"  {'合计':8} {len(risky):5}  (已装地板 {sum(1 for h in risky if h.guarded):4})  ← 去重后的处数")
    dead = [h for h in hits if "dead" in h.risks]
    print(f"\n  另档 `dead` {len(dead):5}  落在**不是活门**的 `tests/evidence/*.py` 里 —— ")
    print("            它们不会红也不会拦，但读数会一直腐着（改名后那些数全是旧世界的）")

    print("\n  ⚠ 「已装地板」= 同块 ±20 行内有反空真断言。它**不等于**「不会静默」——")
    print("     还要人去看那条地板钉的是不是同一件事。")

    print("\n── 面④：命中最密的判据文件（前 25）──")
    per: dict[str, int] = {}
    perrisk: dict[str, int] = {}
    for h in hits:
        per[h.rel] = per.get(h.rel, 0) + 1
        if h.risks:
            perrisk[h.rel] = perrisk.get(h.rel, 0) + 1
    for rel, n in sorted(per.items(), key=lambda x: -x[1])[:25]:
        print(f"  {n:5} ({perrisk.get(rel, 0):3} 可疑)  {rel}")

    print(f"\n── 面⑤：活门（现算，不手抄）—— {len(live)} 把 ──")
    for r in sorted(live):
        n = per.get(r, 0)
        print(f"  {r}   命中 {n}")
    dead_py = sorted({h.rel for h in hits if "dead" in h.risks})
    print(f"\n  另有 {len(dead_py)} 份 `tests/evidence/*.py` 有命中但**不是活门**（读数会腐，不会红）")

    print("\n" + "═" * 78)
    if fails:
        print("🔴 触地板（这是失败，不是 0 命中的绿）：")
        for f in fails:
            print(f"   × {f}")
        print("═" * 78)
        return 3
    print("✅ 每一格都切到了东西（地板全过）")
    print("═" * 78)
    return 0


if __name__ == "__main__":
    sys.exit(main())
