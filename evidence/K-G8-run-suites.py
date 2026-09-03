#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-G8 尺子的第二半：**在沙箱里逐套真跑**，把每套的输出落成 `<套名>.log`，
再喂给 `K-G8-floor-slack-census.py slack --runs <目录>` 算余量。

# 为什么真跑（而不是静态数）

`K-OBSERVE-E2E-audit.md`（08-28 @ `c7a57c9`）那张逐套表是**静态展开**数出来的，
它自己列的第一条量不到的东西逐字是「**量不了「跑得起来吗」**」——
而本件问的「余量」用的是 `assert-pass-floor.sh` 眼里那个数，
那是**运行期**的 `合计 PASS=<n>`。⇒ 两把尺子不同，别互相顶替。

# 🔴 为什么这个文件是 `.py` 而不是 `.sh`（09-03 现打，别改回去）

第一版写成了 `evidence/K-G8-run-suites.sh`，**当场把门禁的 `cargo` 门打红**（退出码 101）：
`src-tauri/src/shell_lint_registry.rs::every_shell_script_is_either_linted_or_registered_as_exempt`
逐字点名「`evidence/K-G8-run-suites.sh` 既不在 CI 的 shellcheck 表达式里、也没登记豁免」。
`ci.yml` 那条 `FILES=$(printf …)` 的分组是 `e2e/*.sh` · `scripts/*.sh` · …，**不含 `evidence/`**
⇒ 往 `evidence/` 放任何 `.sh` 都要么改 `ci.yml`、要么改那张登记表，**两处都在本件写区外**。
⇒ 这也解释了为什么 `evidence/` 下 30 个尺子**全是 `.py`**：不是风格，是那条判据的形状。

# 红线

🔴 只在沙箱里跑（`K31`）。本脚本**拒绝在容器外执行**，判据是 `/.dockerenv` 存在。
🔴 串行跑，不并发 —— `e2e/README.md` 逐字：fixture 目录/cwd 固定名，并发会互删。

# 用法（在沙箱里，工作目录 = 工作树根）

  python3 evidence/K-G8-run-suites.py <落日志的目录> [套名 …]

不给套名 ⇒ 跑 `ci.yml` 调用行里的**全部** 23 套（顺序照 `ci.yml`）。
每套单独超时（默认 300 秒，`--timeout` 或 `K_G8_TIMEOUT` 可调）；超时也落日志、记退出码。

宿主上启动整趟的那一条（供复算）：

  docker run --rm --network none \\
    -v /home/zbl/文档/claudecode-frontend:/home/zbl/文档/claudecode-frontend \\
    -v ccmon-cargo-registry:/opt/rust/cargo/registry \\
    -e CARGO_TARGET_DIR=/home/zbl/文档/claudecode-frontend/.claude/pm-targets/<tag> \\
    -e HOME=/home/zbl -w <工作树> ccmon-devbox:latest \\
    bash -c 'mkdir -p "$HOME/.claude/projects" && python3 evidence/K-G8-run-suites.py <日志目录>'
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import time

RE_TOTAL_PASS = re.compile(r"合计 PASS=(\d+)")
RE_CI_CALL = re.compile(
    r"^\s*run:\s*bash\s+e2e/assert-pass-floor\.sh\s+(\S+)\s+\d+\s*$"
)


def suites_from_ci(repo: str) -> list[str]:
    """人群 = `ci.yml` 的**调用行**（不是覆盖面清单，也不是 package.json 的 `test:*`）。"""
    path = os.path.join(repo, ".github", "workflows", "ci.yml")
    out = []
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            m = RE_CI_CALL.match(line.rstrip("\n"))
            if m:
                out.append(m.group(1))
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description="K-G8：沙箱里逐套真跑 e2e，落日志")
    ap.add_argument("out_dir", help="落日志的目录")
    ap.add_argument("suites", nargs="*", help="套名（不给就跑 ci.yml 调用行的全部）")
    ap.add_argument("--timeout", type=int,
                    default=int(os.environ.get("K_G8_TIMEOUT", "300")),
                    help="每套的超时秒数（默认 300）")
    ap.add_argument("--repo", default=".", help="仓根（默认当前目录）")
    args = ap.parse_args()

    if not os.path.exists("/.dockerenv"):
        print("❌ 本脚本只许在沙箱里跑（K31）。没看见 /.dockerenv ⇒ 拒绝执行。", file=sys.stderr)
        return 3

    repo = os.path.abspath(args.repo)
    os.makedirs(args.out_dir, exist_ok=True)
    suites = args.suites or suites_from_ci(repo)

    try:
        head = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=repo,
                              capture_output=True, text=True).stdout.strip() or "<非 git>"
    except OSError:
        head = "<非 git>"

    print(f"# 套数 {len(suites)}   每套超时 {args.timeout}s   日志落在 {args.out_dir}")
    print(f"# 量于提交：{head}")
    print()

    for s in suites:
        log = os.path.join(args.out_dir, f"{s}.log")
        start = time.time()
        with open(log, "wb") as fh:
            try:
                rc = subprocess.run(
                    ["npm", "run", "--silent", f"test:{s}"],
                    cwd=repo, stdout=fh, stderr=subprocess.STDOUT,
                    timeout=args.timeout,
                ).returncode
                rc_s = str(rc)
            except subprocess.TimeoutExpired:
                rc_s = f"TIMEOUT>{args.timeout}s"
            except OSError as exc:
                rc_s = f"OSError:{exc}"
        dur = int(time.time() - start)
        with open(log, encoding="utf-8", errors="replace") as fh:
            hits = RE_TOTAL_PASS.findall(fh.read())
        got = hits[-1] if hits else "<抓不到>"
        with open(os.path.join(args.out_dir, f"{s}.rc"), "w", encoding="utf-8") as fh:
            fh.write(rc_s + "\n")
        print(f"{s:<26} rc={rc_s:<14} {dur:>4}s  实得={got}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
