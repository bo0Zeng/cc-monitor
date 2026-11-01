#!/usr/bin/env python3
# ruff: noqa: E501
"""两条文案判据**一个进程、一趟普查**跑完，吐一行 JSON 给 vitest（`tests/copy/copy-ledgers.vitest.ts`）。

- **CP1**：存疑带逐条裁决台账 == 普查今天的存疑带（两向集合相等；判法住 `copy-verdicts.py`）。
- **CP2**：全仓**对外字面量零条**（全量抽表的终态），带正控。

# 为什么一趟

人群都借普查 `outward-copy-census.py`：整仓扫一遍（遮注释、找出口、回源码取全文）本机 8–12 秒。
云端 windows runner（4 核）上几趟整仓扫挤在一起会撞期限 ⇒ 这里载**一份**普查、真树只扫**一次**。

# CP2 的人群（本文件不写一条正则，全部借普查与 CP1）

一个源码字面量是对外字面量 ⇔
  ① 它在普查的**主集**里、不在预备队桶，且 `via == "literal"`（出口里的 `copyText` / `copy_text` 调用是 `via == "table"`）；或
  ② 它在普查的**存疑带**里，且 CP1 台账那一行的依据**不是** `[不对外]` 开头（台账里没有的也算 —— 与 CP1「新文案没裁」同时红）。
登记例外（console / tracing 非 error / panic / 预备队 `tracing::error!` / cc-bus 注入 / 台账 `[不对外]`）不另起表，归普查与台账的口径。
契约错只有一句进表（`beContract.malformed.say`），细节是英文诊断 —— 细节里一旦出现汉字，普查照旧数进来 ⇒ CP2 红。

# CP2 的反空真（终态是零条，所以「零命中」本身证明不了尺子切到了东西）

- `via_table`：普查同一趟认出的出口取文口调用条数 > 0；
- `is_copy_text`：普查认得纯符号串（「符号也进表」）的固定样本；
- `static_faces`：临时树里一份 css ＋ 一份入口 html，普查**恰好**认出那几句（注释 · 脚本 · `var(--x)` 不算）；
- `probe`：临时树里一份 `src/backend/probe.rs`（一句 `Err(format!("中文"))` ＋ 一句经表取文的），同一套人群定义**恰好**认出那一条；
- `scope_files`：`src/backend/` ∪ `src/common/` 里扫到的生产文件数过地板。

# 跑法

    python3 tests/evidence/CP-copy-judges.py --json     # 一行：{"CP1": {…}, "CP2": {…}}
    python3 tests/evidence/CP-copy-judges.py --list     # 逐条列出今天的对外字面量（该是空的）

退出码：两条里最坏的那一个（0 齐 · 1 不相等 / 有字面量 · 3 空转）。
"""

import contextlib
import importlib.util
import io
import json
import sys
import tempfile
from pathlib import Path, PurePosixPath, PureWindowsPath

HERE = Path(__file__).resolve().parent

BACKEND_SCOPE = ("src/backend/", "src/common/")
SCOPE_FILES_FLOOR = 100
COPY_TEXT_SAMPLES = ("✕", "↗", " · ", "关闭", "{…}：{…}", "x", "—")
STATIC_PROBE_CSS = '.a::before { content: "▸ "; }\n.b::after { content: var(--m); }\n/* content: "注释里的字" */\n.c::after { content: "\\2713"; }\n'
STATIC_PROBE_HTML = '<!doctype html><html><head><title>某窗口</title><!-- 注释里的字 --></head><body><p>正文</p><script>const s = "脚本里的字";</script></body></html>\n'
PROBE_SRC = (
    "pub fn a() -> Result<(), String> {\n"
    '    Err(format!("探针：这一句是对外字面量 {}", 1))\n'
    "}\n"
    "pub fn b() -> Result<(), String> {\n"
    '    Err(copy_core::copy_text("beProbe.b.say", &[]))\n'
    "}\n"
)
PROBE_WANT = ["探针：这一句是对外字面量 {}"]


def _load(name: str, file: str):
    spec = importlib.util.spec_from_file_location(name, HERE / file)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def share_real_tree(census) -> None:
    """让这份普查对**真树**的 `scan` 只算一次；别的根（临时树）照常现扫。"""
    real = Path(census.SRC_ROOT)
    orig = census.scan
    memo: list = []

    def scan_once(root):
        if Path(root) != real:
            return orig(root)
        if not memo:
            memo.append(orig(root))
        return memo[0]

    census.scan = scan_once


def _json_of(fn) -> tuple[int, dict]:
    """CP1 的 `run_check(..., as_json=True)` 把读数印成一行 JSON：接住它，不另造一份读数形状。"""
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        rc = fn()
    return rc, json.loads(buf.getvalue().strip().split("\n")[-1])


def in_backend_scope(rel: str) -> bool:
    # 普查对仓外临时树给绝对路径（POSIX `/tmp/…` 与 Windows `C:/…/Temp/…`）⇒ 取 `/src/` 之后那一截再判。
    if (PurePosixPath(rel).is_absolute() or PureWindowsPath(rel).is_absolute()) and "/src/" in rel:
        rel = "src/" + rel.split("/src/", 1)[1]
    return rel.startswith(BACKEND_SCOPE)


def outward_literals(cp1, census, band) -> list:
    """→ [dict(file, line, text, src)]：今天源码里全部对外字面量（人群定义见头注）。"""
    entries = census.scan(census.SRC_ROOT)[2]
    out = [
        dict(file=e["file"], line=e["line"], text=e["text"], src="主集·" + e["sink"])
        for e in entries
        if e["bucket"] not in census.RESERVE_BUCKETS and e.get("via", "literal") == "literal"
    ]
    rows, _p = cp1.read_ledger(cp1.LEDGER)
    lk = {cp1.key_of(r["file"], r["text"], r["occ"]): r for r in rows}
    for b in band:
        r = lk.get(cp1.key_of(b["file"], b["text"], b["occ"]))
        if r is not None and r["basis"].startswith("[不对外]"):
            continue
        out.append(dict(file=b["file"], line=b["line"], text=b["text"],
                        src="存疑带·" + ("台账没有" if r is None else r["basis"][:6])))
    return out


def static_probe(census) -> list:
    with tempfile.TemporaryDirectory() as td:
        src = Path(td) / "src"
        src.mkdir()
        (src / "a.css").write_text(STATIC_PROBE_CSS, encoding="utf-8")
        (Path(td) / "index.html").write_text(STATIC_PROBE_HTML, encoding="utf-8")
        _files, got = census.scan_static_faces(src)
    return sorted(f"{e['sink']}:{e['kind']}:{e['text']}" for e in got)


def literal_probe() -> list:
    """异源正控：临时树里现造一份 `src/backend/probe.rs`；另载一份 CP1 与普查（不碰真树那一份），同一套人群定义去数。"""
    cp1 = _load("cp1_copy_verdicts_probe", "copy-verdicts.py")
    census = cp1.load_census()
    with tempfile.TemporaryDirectory() as td:
        root = Path(td) / "src"
        (root / "backend").mkdir(parents=True)
        (root / "backend" / "probe.rs").write_text(PROBE_SRC, encoding="utf-8")
        census.SRC_ROOT = root
        lits = outward_literals(cp1, census, cp1.doubt_band(census, root))
    return sorted(x["text"] for x in lits)


def cp2_report(cp1, census, band) -> tuple[int, dict]:
    lits = outward_literals(cp1, census, band)
    scanned = census.scan(census.SRC_ROOT)
    rep = dict(
        literals=[f"{x['file']}:{x['line']}\t{x['src']}\t{x['text'][:100]}" for x in lits][:60],
        literal_count=len(lits),
        via_table=sum(1 for e in scanned[2] if e.get("via") == "table" and e["bucket"] not in census.RESERVE_BUCKETS),
        is_copy_text={t: census.is_copy_text(t) for t in COPY_TEXT_SAMPLES},
        static_faces=static_probe(census),
        probe=literal_probe(),
        scope_files=sum(1 for _p, rel in scanned[0] if in_backend_scope(rel)),
        scope_files_floor=SCOPE_FILES_FLOOR,
    )
    rep["empty"] = rep["via_table"] == 0 or rep["scope_files"] < SCOPE_FILES_FLOOR or rep["probe"] != PROBE_WANT
    rep["ok"] = not rep["empty"] and not lits
    return (3 if rep["empty"] else 0 if not lits else 1), rep


def main() -> int:
    cp1 = _load("cp1_copy_verdicts", "copy-verdicts.py")
    census = cp1.load_census()
    share_real_tree(census)

    with contextlib.redirect_stdout(io.StringIO()):   # 普查与量具中途的人读输出不进这一行 JSON
        band = cp1.doubt_band(census)
        rc2, r2 = cp2_report(cp1, census, band)

    if "--list" in sys.argv:
        print("\n".join(r2["literals"]) or "（零条）")
        return rc2
    # CP1 的 `aria_kind_check` 读 `LAST_MAIN`（真树那一趟 `doubt_band` 填的）；临时树的正控另载一份 CP1，不覆盖它。
    rc1, r1 = _json_of(lambda: cp1.run_check(band, cp1.LEDGER, True, census=census))

    print(json.dumps({"CP1": r1, "CP2": r2}, ensure_ascii=False))
    return max(rc1, rc2)


if __name__ == "__main__":
    if "--json" not in sys.argv and "--list" not in sys.argv:
        print("用法：python3 tests/evidence/CP-copy-judges.py --json | --list（CP1 人读请跑 copy-verdicts.py）", file=sys.stderr)
        sys.exit(2)
    sys.exit(main())
