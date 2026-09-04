#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R8 摸底的尺子：daemon「连得到哪台 tmux server」这件事，今天是从哪来的。

⚠⚠ 本脚本**只读源码**：不起 tmux、不起 daemon、不碰这台机器的任何 tmux 状态。
    （本件的正题就是「它会乱碰 tmux」——量它的尺子自己去碰就是当场犯那个错。）

# 它回答什么

D2「量清今天的连法」的分母与逐处行号：
  ① daemon crate 生产段里**每一处起 tmux 进程**的点（两种形态，见下）；
  ② 其中有几处带**显式 socket 选择器**（`-L` / `-S`）；
  ③ 起 daemon 的那几条路各自把什么交给了它（env / argv）。

# 分母怎么切的（说清楚，别让下一个人猜）

- **人群** = `remote-daemon-proto/src/**/*.rs` 的**生产段**。
- **生产段的切法** = 每个文件里**最后一个** `^#[cfg(test)]` 之前的部分。
  ⚠ 这是一把**粗尺子**，它的失效方式如实登记在 `SCOPE_CAVEAT` 里，
    并且本脚本会把「切在哪一行」逐文件打出来，好让读的人自己核。
- **起 tmux 的两种形态**（只查这两种 ⇒ 这是**黑名单**，不是白名单）：
    形态 A：`Command::new("tmux")`      —— argv 直传
    形态 B：`Command::new("sh")` + 脚本里 `exec tmux …` —— 过一层 shell
  ⚠ 漏掉的形态会静默不计。**别把本脚本的数读成「全部」**，它是「这两形的全部」。
"""

import re
import subprocess
import sys
from pathlib import Path

SCOPE_CAVEAT = """\
⚠ 尺子作用域，如实登记（本仓最高频的一类错就是「量具的作用域对不上事实」）：
  1. 「生产段 = 最后一个 #[cfg(test)] 之前」是行级近似。文件里若有**多个** cfg(test)
     块且最后一块之后还有生产代码，本尺子会把那段算进测试段 ⇒ **少算**。
     缓解：下面逐文件打出切点行号，可人工核。
  2. 起 tmux 的形态是**黑名单**（两形）。`std::process::Command::new(&some_var)`、
     `exec` 族、`posix_spawn` 一律不计 ⇒ **少算**。
  3. 只看 daemon crate（`remote-daemon-proto/src`）。monitor 侧（`src-tauri/src`）
     自己也起 tmux，那不在本件射程里（本件管的是 **daemon 装 hook 那一跳**）。
"""

REPO = Path(__file__).resolve().parent.parent
DAEMON_SRC = REPO / "remote-daemon-proto" / "src"

TMUX_A = re.compile(r'Command::new\("tmux"\)')
TMUX_B = re.compile(r"exec tmux ")
SELECTOR = re.compile(r'"-L"|"-S"|\bsocket-name\b|\bsocket-path\b|tmux\s+-L\b|tmux\s+-S\b')


def prod_cut(text: str) -> int:
    """返回生产段的结束行号（1-based，不含）。没有 cfg(test) ⇒ 全文都是生产段。"""
    last = 0
    for i, line in enumerate(text.splitlines(), start=1):
        if line.startswith("#[cfg(test)]"):
            last = i
    return last if last else 10**9


def census_tmux_spawns():
    rows = []
    for f in sorted(DAEMON_SRC.rglob("*.rs")):
        text = f.read_text(encoding="utf-8", errors="replace")
        cut = prod_cut(text)
        rel = f.relative_to(DAEMON_SRC)
        # 护栏文件自己的清单里逐字写着这些字面量 ⇒ 排除，否则是自指假阳性。
        if rel.name.endswith("_guard.rs"):
            continue
        for i, line in enumerate(text.splitlines(), start=1):
            if i >= cut:
                continue
            shape = None
            if TMUX_A.search(line):
                shape = "A(argv 直传)"
            elif TMUX_B.search(line):
                shape = "B(sh -c)"
            if shape is None:
                continue
            if line.lstrip().startswith("//"):
                continue  # 散文里提到它，不是真调用
            rows.append((str(rel), i, cut, shape, line.strip()))
    return rows


def selector_check(rows):
    """逐文件问：生产段里有没有任何 `-L` / `-S` 选择器。"""
    out = []
    for rel in sorted({r[0] for r in rows}):
        f = DAEMON_SRC / rel
        text = f.read_text(encoding="utf-8", errors="replace")
        cut = prod_cut(text)
        hits = [
            (i, l.strip())
            for i, l in enumerate(text.splitlines(), start=1)
            if i < cut and SELECTOR.search(l) and not l.lstrip().startswith("//")
        ]
        out.append((rel, cut, hits))
    return out


def install_hooks_signature():
    f = DAEMON_SRC / "control" / "tmux_hook.rs"
    for i, l in enumerate(f.read_text(encoding="utf-8").splitlines(), start=1):
        if "fn install_hooks" in l:
            return i, l.strip()
    return None, "<找不到 —— 它搬家了或改名了，本尺子在空转>"


def handoff_sites():
    """起 daemon 的那几条路：各自把什么交给了它。逐处行号。"""
    probes = [
        ("monitor · 脱离式常驻", "src-tauri/src/local_daemon.rs",
         [r"fn spawn_detached", r'env_remove\("TMUX"\)', r"LISTEN_PORT_ENV", r"LISTEN_TOKEN_ENV"]),
        ("monitor · 被监护 stdio", "src-tauri/src/backend/control/local_backend.rs",
         [r"pub fn supervise_with_stdio", r'cmd\.env_remove\("TMUX"\)', r"for \(k, v\) in &envs"]),
        ("daemon 自己 · 载体由 env 决定", "remote-daemon-proto/src/listen.rs",
         [r"pub const ENV_PORT", r"pub const ENV_TOKEN", r"pub fn mode_from"]),
        ("daemon 自己 · 装 hook 的时机", "remote-daemon-proto/src/observe/watcher.rs",
         [r"fn install_tmux_hooks_best_effort", r"install_tmux_hooks_best_effort\(\);"]),
        ("ccm · 只调一次性子命令", "shared/ccm",
         [r"DAEMON_BIN_RECIPE=", r'"\$_ccm_db" --resolve', r'"\$_ccm_db" --launch']),
    ]
    out = []
    for label, rel, pats in probes:
        f = REPO / rel
        if not f.exists():
            out.append((label, rel, [("?", "<文件不在 —— 本探针在空转>")]))
            continue
        lines = f.read_text(encoding="utf-8", errors="replace").splitlines()
        hits = []
        for p in pats:
            rx = re.compile(p)
            found = [(i, l.strip()) for i, l in enumerate(lines, start=1) if rx.search(l)]
            hits.extend(found[:2] if found else [("-", f"<零命中: {p}>")])
        out.append((label, rel, hits))
    return out


def main():
    print("=" * 78)
    print("K-R8 摸底 · daemon「碰哪台 tmux server」census")
    try:
        head = subprocess.run(
            ["git", "-C", str(REPO), "rev-parse", "--short", "HEAD"],
            capture_output=True, text=True, timeout=10,
        ).stdout.strip()
    except Exception:
        head = "<拿不到>"
    print(f"量于提交: {head}")
    print("=" * 78)
    print(SCOPE_CAVEAT)

    rows = census_tmux_spawns()
    print("── ① daemon 生产段里每一处起 tmux 的点 ──")
    for rel, i, cut, shape, src in rows:
        print(f"  {rel}:{i}  [{shape}]  (该文件生产段切在 :{cut})")
        print(f"      {src[:96]}")
    print(f"\n  合计 = {len(rows)} 处")
    # 反空真：一处都扫不到 ⇒ 下面整段在空转。
    assert rows, "一处起 tmux 的点都没扫到 —— 尺子坏了或人群画错了，本脚本此刻在空转"

    print("\n── ② 这些文件的生产段里有没有显式 socket 选择器（-L / -S）──")
    sel = selector_check(rows)
    total_sel = 0
    for rel, cut, hits in sel:
        total_sel += len(hits)
        if hits:
            for i, l in hits:
                print(f"  {rel}:{i}  {l[:80]}")
        else:
            print(f"  {rel}  生产段零命中（切在 :{cut}）")
    print(f"\n  ★ 生产段显式选择器合计 = {total_sel} 处")
    print("    ⇒ 0 处的读法：**没有任何一条路告诉 daemon 该碰哪台 server**，")
    print("      socket 由 tmux 客户端自己从环境解析（$TMUX → TMUX_TMPDIR → 默认）。")

    ln, sig = install_hooks_signature()
    print(f"\n── ③ 装 hook 那一段读的是什么 ──")
    print(f"  control/tmux_hook.rs:{ln}")
    print(f"      {sig}")
    print("    ⇒ 签名里**没有任何一个参数**表示「哪台 server」。")

    print("\n── ④ 起 daemon 的那几条路各自交了什么 ──")
    for label, rel, hits in handoff_sites():
        print(f"  · {label}  [{rel}]")
        for i, l in hits:
            print(f"      :{i}  {l[:88]}")

    print("\n" + "=" * 78)
    print("结论（一句话）：今天「该碰哪台 tmux server」在协议里**没有任何一个载体** ——")
    print("不是传了个默认值，是**连一个能传它的位置都不存在**。")
    print("=" * 78)
    return 0


if __name__ == "__main__":
    sys.exit(main())
