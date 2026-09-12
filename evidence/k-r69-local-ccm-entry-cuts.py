#!/usr/bin/env python3
"""K-R69 变异台。**住址即本文件**（只属于本轮，被测对象指向 .claude/worktrees/k-r69）。

每一刀：断言锚点在目标文件里**恰好命中 N 次**（不对就整刀作废、不跑）→ 落地 → 打印
「变异已落地」→ 由调用方在沙箱里跑判据 → 无论结果如何都**整份还原**（内存快照，
不走 `git checkout`）。
"""
import subprocess, sys, pathlib, json

WT = pathlib.Path("/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r69")
PROJ = "/home/zbl/文档/claudecode-frontend"
SKILL = "/home/zbl/.claude-accts/z/skills/planned-build"

# 🔴 判据一律进沙箱（`DECISIONS.md#R21`）。下面这段与 `.claude/devbox/gate` 的
# `docker run` **逐字同一段**，只把末尾那条命令换成参数 —— 挂什么 / 不挂什么、
# 为什么不挂 `/tmp/tmux-1000` 与 `~/.cc-monitor`，理由全文住那个脚本，这里不复述。
#
# ⚠ **登记一处「量具在读数之后动过」**：跑那九刀时，这段是 scratchpad 里一个 8 行的
#   shell 包装（`bash -o pipefail -c`），本文件调它。收工时把它内联到这里 ——
#   理由是 `shell_lint_registry` 当场逮到「仓里多了一个从没被 lint 过的 .sh」，
#   而给 evidence 里的一次性量具开一条 lint 豁免会把那张只有一条的表变成垃圾桶。
#   **命令内容逐字未变**（同一个镜像、同一组 -v/-e、同一句 mkdir + 同一条 `bash -o pipefail -c`）；
#   变的只有「谁来起 docker」。读数不重跑就照旧成立，重跑也会得到同一份。
def sandbox(cmd: str):
    return subprocess.run(
        ["docker", "run", "--rm", "--network", "none",
         "-v", f"{PROJ}:{PROJ}", "-v", f"{SKILL}:{SKILL}:ro",
         "-v", "ccmon-cargo-registry:/opt/rust/cargo/registry",
         "-e", f"CARGO_TARGET_DIR={PROJ}/.claude/pm-targets/k-r69",
         "-e", "HOME=/home/zbl", "-e", "PB_WS=backend-consolidation",
         "-w", str(WT), "ccmon-devbox:latest",
         "bash", "-o", "pipefail", "-c",
         f'mkdir -p "$HOME/.claude/projects" && {cmd}'],
        capture_output=True, text=True, timeout=3600)

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
        r = sandbox(cmd)
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
