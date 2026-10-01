#!/usr/bin/env python3
# ruff: noqa: E501
"""CP2c：**后端 crate 与子 crate 里还有对外字面量的文件 == 待办表**（两向集合相等）—— 后端那一半抽表的主判据。

住址：`<仓根>/tests/evidence/CP2c-backend-copy-pending.py`
待办表：`<仓根>/tests/evidence/CP2c-backend-copy-pending.tsv`（一行一个文件 ＋ 一句理由）
挂进 `npm test`：`tests/copy/backend-copy-pending.vitest.ts`

要求住址（逐字）：
  · 「**所有对外文案与报错都从一张表来**（结构化的 key → 文本，插值点留在表里）」；
  · 决定 2「**一份文件，两侧各读，零转换**」—— 后端也读同一份 `table.json`；
  · 「后端 crate 没有表的读口 —— 露得到界面的那部分进表时，读口住哪没定」（本路定了：`copy-core`）。

与 CP2b 的 `CP2b-copy-pending.py` **同形**（人群、两向、三刀一样），只是射程是 `src/backend/**` ∪ `src/common/**`。
两张表各管一半、互不相交：CP2b 那张管前端与 monitor crate（合并时它那张表里这两棵的行删掉）。

跑法（仓根下）：
    python3 tests/evidence/CP2c-backend-copy-pending.py            # 判
    python3 tests/evidence/CP2c-backend-copy-pending.py --json     # 机读（vitest 那一侧吃这个）
    python3 tests/evidence/CP2c-backend-copy-pending.py --list     # 逐条列出还剩的对外字面量（抽表时用）
    python3 tests/evidence/CP2c-backend-copy-pending.py --selftest # 死值验三刀

退出码：0 = 两向相等 · 1 = 不相等 / 待办表形状不对 · 3 = 空转（射程里一个生产文件都没扫到，或正控探针没认出它该认的那一条）

═══════════════════════════════════════════════════════════════════════════════
 一、「对外字面量」—— 人群全部借来，本文件不写一条正则
═══════════════════════════════════════════════════════════════════════════════
一个源码字面量是对外字面量 ⇔
  ① 它在普查 `K-T68-A1` 的**主集**里、不在预备队桶，且不是经表取文的（普查读表之后那一条记 `via == "table"`；
     今天主线上的普查还不读表 ⇒ 没有 `via` 的一律当字面量）；或
  ② 它在普查的**存疑带**里（`CP1-copy-verdicts.py::doubt_band`），且 CP1 台账那一行的依据**不是** `[不对外]` 开头
     （台账里没有的也算 —— 保守，与 CP1 那条「新文案没裁」同时红）。

登记例外（留在源码里的）不在这里另起一张表 —— 每一类都是别人已有的一格：普查明写排除的出口（tracing 非 error /
panic / expect / assert / println …）· 预备队 `tracing::error!` · CP1 台账 `[不对外]` 的行 · 普查的 `EXCLUDED_DIRS`
（`guard-core` 在那里：只进 `[dev-dependencies]` 的判据支撑库）。
**契约错**（调用方是我们自己的代码发来的请求格式不对）只有一句进表（`beContract.malformed.say`），细节是英文诊断 ——
细节里一旦出现汉字，普查照旧把它数进来 ⇒ 本判据红（不另立一张表）。

═══════════════════════════════════════════════════════════════════════════════
 二、反空真 —— 终态待办表会是空的，所以「表非空」当不了正控
═══════════════════════════════════════════════════════════════════════════════
  · 射程里扫到的生产文件数（`scope_files`）过地板；
  · **探针**：在一棵临时树里放一份 `src/backend/probe.rs`（一句 `Err(format!("中文"))` ＋ 一句经表取文的），
    让同一套人群定义去数 —— 必须恰好认出那一条字面量（异源：语料是本文件现造的，不是仓里的）。
"""

import argparse
import contextlib
import importlib.util
import io
import json
import sys
import tempfile
from collections import Counter
from pathlib import Path, PurePosixPath, PureWindowsPath

HERE = Path(__file__).resolve().parent
CP1_PATH = HERE / "CP1-copy-verdicts.py"
PENDING = HERE / "CP2c-backend-copy-pending.tsv"
HEADER = ("文件", "理由")
SCOPE = ("src/backend/", "src/common/")
SCOPE_FILES_FLOOR = 100
PROBE_SRC = (
    "pub fn a() -> Result<(), String> {\n"
    '    Err(format!("探针：这一句是对外字面量 {}", 1))\n'
    "}\n"
    "pub fn b() -> Result<(), String> {\n"
    '    Err(copy_core::copy_text("beProbe.b.say", &[]))\n'
    "}\n"
)


def in_scope(rel: str) -> bool:
    """相对仓根的住址落在两棵树里。⚠ 不能用「含 `/src/backend/`」判：monitor crate 的后端调用层（CP2b 的射程）曾住 `src/frontend/shell/src/backend/`，
    按前缀判才不随目录名漂。普查对仓外的临时树（探针）给的是绝对路径 ⇒ 取 `/src/` 之后那一截再判。"""
    # POSIX（`/tmp/…`）与 Windows（`C:/Users/…/Temp/…`，windows runner 上的仓外临时树）两种绝对路径都认。
    if (PurePosixPath(rel).is_absolute() or PureWindowsPath(rel).is_absolute()) and "/src/" in rel:
        rel = "src/" + rel.split("/src/", 1)[1]
    return rel.startswith(SCOPE)


def load_cp1():
    spec = importlib.util.spec_from_file_location("cp1_copy_verdicts", CP1_PATH)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def outward_literals(cp1, src_root: Path | None = None, ledger: Path | None = None):
    """→ (literals, scope_files)：射程里今天全部对外字面量 [dict(file, line, text, src)] ＋ 扫到的射程内生产文件数。"""
    census = cp1.load_census()
    if src_root is not None:
        census.SRC_ROOT = src_root
    scope_files = sum(1 for _p, rel in census.production_files(census.SRC_ROOT) if in_scope(rel))
    scanned = census.scan(census.SRC_ROOT)
    _f, _l, entries, _r, _en, _cc = scanned
    census.scan = lambda _root: scanned   # doubt_band 里还要扫一遍同一份语料：用这一次的结果
    out = []
    for e in entries:
        if not in_scope(e["file"]) or e["bucket"] in census.RESERVE_BUCKETS or e.get("via", "literal") != "literal":
            continue
        out.append(dict(file=e["file"], line=e["line"], text=e["text"], src="主集·" + e["sink"]))
    band = cp1.doubt_band(census, src_root)
    rows, _p = cp1.read_ledger(ledger or cp1.LEDGER)
    lk = {cp1.key_of(r["file"], r["text"], r["occ"]): r for r in rows}
    for b in band:
        if not in_scope(b["file"]):
            continue
        r = lk.get(cp1.key_of(b["file"], b["text"], b["occ"]))
        if r is not None and r["basis"].startswith("[不对外]"):
            continue
        out.append(dict(file=b["file"], line=b["line"], text=b["text"],
                        src="存疑带·" + ("台账没有" if r is None else r["basis"][:6])))
    return out, scope_files


def probe() -> list:
    """异源正控：临时树里造一份射程内的文件，同一套人群定义去数。→ 认出来的字面量原文。"""
    with tempfile.TemporaryDirectory() as td:
        root = Path(td) / "src"
        (root / "backend").mkdir(parents=True)
        (root / "backend" / "probe.rs").write_text(PROBE_SRC, encoding="utf-8")
        with contextlib.redirect_stdout(io.StringIO()):
            lits, _n = outward_literals(load_cp1(), root)
    return sorted(x["text"] for x in lits)


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
        if not in_scope(cols[0]):
            problems.append(f"待办表第 {i} 行不在射程里（只管 src/backend/ 与 src/common/）：{cols[0]}")
        rows[cols[0]] = cols[1]
    return rows, problems


def run_check(lits, scope_files: int, pending_path: Path, as_json: bool, probe_got=None) -> int:
    rows, problems = read_pending(pending_path)
    have = Counter(x["file"] for x in lits)
    unlisted = sorted(set(have) - set(rows))      # 抽完的文件长回了字面量 / 新文件没进表
    stale = sorted(set(rows) - set(have))         # 待办表说还没抽，其实已经一条不剩
    empty = scope_files < SCOPE_FILES_FLOOR
    probe_ok = probe_got is None or probe_got == ["探针：这一句是对外字面量 {}"]
    ok = not (unlisted or stale or problems or empty) and probe_ok
    rep = dict(ok=ok, literals=len(lits), files=len(have), pending=len(rows), scope_files=scope_files,
               scope_files_floor=SCOPE_FILES_FLOOR,
               unlisted=[f"{f}（{have[f]} 条）" for f in unlisted][:60], stale=stale[:60],
               problems=problems[:40], empty=empty, probe=probe_got,
               by_file={f: have[f] for f in sorted(have)})
    if as_json:
        print(json.dumps(rep, ensure_ascii=False))
    else:
        print(f"射程内生产文件 {scope_files} 份 · 对外字面量 {len(lits)} 条 · 分布在 {len(have)} 个文件 · 待办表 {len(rows)} 行")
        print(f"  有字面量、却不在待办表（抽完的文件又长回来了 / 新文件没登记）: {len(unlisted)}")
        for s in rep["unlisted"]:
            print(f"    + {s}")
        print(f"  在待办表、其实已经一条不剩（抽完了没划掉）: {len(stale)}")
        for s in rep["stale"]:
            print(f"    - {s}")
        for p in problems:
            print(f"  ✘ {p}")
        if empty:
            print(f"  ✘ 射程里只扫到 {scope_files} 份生产文件（地板 {SCOPE_FILES_FLOOR}）⇒ 普查没切到东西（空转）")
        if not probe_ok:
            print(f"  ✘ 探针没认对：{probe_got}")
    if empty or not probe_ok:
        return 3
    return 0 if ok else 1


def list_remaining(lits):
    for x in sorted(lits, key=lambda x: (x["file"], x["line"])):
        print(f"{x['file']}:{x['line']}\t{x['src']}\t{x['text'][:100]!r}")


def selftest() -> int:
    """死值验三刀：① 空语料 ⇒ 3 · ② 待办表删掉一个还有字面量的文件 ⇒ 1（unlisted）· ③ 待办表多一行没有字面量的文件 ⇒ 1（stale）。"""
    cp1 = load_cp1()
    ok = True
    with tempfile.TemporaryDirectory() as td:
        with contextlib.redirect_stdout(io.StringIO()):
            lits0, n0 = outward_literals(load_cp1(), Path(td))
            rc = run_check(lits0, n0, PENDING, True)
    print(f"  ① 空语料 ⇒ 退出码 {rc}（期望 3）")
    ok &= rc == 3
    lits, n = outward_literals(cp1)
    rows, _ = read_pending(PENDING)
    victim = next((f for f in sorted(rows) if any(x["file"] == f for x in lits)), None)
    if victim is None:
        # 终态（待办表抽空了）：拿一个射程内的真文件造靶 —— 往语料里假装它还有一条字面量
        victim = "src/backend/lib.rs"
        lits = lits + [dict(file=victim, line=1, text="死值验", src="造")]
        rows = {**rows, victim: "死值验"}
    with tempfile.TemporaryDirectory() as td:
        p = Path(td) / "p.tsv"
        body = [f"{f}\t{r}" for f, r in sorted(rows.items())]
        p.write_text("\n".join(["\t".join(HEADER)] + [b for b in body if not b.startswith(victim + "\t")]) + "\n", encoding="utf-8")
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = run_check(lits, n, p, True)
        j = json.loads(buf.getvalue())
        print(f"  ② 待办表删掉 {victim} ⇒ 退出码 {rc}（期望 1）· unlisted={len(j['unlisted'])}（期望 1）")
        ok &= rc == 1 and len(j["unlisted"]) == 1 and not j["stale"]
        p.write_text("\n".join(["\t".join(HEADER)] + body + ["src/backend/不存在.rs\t死值验"]) + "\n", encoding="utf-8")
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = run_check(lits, n, p, True)
        j = json.loads(buf.getvalue())
        print(f"  ③ 待办表多一行没有字面量的文件 ⇒ 退出码 {rc}（期望 1）· stale={j['stale']}")
        ok &= rc == 1 and j["stale"] == ["src/backend/不存在.rs"] and not j["unlisted"]
    print("  ✔ 三刀都死了" if ok else "  ✘ 有刀没死 ⇒ 本判据不可信")
    return 0 if ok else 1


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description="CP2c 后端抽表：后端与子 crate 里还有对外字面量的文件 == 待办表")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args()
    if a.selftest:
        sys.exit(selftest())
    got, n = outward_literals(load_cp1())
    if a.list:
        list_remaining(got)
        sys.exit(0)
    sys.exit(run_check(got, n, PENDING, a.json, probe()))
