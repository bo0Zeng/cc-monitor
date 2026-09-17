#!/usr/bin/env python3
"""PM 量具（只读文本，不跑任何测试）：源码里「file.rs:行号」这类地址有多少已经烂了。

尺子作用域，先写出来（本仓最高频的一类错就是这个没写）：
  · 分母 = 主树 HEAD 里 git 跟踪的 .rs 文件，排 vendor/ 与 code-picture-core。
  · 只认 `xxx.rs:数字` 这一种形状 ⇒ `.sh:N` / `.ts:N` 的引用**不在本尺子里**
    （已知至少 4 处 gate.sh:N 在 capability_registry.rs 里，且已现打是馊的）。
  · 「馊没馊」只在**能判**的子集上判：引用旁边（同行或前后 2 行内）带了
    「…」或 `…` 引文的，才拿去与被引行 ±3 行比对。判不了的单列一类，不算绿也不算红。
"""
import re, subprocess, os, sys
from pathlib import Path

ROOT = Path("/home/zbl/文档/claudecode-frontend/cc-monitor")
files = [f for f in subprocess.run(["git","-C",str(ROOT),"ls-files","*.rs"],
         capture_output=True,text=True).stdout.split()
         if not f.startswith("vendor/") and "code-picture-core" not in f]

# 基名 -> 盘上路径（唯一才用得上）
bybase = {}
for f in files:
    bybase.setdefault(os.path.basename(f), []).append(f)

REF = re.compile(r'([A-Za-z0-9_/.\-]+\.rs):(\d+)(?:-(\d+))?')
QUOTE = re.compile(r'[「『]([^」』]{6,})[」』]|`([^`\n]{6,})`')

tot=stale=ok=unjudged=noresolve=0
rows=[]
for f in files:
    lines = (ROOT/f).read_text(encoding="utf-8",errors="replace").splitlines()
    for i,ln in enumerate(lines):
        for m in REF.finditer(ln):
            tot+=1
            cited, n = m.group(1), int(m.group(2))
            end = int(m.group(3)) if m.group(3) else n
            base = os.path.basename(cited)
            cands = bybase.get(base, [])
            # 尽量用路径后缀消歧
            cands2 = [c for c in cands if c.endswith(cited)] or cands
            if len(cands2)!=1:
                noresolve+=1; continue
            tgt = (ROOT/cands2[0]).read_text(encoding="utf-8",errors="replace").splitlines()
            if n > len(tgt):
                stale+=1; rows.append((f,i+1,cited,n,"越界：被引文件只有 %d 行"%len(tgt))); continue
            # 找引文：同行 + 前后 2 行
            ctx = "\n".join(lines[max(0,i-2):i+3])
            qs = [g for g in (x for t in QUOTE.findall(ctx) for x in t) if g and ".rs" not in g]
            if not qs:
                unjudged+=1; continue
            window = "\n".join(tgt[max(0,n-4):end+3])
            hit = any(q.strip() in window for q in qs)
            if hit: ok+=1
            else:
                stale+=1
                rows.append((f,i+1,cited,n,"引文 %r 不在被引行 ±3 内"%qs[0][:40]))

print(f"总命中 {tot}  |  判得了：绿 {ok} · 红 {stale}  |  判不了 {unjudged}（旁边没引文）  |  认不出被引文件 {noresolve}")
print()
for r in rows:
    print(f"  红  {r[0]}:{r[1]}  引 {r[2]}:{r[3]}  —— {r[4]}")
