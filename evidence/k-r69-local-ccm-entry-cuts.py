#!/usr/bin/env python3
"""K-R69 变异台。**住址即本文件**（只属于本轮，被测对象指向 .claude/worktrees/k-r69）。

每一刀：断言锚点在目标文件里**恰好命中 N 次**（不对就整刀作废、不跑）→ 落地 → 打印
「变异已落地」→ 由调用方在沙箱里跑判据 → 无论结果如何都**整份还原**（内存快照，
不走 `git checkout`）。
"""
import subprocess, sys, pathlib, json

WT = pathlib.Path("/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r69")
SANDBOX = "/tmp/claude-1000/-home-zbl----claudecode-frontend/f80a7aea-0ccf-4854-9b7a-12961a9c55aa/scratchpad/kr69-sandbox.sh"

def run_cut(name, edits, cmd):
    """edits: [(相对路径, 锚点, 替换)]；cmd: 在沙箱里跑的那条命令。"""
    snaps = {}
    ok = True
    hits = []
    for rel, anchor, new in edits:
        p = WT / rel
        src = p.read_text()
        snaps[p] = src
        n = src.count(anchor)
        hits.append((rel, n))
        if n != 1:
            ok = False
    if not ok:
        for p, s in snaps.items():
            p.write_text(s)
        return {"cut": name, "anchors": hits, "status": "锚点命中数不是 1 —— 整刀作废，没跑"}
    for rel, anchor, new in edits:
        p = WT / rel
        p.write_text(p.read_text().replace(anchor, new, 1))
    print(f"[{name}] 变异已落地，锚点命中 {hits}", flush=True)
    try:
        r = subprocess.run([SANDBOX, cmd], capture_output=True, text=True, timeout=3600)
        out = r.stdout + r.stderr
    finally:
        for p, s in snaps.items():
            p.write_text(s)
    return {"cut": name, "anchors": hits, "exit": r.returncode, "out": out}

if __name__ == "__main__":
    spec = json.load(open(sys.argv[1]))
    res = run_cut(spec["name"], [tuple(e) for e in spec["edits"]], spec["cmd"])
    print(json.dumps({k: v for k, v in res.items() if k != "out"}, ensure_ascii=False))
    print("----- 输出 -----")
    print(res.get("out", "")[-4000:])
