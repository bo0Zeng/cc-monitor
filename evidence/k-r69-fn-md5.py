#!/usr/bin/env python3
"""K-R69 改动面：**逐函数** md5（基线 79bf97d ↔ 工作树）。住址即本文件。

⚠ 射程写死，别读宽：它按**大括号配平**切函数体（Rust `fn` / TS `function`），
认不出宏体里的花括号、也认不出 `impl` 块里的关联常量 ⇒ 它给的是「哪几个函数变了」
这一层的读数，**不是**「改动面就这些」。表 / 常量 / 注释的改动由 `git diff --stat` 那一行管。
"""
import hashlib, re, subprocess, sys, pathlib

WT = pathlib.Path("/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r69")
BASE = "79bf97d"
HEAD_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:const\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)", )
TS_RE = re.compile(r"^\s*(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_][A-Za-z0-9_]*)")

def funcs(src, ts):
    out = {}
    lines = src.split("\n")
    i = 0
    while i < len(lines):
        m = (TS_RE if ts else HEAD_RE).match(lines[i])
        if m:
            depth, started, j, buf = 0, False, i, []
            while j < len(lines):
                buf.append(lines[j])
                for c in lines[j]:
                    if c == "{":
                        depth += 1; started = True
                    elif c == "}":
                        depth -= 1
                if started and depth <= 0:
                    break
                j += 1
            out.setdefault(m.group(1), []).append("\n".join(buf))
            i = j + 1
        else:
            i += 1
    return {k: hashlib.md5("\n\n".join(v).encode()).hexdigest()[:12] for k, v in out.items()}

files = subprocess.run(["git", "-C", str(WT), "diff", "--name-only", BASE],
                       capture_output=True, text=True).stdout.split()
for f in files:
    if not (f.endswith(".rs") or f.endswith(".ts")):
        continue
    old = subprocess.run(["git", "-C", str(WT), "show", f"{BASE}:{f}"],
                         capture_output=True, text=True).stdout
    new = (WT / f).read_text()
    ts = f.endswith(".ts")
    a, b = funcs(old, ts), funcs(new, ts)
    changed = [(k, a.get(k, "（新增）"), b[k]) for k in b if a.get(k) != b[k]]
    gone = [k for k in a if k not in b]
    if changed or gone:
        print(f"\n== {f}")
        for k, o, n in sorted(changed):
            print(f"   {k:<62} {o} → {n}")
        for k in sorted(gone):
            print(f"   {k:<62} （本文件里没了）")
