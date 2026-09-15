#!/usr/bin/env python3
"""K-R55 量具④：照**门禁的真实条件**（全量并行的那个 daemon 测试二进制）连跑 N 趟，
数整体红几趟、其中命中那条 flaky 判据几趟，并把红的那几趟的失败清单留到文件里。

住址与跑法同 `evidence/K-R55-flaky-loop.py`（不写成 `.sh` 的理由也同那一份）。
"""
import glob, os, subprocess, sys

TARGET_NAME = "an_unreadable_environ_is_never_reported_as_the_zero_account"


def main() -> int:
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 30
    out_path = sys.argv[2] if len(sys.argv) > 2 else "/tmp/kr55-fullsuite-red.log"
    target = os.environ.get("CARGO_TARGET_DIR")
    if not target:
        print("要 CARGO_TARGET_DIR")
        return 3
    cands = [p for p in glob.glob(f"{target}/debug/deps/cc_monitor_remote-*") if not p.endswith(".d")]
    cands = [p for p in cands if os.access(p, os.X_OK)]
    if not cands:
        print("找不到测试二进制 —— 先 cargo test --no-run")
        return 3
    binary = max(cands, key=os.path.getmtime)
    red = target_red = 0
    with open(out_path, "w") as log:
        for i in range(1, n + 1):
            r = subprocess.run([binary], stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            if r.returncode != 0:
                red += 1
                text = r.stdout.decode(errors="replace")
                log.write(f"=== 第 {i} 趟 ===\n{text[-4000:]}\n")
                if TARGET_NAME in text:
                    target_red += 1
    print(f"全量并行连跑 {n} 趟：整体红 {red} 趟 · 其中命中那条判据 {target_red} 趟（原文在 {out_path}）")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
