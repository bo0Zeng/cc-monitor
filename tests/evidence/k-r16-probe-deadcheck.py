#!/usr/bin/env python3
"""K-R16 D4 · 对 D2 量具（`k-r16-pidfile-cadence-probe.py`）做死值验。

为什么非做不可：本件的结论（「CC 没有心跳，只在状态转换时写」）**整个压在**
探针报出来的「静置段零写入」上。而「**真的零写入**」与「**探针自己哑了**」
（看错目录 / 变更判据失效 / 进程早退）在输出上长得一模一样 —— 正是本仓最恨的静默失效。

判据两臂：
  绿臂：探针盯**真**目录 → 静置段该报 0，注入一次写后该报**恰好 1**。
  红臂：把同一台探针指到一个**空的诱饵目录**（模拟「尺子看错地方」），
        写照样发生在真目录 → 探针该报 0 ⇒ 判据必须**当场翻红**。
        🔴 红臂不红 = 这条判据本身是死值，绿臂那个 PASS 一文不值。

⚠ 这份夹具原先是 `.sh`。改成 `.py` **不是为了绕开谁**：
`src-tauri/src/shell_lint_registry.rs` 那条守卫（`every_shell_script_is_either_linted_or_registered_as_exempt`）
**故意扫全仓**，就是为了逮「落在 CI 那条手写分组之外、于是一次都没被 lint 过的新脚本」——
它逮我逮得对（门禁 `cargo` 退出码 101 就是它）。而 `evidence/` 下 44 个量具全是 `.py`、
只有我这一个是 `.sh` ⇒ **随这个目录的规矩走**才是对的处置，登记豁免反而是给它开洞。

用法：`python3 evidence/k-r16-probe-deadcheck.py [工作目录]`
"""

import json
import pathlib
import subprocess
import sys
import tempfile
import threading
import time

HERE = pathlib.Path(__file__).resolve().parent
PROBE = HERE / "k-r16-pidfile-cadence-probe.py"

SEED = '{"pid":999999,"sessionId":"dead-check","cwd":"/x","status":"idle","updatedAt":1,"statusUpdatedAt":1}\n'
POKE = '{"pid":999999,"sessionId":"dead-check","cwd":"/x","status":"busy","updatedAt":2,"statusUpdatedAt":2}\n'


def run_arm(arm: str, watch: pathlib.Path, real: pathlib.Path, out: pathlib.Path, want: int) -> bool:
    (real / "999999.json").write_text(SEED)
    if out.exists():
        out.unlink()
    proc = subprocess.Popen(
        [sys.executable, str(PROBE), "--dir", str(watch), "--out", str(out),
         "--interval", "0.2", "--seconds", "6", "--tick", "999"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )

    def poke() -> None:
        time.sleep(2.5)  # 静置段：这 2.5s 内不许出现 write
        (real / "999999.json").write_text(POKE)

    t = threading.Thread(target=poke)
    t.start()
    proc.wait()
    t.join()

    rows = [json.loads(l) for l in out.read_text().splitlines() if l.strip()]
    wr = [r for r in rows if r["kind"] == "write"]
    seen = sum(1 for r in rows if r["kind"] == "seen")
    print(f"[{arm}] seen={seen} write={len(wr)} (期望 {want})")
    for r in wr:
        print(f"    {r['iso']} gap={r['gap_s']}s {r['prev_status']} -> {r['status']} changed={r['changed']}")
    ok = len(wr) == want and all(r["status"] == "busy" and r["prev_status"] == "idle" for r in wr)
    print(f"[{arm}] {'PASS' if ok else 'FAIL'}")
    return ok


def main() -> int:
    base = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path(tempfile.mkdtemp(prefix="k-r16-deadcheck-"))
    real = base / "real"
    decoy = base / "decoy"
    for d in (real, decoy):
        d.mkdir(parents=True, exist_ok=True)

    print("=== 绿臂：探针盯真目录，注入一次写 ⇒ 该报 1 ===")
    green = run_arm("green", real, real, base / "green.jsonl", 1)
    print()
    print("=== 红臂：同一台探针指到空诱饵目录，写仍发生在真目录 ⇒ 该报 0，判据该翻红 ===")
    decoy_ok = run_arm("decoy", decoy, real, base / "decoy.jsonl", 1)
    print()
    if green and not decoy_ok:
        print("DEADCHECK: PASS —— 绿臂 PASS 且红臂**真的红了** ⇒ 探针的「零写入」是读数，不是死值。")
        return 0
    print(f"DEADCHECK: FAIL —— 绿臂={green} 红臂={decoy_ok}"
          "（红臂必须失败；它若也 PASS，说明这条判据分不出真假）")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
