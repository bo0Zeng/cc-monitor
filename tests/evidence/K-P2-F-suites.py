#!/usr/bin/env python3
"""K-P2 `F` 拍：把「删掉建会话那条本地退路」的**真实波及面**量出来 —— 改前 / 改后各一趟。

用法（宿主上跑，套件一律进沙箱容器）：
    python3 evidence/K-P2-F-suites.py <被测工作树绝对路径> <改前那份 shared/ccm 的提交> [套件名…]

住址与射程（`brief` 第 12 条）：
  · 量具住址 = `<被测树>/evidence/K-P2-F-suites.py`（跟着被测树走，不住临时目录）；
  · 被测对象 = **argv[1] 那棵树**；改前那份 `shared/ccm` 取自 **argv[2] 那个提交**。
  · 它量的是**套件的判定行**，不是门禁。门禁唯一合法跑法是 `.claude/devbox/gate`
    ⇒ 这里的数**不许当门禁读数引用**（门禁只挂四套里的四套，本脚本还跑了另外几套）。

为什么要「改前那一趟」（`brief` 第 12 条「差集为空 / 没有变化」那一族）：
  只报改后的数，读者分不开「这套本来就红」与「是我打红的」。
  沙箱与宿主不等价（容器里 `HOME` 几乎是空的、没有真 tmux server）
  ⇒ 好几套在**基线上就红**。**不给基线，改后那个数就是一句自信的错答案。**

做法：把 `shared/ccm` 换成 `git show <提交>:shared/ccm` 那份 → 跑 → 逐字节还原（比 md5）。
⚠ 只动 `shared/ccm` 这一个文件；`assert` 守着还原，还原不上当场炸。
"""

import hashlib
import pathlib
import subprocess
import sys

IMAGE = "ccmon-devbox:latest"
PROJ = "/home/zbl/文档/claudecode-frontend"

SUITES = [
    "ccm-print-parity",
    "ccm-rbind-title",
    "ccm-cli.test",
    "ccm-contract-parity",
    "ccm-acceptance",
    "ccm-pretrust-acceptance",
    "p3t-local-tmux",
    "cc-spawn-uplift",
]


def run_suites(tree: pathlib.Path, suites: list[str]) -> dict[str, str]:
    script = 'mkdir -p "$HOME/.claude/projects"\n'
    for s in suites:
        script += (
            f'printf "@@ %s " {s!r}; '
            f'out=$(bash e2e/{s}.sh 2>&1); rc=$?; '
            f'printf "rc=%s | %s\\n" "$rc" '
            f'"$(printf "%s\\n" "$out" | grep -o "合计 PASS=[0-9]* FAIL=[0-9]*" | tail -1)"\n'
        )
    p = subprocess.run(
        [
            "docker", "run", "--rm", "--network", "none",
            "-v", f"{PROJ}:{PROJ}", "-e", "HOME=/home/zbl",
            "-w", str(tree), IMAGE, "bash", "-c", script,
        ],
        capture_output=True, text=True,
    )
    got: dict[str, str] = {}
    for ln in (p.stdout + p.stderr).splitlines():
        if ln.startswith("@@ "):
            _, name, rest = ln.split(" ", 2)
            got[name] = rest.strip() or "<抓不到判定行 —— 按 CRASH 记>"
    return got


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    tree = pathlib.Path(sys.argv[1]).resolve()
    base_rev = sys.argv[2]
    suites = sys.argv[3:] or SUITES

    head = subprocess.run(["git", "-C", str(tree), "rev-parse", "HEAD"],
                          capture_output=True, text=True).stdout.strip()
    dirty = subprocess.run(["git", "-C", str(tree), "status", "--porcelain"],
                           capture_output=True, text=True).stdout.strip()
    print(f"# 被测树：{tree}")
    print(f"# 量具：  {pathlib.Path(__file__).resolve()}")
    print(f"# 改后 = 本树 HEAD `{head}`（`git status` {len(dirty.splitlines())} 条）")
    print(f"# 改前 = 只把 `shared/ccm` 换成 `{base_rev}` 那份，其余一个字节不动")
    print(f"# 沙箱镜像 {IMAGE}（断网）")

    ccm = tree / "shared/ccm"
    now = ccm.read_bytes()
    md5_now = hashlib.md5(now).hexdigest()

    after = run_suites(tree, suites)

    before_bytes = subprocess.run(
        ["git", "-C", str(tree), "show", f"{base_rev}:shared/ccm"],
        capture_output=True,
    ).stdout
    assert before_bytes and before_bytes != now, "取不到改前那份、或它与现在一样"
    ccm.write_bytes(before_bytes)
    try:
        before = run_suites(tree, suites)
    finally:
        ccm.write_bytes(now)
        assert hashlib.md5(ccm.read_bytes()).hexdigest() == md5_now, "还原不逐字节！"

    print()
    print("| 套件 | 改前（`shared/ccm` @ 基点） | 改后（本树 HEAD） | 在门禁里吗 |")
    print("|---|---|---|---|")
    gate = {"ccm-print-parity", "ccm-rbind-title", "ccm-cli.test", "ccm-contract-parity"}
    for s in suites:
        mark = "✅ 门⑥/⑦/⑧" if s in gate else "❌ 只在 CI"
        print(f"| `{s}` | {before.get(s, '<没跑到>')} | {after.get(s, '<没跑到>')} | {mark} |")
    print()
    print("⚠ 还原对拍：上面这一行打得出来，就说明 `shared/ccm` 逐字节回到了 HEAD 那一份"
          f"（md5 {md5_now}）。")


if __name__ == "__main__":
    main()
