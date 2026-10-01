#!/usr/bin/env python3
# ruff: noqa: E501
"""CP1：**存疑带逐条裁决台账** 的量具 ＋ 判据。

住址：`<仓根>/tests/evidence/CP1-copy-verdicts.py`
台账：`<仓根>/tests/evidence/CP1-copy-verdicts.tsv`
要求：存疑带「对账开工前必须逐条裁完」。

跑法（仓根下）：
    python3 tests/evidence/CP1-copy-verdicts.py            # 判：台账人群 == 普查存疑带（两向）
    python3 tests/evidence/CP1-copy-verdicts.py --json     # 同上，机读（vitest 那一侧吃这个）
    python3 tests/evidence/CP1-copy-verdicts.py --dump     # 把今天的存疑带逐条吐成 JSONL（裁的时候用）
    python3 tests/evidence/CP1-copy-verdicts.py --selftest # 死值验（空语料 · 删一行 · 加一条）

退出码：0 = 两向相等且每行裁词合法 · 1 = 不相等 / 裁词不合法 · 3 = 空转（正控没过：普查认不出那一句现造的存疑带样本）

═══════════════════════════════════════════════════════════════════════════════
 一、人群是**借来的**，不是本文件自己定义的
═══════════════════════════════════════════════════════════════════════════════
「存疑带」的定义住在 `K-T68-A1-outward-copy-census.py`（`scan()` 的残差 ＋ `is_declined()`）。
本文件**把那个模块原样 import 进来调它**，不抄它的正则 —— 一个性质两把尺子会各自漂
（`gate.sh` 头注逐字：「一个性质一把尺子」）。普查改口径，本判据的人群跟着变，台账当场对不上 ⇒ 红。

═══════════════════════════════════════════════════════════════════════════════
 二、身份键 = （文件, 原文全文, 同文件同文的第几次出现）—— **不含行号**
═══════════════════════════════════════════════════════════════════════════════
行号只进「住址」列当导航用，**不参与相等**。理由：`src/` 里任何一处改动都会让下面的行号整体漂，
行号进键 ⇒ 这条判据会在「没加没删任何文案」时红，红多了就没人看了。
代价（写明，不藏）：**同一文件里把一句话原样搬个位置**，本判据看不见 —— 那不是新文案，本来也不该红。
代价二（死值验现打）：同一文件里有几句**同文**时（如 `tabs.ts` 的三处「拉前失败」），改其中一句，
「台账有、普查没有」那一侧报出的住址是**同文里的最后一处**（`occ` 顺延），不一定是被改的那一行 ——
红是对的、条数是对的，住址要人去同文那几处里认。

⚠ 普查的残差里 `text` 截到 80 字；本文件按 (文件, 行, 前缀) 回到遮注释后的源码里把**全文**取回来。
取不回来 ⇒ 记 `__UNRESOLVED__` 并算结构失败（红），不静默拿截断的串凑数。

═══════════════════════════════════════════════════════════════════════════════
 三、裁词只许四档（任务书逐字）
═══════════════════════════════════════════════════════════════════════════════
  保留 · 改（属哪一类病）· 内部词不该对用户说（换成什么）· 判不了（为什么）
台账第三列必须以下列前缀之一开头，否则红：
  `保留` · `改·§2.1` … `改·§2.6` · `内部词→` · `判不了`
"判不了" 不是失败 —— 如实留着；它只要求第四列（依据）说了为什么。
本文件只判**形状**（人群两向相等 · 裁词在档 · 依据非空），不判裁得对不对 —— 那是下面四、五两段的口径，靠人复核。

═══════════════════════════════════════════════════════════════════════════════
 四、裁词口径（逐条裁时照这个裁；与 `tests/frontend/ui/settings/ui-copy-discipline.vitest.ts` 的五种形状对齐）
═══════════════════════════════════════════════════════════════════════════════
先判**对不对外**，再判**有没有病**：

  · 不对外（内部键 / 匹配用的探针串 / 日志 / 给另一个 agent 的 cc-bus 注入文本/
    判据支撑表 / 测试替身的台词 …）⇒ `保留`，依据以 `[不对外]` 开头，说清它去哪了。
  · 对外、或**可达性存疑但按对外裁**（后端 `Err` 回到 toast 的那一族）⇒ 依据以 `[对外]` / `[可达存疑]` 开头，再裁病：

  | 裁词 | 什么时候用 |
  |---|---|
  | `保留` | 对外且四类病都不中。R1b 限用词（tmux / ssh / git / PowerShell）在「用户自己也这么叫」的语境里出现 ⇒ 保留 |
  | `内部词→<换成什么>` | 病只在**一个词**上，换词就治好：R1 词表（daemon · sid · jsonl · relay · pidfile · HWND · 灰 · 判据 …）或 R1 外的内部名 |
  | `改·§2.1` | 内部标识符外泄，但**换词治不好**：整句是在讲实现（「要一个 tokio 运行时」「@option 没打上」）· 源码住址（`x.rs` / `a::b`）· 日志行格式 |
  | `改·§2.2` | 内部推理 / 设计论证当报错（「判不了」「放大器」「本条不推翻」「那条路」、解释我们为什么这样设计）|
  | `改·§2.3` | 把内部维护派给用户（「建议重装 X」「请先检查 Y」「下一步：…」处方），且那件事用户做不了或做了未必好 |
  | `改·§2.4` | 一句三合一 ＋ 破折号 / 括号补充 / markdown `**` 的 AI 味形状（与普查代理同口径：破折号 · 括号补充 · 首句 ≥30 汉字 三信号中 ≥2；或读得出「发生了什么＋我接下来做什么＋为什么没事」塞在一句里）|
  | `改·§2.5` / `改·§2.6` | 自造比喻当概念名 / `title` 档的口语腔（极少；自造比喻若换词能治，优先走 `内部词→`）|
  | `判不了` | 读了上下文仍定不下：碎片（运行期拼接，单看这一截没有语义）· 需要产品拍板的术语（今天没有对外词可换）· 可达性与语义都悬着 |

  一条同时中几类时，裁词取**最要紧的那一类**（外泄 > 推理 > 派活 > 形状），其余写进依据：`兼§2.x`。

  · `aria-*` 的无障碍名（普查桶 a11y）**对外**（读屏器念给用户听）：按标签面裁 ——
    不许问句、不许口语词、许动词开头（R5 / R6）。进表时只进 `aria-label` 的键填 `kind: "aria"`，
    与看得见的字共用的按看得见的那一档填；`aria_kind_check` 两向判这件事（表里 aria == 只落在 a11y 出口的键）。

═══════════════════════════════════════════════════════════════════════════════
 五、依据列的机读标记（摘要是从这些标记数出来的，别自由发挥）
═══════════════════════════════════════════════════════════════════════════════
  · 开头：`[不对外]` / `[对外]` / `[可达存疑]` 三选一。
  · `兼§2.1` … `兼§2.6`：次要的病。
  · `新词:<词>`：R1 词表之外、这一条里出现的**内部词**（提给 R1 增补用）。可以有多个。
  · `→「…」`：可选，一句改写方向（CP2 的参考，不是定稿）。
"""

import argparse
import importlib.util
import json
import sys
import tempfile
from collections import Counter, defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
CENSUS_PATH = HERE / "K-T68-A1-outward-copy-census.py"
LEDGER = HERE / "CP1-copy-verdicts.tsv"

# `改·` 后跟病号：四类主病 §2.1–§2.4，另收 §2.5（自造比喻）/ §2.6（口语腔）两个后补的病号，
# 免得真撞上时被迫塞进别的病号里。
VERDICT_PREFIXES = ("保留", "改·§2.1", "改·§2.2", "改·§2.3", "改·§2.4", "改·§2.5", "改·§2.6", "内部词→", "判不了")
HEADER = ("住址", "原文", "裁词", "依据")
REACH_TAGS = ("[不对外]", "[对外]", "[可达存疑]")

# 防空转 = **正控**，不是地板。
#
# 这里原来是地板 `FLOOR_BAND = 300`（「今天的存疑带是一千条上下；跌破它说明普查那把尺子没切到东西」）。
# 它的前提被全量抽表推翻了：CP2b（前端 ＋ monitor）与 CP2c（常驻端）把对外句子搬进文案表之后，
# 存疑带**本来就该**越来越小 —— 两路合进主线那一拍是 241 条，两边单看都 > 300、合起来 < 300，
# 地板把「抽对了」读成「尺子坏了」。按数字往下重钉只是换一个下一次抽表又会失效的数。
#
# ⇒ 换成**正控**（同 `CP2c-backend-copy-pending.py` 的做法）：在一棵临时树里现造一份文件，里面恰好一句
# 普查该认进存疑带的字面量（`PROBE_BAND`，一个裸常量 —— 静态扫描分不出它上不上界面）＋ 一句该进主集、
# **不许**进存疑带的（`PROBE_MAIN`，`throw new Error(…)`）；普查对那棵树的存疑带必须**恰好**是
# `[PROBE_BAND]`：认不出（尺子没切到东西 / import 错了）⇒ 空转红；认多（主集那句也掉进来）⇒ 同样红。
# 正控跑的是**同一个** `doubt_band`（同一份普查模块），不是另写一份判法。
PROBE_BAND = "正控样本：这一句只为让普查认进存疑带"
PROBE_MAIN = "正控样本：这一句进主集，不进存疑带"
PROBE_FILE = "probe.ts"
PROBE_BODY = (
    f'export const PROBE_SENTENCE = "{PROBE_BAND}";\n'
    "export function probeThrow(): never {\n"
    f'  throw new Error("{PROBE_MAIN}");\n'
    "}\n"
)


def probe_control(census=None, body: str = PROBE_BODY) -> tuple[bool, list[str]]:
    """→ (过没过, 普查在临时树里认出的存疑带原文)。过 ⇔ 认出的恰好是 `[PROBE_BAND]`。"""
    census = census or load_census()
    saved = census.SRC_ROOT
    try:
        with tempfile.TemporaryDirectory() as td:
            (Path(td) / PROBE_FILE).write_text(body, encoding="utf-8")
            got = [e["text"] for e in doubt_band(census, Path(td))]
    finally:
        census.SRC_ROOT = saved
    return got == [PROBE_BAND], got


def census_table() -> dict:
    return json.loads((REPO / "src" / "shared" / "copy" / "table.json").read_text(encoding="utf-8")).get("entries", {})


def load_census():
    spec = importlib.util.spec_from_file_location("k_t68_census", CENSUS_PATH)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def esc(s: str) -> str:
    """TSV 一格：反斜杠、制表、换行都转义（台账必须一条一行）。"""
    return s.replace("\\", "\\\\").replace("\t", "\\t").replace("\n", "\\n").replace("\r", "\\r")


def unesc(s: str) -> str:
    out, i = [], 0
    while i < len(s):
        c = s[i]
        if c == "\\" and i + 1 < len(s):
            out.append({"t": "\t", "n": "\n", "r": "\r", "\\": "\\"}.get(s[i + 1], "\\" + s[i + 1]))
            i += 2
            continue
        out.append(c)
        i += 1
    return "".join(out)


# 最近一趟对真树 `scan()` 的主集（`aria_kind_check` 借它，免得同一趟再扫一遍）。
LAST_MAIN: list = []


def all_copy_refs(census) -> Counter:
    """真树生产源码里每个键被 `copyText("key")` / `copy_text("key")` 引了几次（遮注释、剥测块，同普查）。"""
    n = Counter()
    for path, rel in census.production_files(census.SRC_ROOT):
        lang = "rs" if rel.endswith(".rs") else "ts"
        m = census.mask_comments(path.read_text(encoding="utf-8"), lang)
        if lang == "rs":
            m = census.strip_cfg_test(m)
        n.update(r.group(1) for r in census.COPY_REF.finditer(m))
    return n


def aria_kind_check(main: list, refs: Counter, table: dict) -> dict:
    """：「aria-* 单立一个 kind、按标签面规则管」。

    两向相等：表里 `kind == "aria"` 的键 == 生产源码里**每一处**引用都落在 a11y 出口（`aria-label`）的键。
    与看得见的字共用的键（别处还引它，如命令栏的 `title:`）按看得见的那一档管，不算 aria。
    经变量转交的（`const X = copyText(…)` 再 `setAttribute("aria-label", X)`）不落在出口上，不在人群里（已登记缺口）。
    """
    at = Counter(e["key"] for e in main if e.get("via") == "table" and e["bucket"] == "a11y")
    want = {k for k, c in at.items() if refs[k] == c}
    got = {k for k, v in table.items() if v.get("kind") == "aria"}
    return dict(ok=bool(want) and want == got, want=len(want),
                untagged=sorted(want - got), stray=sorted(got - want))


def doubt_band(census, src_root: Path | None = None):
    """→ [dict(file, line, text, ctx, occ)]，**人群定义全部来自普查模块**。"""
    if src_root is not None:
        census.SRC_ROOT = src_root
    _files, _lines, entries, residual, _en, _cc = census.scan(census.SRC_ROOT)
    if src_root is None:  # 正控那棵临时树不覆盖真树的主集
        LAST_MAIN[:] = entries
    unsure = [r for r in residual if not census.is_declined(r)]

    # 回源码取全文（普查残差里的 text 截到 80 字）
    cache = {}
    root = census.SRC_ROOT

    def masked_of(rel):
        if rel not in cache:
            p = (census.REPO / rel) if (census.REPO / rel).exists() else (root / rel)
            raw = p.read_text(encoding="utf-8")
            lang = "rs" if rel.endswith(".rs") else "ts"
            m = census.mask_comments(raw, lang)
            if lang == "rs":
                m = census.strip_cfg_test(m)
            # 行首偏移表
            starts = [0]
            for i, ch in enumerate(m):
                if ch == "\n":
                    starts.append(i + 1)
            cache[rel] = (m, lang, starts)
        return cache[rel]

    out = []
    for r in unsure:
        m, lang, starts = masked_of(r["file"])
        lo = starts[r["line"] - 1]
        hi = starts[r["line"]] if r["line"] < len(starts) else len(m)
        full = None
        for s, _e, text in census.iter_strings(m, lang, lo, len(m)):
            if s >= hi:
                break
            if s >= lo and text[:80] == r["text"]:
                full = text
                break
        out.append(dict(file=r["file"], line=r["line"], text=full if full is not None else "__UNRESOLVED__",
                        ctx=r["ctx"]))
    # 同文件同文的第几次出现（按行序）
    seen = Counter()
    for e in sorted(out, key=lambda x: (x["file"], x["line"])):
        k = (e["file"], e["text"])
        e["occ"] = seen[k]
        seen[k] += 1
    return out


def key_of(file: str, text: str, occ: int):
    return (file, text, occ)


def read_ledger(path: Path):
    """→ (rows, problems)。rows: [dict(file, line, text, verdict, basis, occ, lineno)]"""
    problems = []
    rows = []
    if not path.exists():
        return rows, [f"台账不存在：{path}"]
    lines = path.read_text(encoding="utf-8").split("\n")
    if not lines or tuple(lines[0].split("\t")) != HEADER:
        problems.append(f"台账表头不对：期望 {'\\t'.join(HEADER)}")
    seen = Counter()
    for i, ln in enumerate(lines[1:], start=2):
        if not ln.strip():
            continue
        cols = ln.split("\t")
        if len(cols) != 4:
            problems.append(f"台账第 {i} 行列数 {len(cols)} ≠ 4")
            continue
        addr, text, verdict, basis = cols
        if ":" not in addr:
            problems.append(f"台账第 {i} 行住址不是 file:line：{addr}")
            continue
        f, _, ln_no = addr.rpartition(":")
        text = unesc(text)
        if not verdict.startswith(VERDICT_PREFIXES):
            problems.append(f"台账第 {i} 行裁词不在四档里：「{verdict}」")
        if not basis.strip():
            problems.append(f"台账第 {i} 行依据为空（{addr}）")
        elif not basis.startswith(REACH_TAGS):
            problems.append(f"台账第 {i} 行依据没以 {'/'.join(REACH_TAGS)} 开头（{addr}）")
        if verdict.startswith("判不了") and len(basis.strip()) < 6:
            problems.append(f"台账第 {i} 行「判不了」没说为什么（{addr}）")
        occ = seen[(f, text)]
        seen[(f, text)] += 1
        rows.append(dict(file=f, line=int(ln_no) if ln_no.isdigit() else -1, text=text,
                         verdict=verdict, basis=basis, occ=occ, lineno=i))
    return rows, problems


def compare(band, rows):
    bk = {key_of(e["file"], e["text"], e["occ"]): e for e in band}
    lk = {key_of(r["file"], r["text"], r["occ"]): r for r in rows}
    missing = [bk[k] for k in bk.keys() - lk.keys()]      # 普查有、台账没有 ⇒ 新文案没裁
    extra = [lk[k] for k in lk.keys() - bk.keys()]        # 台账有、普查没有 ⇒ 文案删了/改了台账没跟
    drift = [(bk[k]["line"], lk[k]["line"], k) for k in bk.keys() & lk.keys() if bk[k]["line"] != lk[k]["line"]]
    return missing, extra, drift


def run_check(band, ledger_path: Path, as_json: bool, probe: tuple[bool, list[str]] | None = None) -> int:
    probe_ok, probe_got = probe if probe is not None else probe_control()
    rows, problems = read_ledger(ledger_path)
    unresolved = [e for e in band if e["text"] == "__UNRESOLVED__"]
    if unresolved:
        problems += [f"普查残差回源码取不到全文：{e['file']}:{e['line']}" for e in unresolved]
    missing, extra, drift = compare(band, rows)
    vc = Counter()
    for r in rows:
        v = r["verdict"]
        vc[next(p for p in VERDICT_PREFIXES if v.startswith(p)) if v.startswith(VERDICT_PREFIXES) else "?"] += 1
    aria = aria_kind_check(LAST_MAIN, all_copy_refs(load_census()), census_table())
    if not aria["ok"]:
        problems += [f"只进 aria-label 的键 kind 不是 aria：{k}" for k in aria["untagged"]]
        problems += [f"kind 是 aria、却不是只进 aria-label：{k}" for k in aria["stray"]]
        if not aria["want"]:
            problems.append("普查一条只进 aria-label 的取文都没认出来 ⇒ aria 那一格空转")
    ok = not (missing or extra or problems or not probe_ok)
    rep = dict(ok=ok, band=len(band), ledger=len(rows), missing=len(missing), extra=len(extra),
               line_drift=len(drift), problems=problems[:40], probe_ok=probe_ok, probe_got=probe_got[:5],
               verdicts=dict(vc), aria=aria,
               missing_sample=[f"{e['file']}:{e['line']}\t{e['text'][:60]}" for e in sorted(missing, key=lambda x: (x['file'], x['line']))[:40]],
               extra_sample=[f"{r['file']}:{r['line']}\t{r['text'][:60]}" for r in sorted(extra, key=lambda x: (x['file'], x['line']))[:40]])
    if as_json:
        print(json.dumps(rep, ensure_ascii=False))
    else:
        print(f"普查存疑带 {len(band)} 条 · 台账 {len(rows)} 行")
        print(f"  普查有、台账没有（新文案没裁）: {len(missing)}")
        for s in rep["missing_sample"]:
            print(f"    + {s}")
        print(f"  台账有、普查没有（台账没跟上删改）: {len(extra)}")
        for s in rep["extra_sample"]:
            print(f"    - {s}")
        print(f"  行号漂移（不判，只提示）: {len(drift)}")
        for p in problems[:40]:
            print(f"  ✘ {p}")
        print("  裁词分布: " + " · ".join(f"{k} {v}" for k, v in vc.most_common()))
        if not probe_ok:
            print(f"  ✘ 正控没过：临时树里只放了一句存疑带样本，普查认出的是 {probe_got!r} ⇒ 空转")
    if not probe_ok:
        return 3
    return 0 if ok else 1


def dump(band):
    for e in sorted(band, key=lambda x: (x["file"], x["line"])):
        print(json.dumps(e, ensure_ascii=False))


def selftest() -> int:
    """死值验：① 普查空转（扫描吐空）⇒ 正控红 ⇒ 3 · ①b 正控样本多一句存疑带 ⇒ 认多 ⇒ 不过 · ② 台账删一行 ⇒ 1 · ③ 台账多一行假条 ⇒ 1。"""
    import contextlib
    import io
    census = load_census()
    band = doubt_band(census)
    ok = True
    # ① 普查空转：扫描结果换成空（同「扫描根指空目录 / import 错了」）⇒ 正控认不出那一句 ⇒ 3
    c2 = load_census()
    real_scan = c2.scan
    c2.scan = lambda root: (lambda r: (r[0], r[1], r[2], [], r[4], r[5]))(real_scan(root))
    idle = probe_control(c2)
    with contextlib.redirect_stdout(io.StringIO()):
        rc = run_check(band, LEDGER, True, idle)
    print(f"  ① 普查空转（残差吐空）⇒ 正控 {idle} · 退出码 {rc}（期望 3）")
    ok &= rc == 3 and idle[0] is False
    # ①b 认多：样本里再放一句存疑带（裸常量）⇒ 认出两句 ⇒ 不过
    extra_body = PROBE_BODY + 'export const PROBE_EXTRA = "正控样本：多出来的第二句";\n'
    many = probe_control(load_census(), extra_body)
    print(f"  ①b 样本里多一句存疑带 ⇒ 正控 {many[0]}（期望 False）· 认出 {len(many[1])} 句（期望 2）")
    ok &= many[0] is False and len(many[1]) == 2
    rows_txt = LEDGER.read_text(encoding="utf-8").split("\n")
    body = [x for x in rows_txt[1:] if x.strip()]
    if not body:
        print("  ✘ 台账是空的，②③ 两刀没有靶子")
        return 1
    with tempfile.TemporaryDirectory() as td:
        # ② 删一行
        p = Path(td) / "l.tsv"
        p.write_text("\n".join([rows_txt[0]] + body[1:]) + "\n", encoding="utf-8")
        import contextlib
        import io
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = run_check(band, p, True)
        j = json.loads(buf.getvalue())
        print(f"  ② 台账删第一行 ⇒ 退出码 {rc}（期望 1）· missing={j['missing']}（期望 1）")
        ok &= rc == 1 and j["missing"] == 1 and j["extra"] == 0
        # ③ 加一行假条
        p.write_text("\n".join([rows_txt[0]] + body + ["src/不存在.ts:1\t假文案\t保留\t死值验"]) + "\n", encoding="utf-8")
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = run_check(band, p, True)
        j = json.loads(buf.getvalue())
        print(f"  ③ 台账多一行假条 ⇒ 退出码 {rc}（期望 1）· extra={j['extra']}（期望 1）")
        ok &= rc == 1 and j["extra"] == 1 and j["missing"] == 0
    print("  ✔ 四刀都死了" if ok else "  ✘ 有刀没死 ⇒ 本判据不可信")
    return 0 if ok else 1


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description="CP1 存疑带裁决台账：两向集合相等判据")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--dump", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args()
    if a.selftest:
        sys.exit(selftest())
    b = doubt_band(load_census())
    if a.dump:
        dump(b)
        sys.exit(0)
    sys.exit(run_check(b, LEDGER, a.json))
