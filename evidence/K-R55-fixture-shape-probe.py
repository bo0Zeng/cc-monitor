# K-R55 实现方自用量具②：**照那条 flaky 判据的形状**复现一遍，量「丙（对照活体）的
# 住址：本文件（仓内 evidence/）。被测对象 = .claude/worktrees/k-r55 那棵树；跑法见 evidence/K-R55-sandbox-run.py。
# /proc/<pid>/environ 在该读的那一刻是 0 字节」的发生率。
# 被测对象 = 判据的夹具形状本身（不是仓里的 Rust 代码）。
import os, subprocess, sys, time
N = int(sys.argv[1]) if len(sys.argv) > 1 else 200
load = len(sys.argv) > 2 and sys.argv[2] == "load"
busy = []
if load:
    for _ in range(os.cpu_count() * 4):
        busy.append(subprocess.Popen(["sh", "-c", "while :; do :; done"]))

def state_of(pid):
    try:
        with open(f"/proc/{pid}/stat") as f:
            s = f.read()
    except OSError:
        return None
    after = s.rfind(")") + 2
    return s[after:after + 1]

bad_c = 0
bad_b = 0
for _ in range(N):
    # 甲：僵尸（故意不 wait）
    z = subprocess.Popen(["sh", "-c", "exit 0"])
    # 乙：空环境活体
    e = subprocess.Popen(["sleep", "60"], env={})
    # 丙：对照活体（环境非空）
    p = subprocess.Popen(["sleep", "60"], env={"PATH": "/usr/bin:/bin"})
    for _ in range(2000):
        if state_of(z.pid) == "Z":
            break
        time.sleep(0.001)
    def rd(pid):
        try:
            with open(f"/proc/{pid}/environ", "rb") as f:
                return f.read()
        except OSError:
            return None
    rb = rd(e.pid)
    rc = rd(p.pid)
    if rc is None or len(rc) == 0:
        bad_c += 1
    if rb is None or len(rb) != 0:
        bad_b += 1
    e.kill(); e.wait(); p.kill(); p.wait(); z.wait()
for b in busy:
    b.kill(); b.wait()
print(f"N={N} load={load} 丙读回0字节(=判据会红)={bad_c} 乙不是0字节={bad_b}")
