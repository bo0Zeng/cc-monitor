# K-R55 实现方自用量具：量「spawn 之后立刻读 /proc/<pid>/environ 读回 0 字节」的发生率。
# 住址：本文件（仓内 evidence/）。被测对象 = .claude/worktrees/k-r55 那棵树；跑法见 evidence/K-R55-sandbox-run.sh。
# 被测对象不是本仓代码，是内核那个 exec 窗口本身（判据的前提）。
import os, subprocess, sys, time
N = int(sys.argv[1]) if len(sys.argv) > 1 else 400
load = len(sys.argv) > 2 and sys.argv[2] == "load"
busy = []
if load:
    for _ in range(os.cpu_count() * 4):
        busy.append(subprocess.Popen(["sh", "-c", "while :; do :; done"]))
empty = 0
missing = 0
procs = []
for _ in range(N):
    p = subprocess.Popen(["sleep", "60"], env={"PATH": "/usr/bin:/bin"})
    procs.append(p)
    try:
        with open(f"/proc/{p.pid}/environ", "rb") as f:
            b = f.read()
        if len(b) == 0:
            empty += 1
    except OSError:
        missing += 1
for p in procs:
    p.kill(); p.wait()
for b in busy:
    b.kill(); b.wait()
print(f"N={N} load={load} 读回0字节={empty} 读不到={missing}")
