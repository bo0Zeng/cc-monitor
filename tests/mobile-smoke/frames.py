#!/usr/bin/env python3
# 冒烟台架的帧客户端：经台架 sshd 开一条 `ccm -- --resident-attach`（和手机同一条路），问一句、印回话。
# 用法：frames.py <命令> [JSON 入参] [期限毫秒]   ·   frames.py watch <秒>（只看流上的帧）   例：frames.py session-open '{"what":{"kind":"new","agent":"claude","cwd":"~/work","prompt":"hi"},"run":"tmux","local":false,"ticket":"t1"}' 45000
# 回话那一行原样印到 stdout；期间流上来的帧（会话帧 · line 帧）印到 stderr，带收到的时刻（秒）。
import json
import os
import subprocess
import sys
import time

here = os.path.dirname(os.path.abspath(__file__))
cmd = sys.argv[1]
args = json.loads(sys.argv[2]) if len(sys.argv) > 2 and cmd != "watch" else None
within = int(sys.argv[3]) if len(sys.argv) > 3 else 20000

# 常驻不在就先起（和手机一样：attach 不上 ⇒ --resident-ensure 再接）。
subprocess.run(["bash", os.path.join(here, "rig.sh"), "ssh", '"$HOME"/.cc-monitor/bin/ccm -- --resident-ensure'],
               stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, check=False)
p = subprocess.Popen(["bash", os.path.join(here, "rig.sh"), "ssh", '"$HOME"/.cc-monitor/bin/ccm -- --resident-attach'],
                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
t0 = time.time()


def read():
    line = p.stdout.readline()
    if not line:
        sys.exit("frames: 流断了")
    return json.loads(line)


hello = read()
p.stdin.write(json.dumps({"attach": True, "tz": "UTC"}) + "\n")
while read().get("attach") != "ok":
    pass
if cmd == "watch":
    end = t0 + float(sys.argv[2])
    import select
    while time.time() < end:
        if select.select([p.stdout], [], [], 0.5)[0]:
            print(f"{time.time() - t0:6.2f} {json.dumps(read(), ensure_ascii=False)}", flush=True)
    p.terminate()
    sys.exit(0)
p.stdin.write(json.dumps({"id": "smoke-1", "cmd": cmd, "args": args, "within_ms": within}) + "\n")
while True:
    m = read()
    if m.get("id") == "smoke-1":
        print(json.dumps(m, ensure_ascii=False))
        break
    print(f"{time.time() - t0:6.2f} {json.dumps(m, ensure_ascii=False)[:300]}", file=sys.stderr)
p.stdin.close()
p.terminate()
