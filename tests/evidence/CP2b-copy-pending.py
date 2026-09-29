#!/usr/bin/env python3
# ruff: noqa: E501
"""CP2b：**还有对外字面量的文件 == 待办表**（两向集合相等）—— 全量抽表的主判据。

住址：`<仓根>/tests/evidence/CP2b-copy-pending.py`
待办表：`<仓根>/tests/evidence/CP2b-copy-pending.tsv`（一行一个文件 ＋ 一句理由）
挂进 `npm test`：`tests/copy/copy-pending.vitest.ts`

要求住址（逐字）：
  · `设计/01 §6.9`「**所有对外文案与报错都从一张表来**（结构化的 key → 文本，插值点留在表里）」；
  · `设计/91 §5.7` 第 4 步「**全量抽表**，并照台账的『改』与术语表的换法改句子」；
  · 用户 2026-09-17「**要引入一层文案表。把文本都抽出来解耦。**」（`91 §1`）。

跑法（仓根下）：
    python3 tests/evidence/CP2b-copy-pending.py            # 判
    python3 tests/evidence/CP2b-copy-pending.py --json     # 机读（vitest 那一侧吃这个）
    python3 tests/evidence/CP2b-copy-pending.py --list     # 逐文件列出还剩的对外字面量（抽表时用）
    python3 tests/evidence/CP2b-copy-pending.py --selftest # 死值验三刀

退出码：0 = 两向相等 · 1 = 不相等 / 待办表形状不对 · 3 = 空转（普查一个对外字面量都没数到 —— 尺子没切到东西）

═══════════════════════════════════════════════════════════════════════════════
 一、「对外字面量」—— 人群全部借来，本文件不写一条正则
═══════════════════════════════════════════════════════════════════════════════
一个源码字面量是对外字面量 ⇔
  ① 它在普查 `K-T68-A1` 的**主集**里，且 `via == "literal"`（出口里的 `copyText` / `copy_text` 调用是 `via == "table"`，已经抽走了）；或
  ② 它在普查的**存疑带**里（`CP1-copy-verdicts.py::doubt_band`），且 CP1 台账那一行的依据**不是** `[不对外]` 开头
     （`[对外]` / `[可达存疑]` 都算；台账里没有的也算 —— 保守，与 CP1 那条「新文案没裁」同时红）。

**登记例外**（留在源码里的）不在这里另起一张表 —— 每一类都是别人已有的一格：
普查明写排除的出口（console / tracing 非 error / panic / expect / assert / eprintln / println）·
预备队 `tracing::error!`（`91 §2.5`）· cc-bus 注入文本（`91 §3.2`）· CP1 台账 `[不对外]` 的行。

═══════════════════════════════════════════════════════════════════════════════
 二、为什么按**文件**、为什么是**集合**相等
═══════════════════════════════════════════════════════════════════════════════
  · 按文件：`91 §5.1` 决定 1「key 的第一段与文件面对齐」—— 一个文件抽一次，key 前缀天然分块。
  · 集合、不是逐文件计数：待办表里的文件别的路照样会加字，逐文件计数会让每一句新字都来改这张表；
    本判据要挡的是 **抽完的文件（不在表里）又长回字面量** 与 **抽完了没划掉**。
  · 不是地板：抽完一个文件，必须回来删掉那一行（强制触碰）；终态只剩射程外那几行。
"""

import argparse
import importlib.util
import json
import sys
import tempfile
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
CP1_PATH = HERE / "CP1-copy-verdicts.py"
PENDING = HERE / "CP2b-copy-pending.tsv"
HEADER = ("文件", "理由")

# 正控：普查的两处新口径（V99 认得纯符号串 · 读表认得出口里的取文口调用）在执行链上的读数。
# 抽完以后源码里一个符号字面量都不剩，CP1 台账那几行也跟着走 ⇒ 「普查还认不认得符号」就没人看了 ——
# 所以把判准本身拿固定样本现打一遍，交给 vitest 判相等。
PROBE_SAMPLES = ("✕", "↗", " · ", "关闭", "{…}：{…}", "x", "—")
PROBE: dict = {}

# 〔FIX5 · `91 §6` 第 5 条〕正控：CSS 的 `content:` 与入口 HTML 那两面真在人群里 —— 现造一棵临时树（一份 css ＋ 一份入口 html），
# 普查必须**恰好**认出下面那几句（注释 · 脚本里的字 · `var(--x)` 都不算）。今天源码里一条都不剩，没有这一格就没人看「还认不认得」。
STATIC_PROBE_CSS = '.a::before { content: "▸ "; }\n.b::after { content: var(--m); }\n/* content: "注释里的字" */\n.c::after { content: "\\2713"; }\n'
STATIC_PROBE_HTML = '<!doctype html><html><head><title>某窗口</title><!-- 注释里的字 --></head><body><p>正文</p><script>const s = "脚本里的字";</script></body></html>\n'


def static_probe(census) -> list:
    with tempfile.TemporaryDirectory() as td:
        src = Path(td) / "src"
        src.mkdir()
        (src / "a.css").write_text(STATIC_PROBE_CSS, encoding="utf-8")
        (Path(td) / "index.html").write_text(STATIC_PROBE_HTML, encoding="utf-8")
        _files, got = census.scan_static_faces(src)
    return sorted(f"{e['sink']}:{e['kind']}:{e['text']}" for e in got)


def load_cp1():
    spec = importlib.util.spec_from_file_location("cp1_copy_verdicts", CP1_PATH)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


# 〔MG1 · 合并 CP2b × CP2c，09-25〕常驻端 `src/backend/**` 与 monitor 的子 crate `src/common/**` 归 CP2c 的待办表
# （`CP2c-backend-copy-pending.py`，射程恰是这两棵）⇒ 本表的人群排除它们：两张表各管一半、人群不重叠，
# 并集仍是普查的全部对外字面量（两份判据都挂在 vitest 上，漏哪一半都红）。
# ⚠ 前缀按「相对仓根」判；monitor crate 里的后端调用层（`src/frontend/shell/src/{backend_control,inbound_client,local_backend,…}.rs`，
# THIN 第 7 件前住 `shell/src/backend/`）仍归本表。
CP2C_SCOPE = ("src/backend/", "src/common/")


def outward_literals(cp1, src_root: Path | None = None, ledger: Path | None = None):
    """→ [dict(file, line, text, from)]：今天源码里全部对外字面量（CP2c 那两棵除外）。"""
    return [x for x in _outward_literals_all(cp1, src_root, ledger) if not _in_cp2c_scope(x["file"])]


def _in_cp2c_scope(rel: str) -> bool:
    # 普查对仓外临时树（selftest 的空语料）给绝对路径 ⇒ 取 `/src/` 之后那一截再判（同 CP2c 的 `in_scope`）。
    if rel.startswith("/") and "/src/" in rel:
        rel = "src/" + rel.split("/src/", 1)[1]
    return rel.startswith(CP2C_SCOPE)


def _outward_literals_all(cp1, src_root: Path | None = None, ledger: Path | None = None):
    census = cp1.load_census()
    if src_root is not None:
        census.SRC_ROOT = src_root
    scanned = census.scan(census.SRC_ROOT)
    _f, _l, entries, _r, _en, _cc = scanned
    census.scan = lambda _root: scanned   # doubt_band 里还要扫一遍同一份语料：用这一次的结果，不扫两遍
    out = []
    PROBE["via_table"] = sum(1 for e in entries if e.get("via") == "table" and e["bucket"] not in census.RESERVE_BUCKETS)
    PROBE["is_copy_text"] = {t: census.is_copy_text(t) for t in PROBE_SAMPLES}
    PROBE["static_faces"] = static_probe(census)
    for e in entries:
        if e["bucket"] in census.RESERVE_BUCKETS or e.get("via") != "literal":
            continue
        out.append(dict(file=e["file"], line=e["line"], text=e["text"], src="主集·" + e["sink"]))
    band = cp1.doubt_band(census, src_root)
    rows, _p = cp1.read_ledger(ledger or cp1.LEDGER)
    lk = {cp1.key_of(r["file"], r["text"], r["occ"]): r for r in rows}
    for b in band:
        r = lk.get(cp1.key_of(b["file"], b["text"], b["occ"]))
        if r is not None and r["basis"].startswith("[不对外]"):
            continue
        out.append(dict(file=b["file"], line=b["line"], text=b["text"],
                        src="存疑带·" + ("台账没有" if r is None else r["basis"][:6])))
    return out


def read_pending(path: Path):
    problems = []
    rows = {}
    if not path.exists():
        return rows, [f"待办表不存在：{path}"]
    lines = path.read_text(encoding="utf-8").split("\n")
    if not lines or tuple(lines[0].split("\t")) != HEADER:
        problems.append("待办表表头不对：期望 文件\\t理由")
    for i, ln in enumerate(lines[1:], start=2):
        if not ln.strip():
            continue
        cols = ln.split("\t")
        if len(cols) != 2 or not cols[1].strip():
            problems.append(f"待办表第 {i} 行不是「文件\\t理由」两列（理由不许空）")
            continue
        if cols[0] in rows:
            problems.append(f"待办表第 {i} 行重复：{cols[0]}")
        rows[cols[0]] = cols[1]
    return rows, problems


def run_check(lits, pending_path: Path, as_json: bool) -> int:
    rows, problems = read_pending(pending_path)
    have = Counter(x["file"] for x in lits)
    unlisted = sorted(set(have) - set(rows))      # 抽完的文件长回了字面量 / 新文件没进表
    stale = sorted(set(rows) - set(have))         # 待办表说还没抽，其实已经一条不剩
    empty = len(lits) == 0
    ok = not (unlisted or stale or problems or empty)
    rep = dict(ok=ok, literals=len(lits), files=len(have), pending=len(rows),
               unlisted=[f"{f}（{have[f]} 条）" for f in unlisted][:40], stale=stale[:40],
               problems=problems[:40], empty=empty,
               by_file={f: have[f] for f in sorted(have)}, **PROBE)
    if as_json:
        print(json.dumps(rep, ensure_ascii=False))
    else:
        print(f"对外字面量 {len(lits)} 条 · 分布在 {len(have)} 个文件 · 待办表 {len(rows)} 行")
        print(f"  有字面量、却不在待办表（抽完的文件又长回来了 / 新文件没登记）: {len(unlisted)}")
        for s in rep["unlisted"]:
            print(f"    + {s}")
        print(f"  在待办表、其实已经一条不剩（抽完了没划掉）: {len(stale)}")
        for s in rep["stale"]:
            print(f"    - {s}")
        for p in problems:
            print(f"  ✘ {p}")
        if empty:
            print("  ✘ 一个对外字面量都没数到 ⇒ 普查没切到东西（空转），不是抽完了")
    if empty:
        return 3
    return 0 if ok else 1


def list_remaining(lits):
    for x in sorted(lits, key=lambda x: (x["file"], x["line"])):
        print(f"{x['file']}:{x['line']}\t{x['src']}\t{x['text'][:100]!r}")


def selftest() -> int:
    """死值验三刀：① 空语料 ⇒ 3 · ② 待办表删掉一个还有字面量的文件 ⇒ 1（unlisted）· ③ 待办表多一行没有字面量的文件 ⇒ 1（stale）。"""
    import contextlib
    import io
    cp1 = load_cp1()
    ok = True
    with tempfile.TemporaryDirectory() as td:
        with contextlib.redirect_stdout(io.StringIO()):
            rc = run_check(outward_literals(load_cp1(), Path(td)), PENDING, True)
    print(f"  ① 空语料 ⇒ 退出码 {rc}（期望 3）")
    ok &= rc == 3
    lits = outward_literals(cp1)
    rows, _ = read_pending(PENDING)
    victim = next((f for f in sorted(rows) if any(x["file"] == f for x in lits)), None)
    if victim is None:
        print("  ✘ 待办表里没有一个还有字面量的文件 —— ② 没有靶子")
        return 1
    with tempfile.TemporaryDirectory() as td:
        p = Path(td) / "p.tsv"
        body = [f"{f}\t{r}" for f, r in sorted(rows.items())]
        p.write_text("\n".join(["\t".join(HEADER)] + [b for b in body if not b.startswith(victim + "\t")]) + "\n", encoding="utf-8")
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = run_check(lits, p, True)
        j = json.loads(buf.getvalue())
        print(f"  ② 待办表删掉 {victim} ⇒ 退出码 {rc}（期望 1）· unlisted={len(j['unlisted'])}（期望 1）")
        ok &= rc == 1 and len(j["unlisted"]) == 1 and not j["stale"]
        p.write_text("\n".join(["\t".join(HEADER)] + body + ["src/不存在.ts\t死值验"]) + "\n", encoding="utf-8")
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = run_check(lits, p, True)
        j = json.loads(buf.getvalue())
        print(f"  ③ 待办表多一行没有字面量的文件 ⇒ 退出码 {rc}（期望 1）· stale={j['stale']}")
        ok &= rc == 1 and j["stale"] == ["src/不存在.ts"] and not j["unlisted"]
    print("  ✔ 三刀都死了" if ok else "  ✘ 有刀没死 ⇒ 本判据不可信")
    return 0 if ok else 1


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description="CP2b 全量抽表：还有对外字面量的文件 == 待办表")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args()
    if a.selftest:
        sys.exit(selftest())
    got = outward_literals(load_cp1())
    if a.list:
        list_remaining(got)
        sys.exit(0)
    sys.exit(run_check(got, PENDING, a.json))
