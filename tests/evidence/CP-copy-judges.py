#!/usr/bin/env python3
"""CP1 · CP2b · CP2c 三条文案判据**一个进程、一趟普查**跑完，吐一行 JSON 给 vitest（`tests/copy/copy-judges.vitest.ts`）。

# 为什么有这个文件

三条判据的人群都借普查 `K-T68-A1-outward-copy-census.py`：各自的 `--json` 都要把整仓扫一遍（遮注释、找出口、
回源码取全文），一趟在本机 8–12 秒。原先三个 vitest 文件各起一个 python、并行各扫一遍 ——
云端 windows runner（4 核）上三个整仓扫挤在一起，连同一时刻起的别的小量具（S27）一起被拖到超时
（`backend-copy-pending` 129 s 撞 120 s 的期限，S27 一个零点几秒的脚本跑了 70 s）。
⇒ 这里载**一份**普查、真树只扫**一次**，三条判据的读数都从它出。

# 不改什么

- 三条判据的口径、人群、正控一条不动：这里只是**调用方**，调的就是三份量具自己的 `outward_literals` / `doubt_band` /
  `run_check` / 探针（同一份代码，不另写判法）。三份量具各自的命令行（人读 · `--json` · `--list` · `--selftest`）照旧能单跑。
- 正控那几棵临时树**不**走共用的那份普查：各自现载一份（共用那份的 `scan` 只认真树）。

# 跑法

    python3 tests/evidence/CP-copy-judges.py --json     # 一行：{"CP1": {…}, "CP2b": {…}, "CP2c": {…}}

退出码：三条里最坏的那一个（0 齐 · 1 不相等 · 3 空转）。读数本身照三份量具各自 `--json` 的形状，一个字段不改。
"""

import contextlib
import importlib.util
import io
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent


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
    """三份量具的 `run_check(..., as_json=True)` 把读数印成一行 JSON：接住它，不另造一份读数形状。"""
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        rc = fn()
    return rc, json.loads(buf.getvalue().strip().split("\n")[-1])


def main() -> int:
    cp1 = _load("cp1_copy_verdicts", "CP1-copy-verdicts.py")
    cp2b = _load("cp2b_copy_pending", "CP2b-copy-pending.py")
    cp2c = _load("cp2c_backend_copy_pending", "CP2c-backend-copy-pending.py")

    census = cp1.load_census()
    share_real_tree(census)

    with contextlib.redirect_stdout(io.StringIO()):   # 普查与量具中途的人读输出不进这一行 JSON
        band = cp1.doubt_band(census)
        lits_b = cp2b.outward_literals(cp1, census=census)
        lits_c, scope_c = cp2c.outward_literals(cp1, census=census)
        probe_c = cp2c.probe()

    # CP1 的 `aria_kind_check` 读 `LAST_MAIN`（真树那一趟 `doubt_band` 填的）⇒ 它要在真树的 doubt_band 之后、临时树之前不被覆盖：
    # 临时树的正控（`probe_control`）不写 `LAST_MAIN`（`src_root` 不为空时不覆盖）。
    rc1, r1 = _json_of(lambda: cp1.run_check(band, cp1.LEDGER, True, census=census))
    rc2b, r2b = _json_of(lambda: cp2b.run_check(lits_b, cp2b.PENDING, True))
    rc2c, r2c = _json_of(lambda: cp2c.run_check(lits_c, scope_c, cp2c.PENDING, True, probe_c))

    print(json.dumps({"CP1": r1, "CP2b": r2b, "CP2c": r2c}, ensure_ascii=False))
    return max(rc1, rc2b, rc2c)


if __name__ == "__main__":
    if "--json" not in sys.argv:
        print("用法：python3 tests/evidence/CP-copy-judges.py --json（人读请分别跑三份量具）", file=sys.stderr)
        sys.exit(2)
    sys.exit(main())
