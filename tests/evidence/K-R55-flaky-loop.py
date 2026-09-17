#!/usr/bin/env python3
"""K-R55 量具③：在**加载**下把那条 flaky 判据连跑 N 趟，数红几趟。

住址：本文件（仓内 `evidence/`）。被测对象 = `.claude/worktrees/k-r55` 那棵树编出来的
daemon 测试二进制。跑法：在沙箱里（`evidence/K-R55-sandbox-run.py`），
先 `cd remote-daemon-proto && cargo test --no-run`，再
`CARGO_TARGET_DIR=<那棵树的 target> python3 evidence/K-R55-flaky-loop.py 400`。

⚠ 它**不是** shell 脚本：`shell_lint_registry` 那道闸要求全仓每个 `.sh` 要么进 CI 的
shellcheck 表达式、要么在它的 `EXEMPT` 里登记，而那两处都不在 `K-R55` 的写区
⇒ 本量具写成 Python（`evidence/` 里既有的多数量具也是 Python）。
"""
import glob, os, subprocess, sys

TEST = "observe::accounts_query::tests::an_unreadable_environ_is_never_reported_as_the_zero_account"


def main() -> int:
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 40
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
    busy = [subprocess.Popen(["sh", "-c", "while :; do :; done"]) for _ in range(os.cpu_count() * 4)]
    red = 0
    try:
        for _ in range(n):
            r = subprocess.run([binary, "--exact", TEST], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if r.returncode != 0:
                red += 1
    finally:
        for b in busy:
            b.kill()
            b.wait()
    print(f"加载下连跑 {n} 趟，红 {red} 趟（二进制 {os.path.basename(binary)}）")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
