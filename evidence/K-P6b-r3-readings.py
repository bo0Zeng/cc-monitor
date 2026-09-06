#!/usr/bin/env python3
"""K-P6b 第三轮的读数量具。**只读盘上文本与 git，一次 cargo 都不跑、一个进程都不起。**

被测对象 = **本脚本所在的那棵工作树**（按 `__file__` 往上两级定位仓根，不读任何环境变量、
不认 cwd）。跑法：

    python3 evidence/K-P6b-r3-readings.py            # 量当前盘上
    python3 evidence/K-P6b-r3-readings.py <base>     # 顺带对 <base> 做两版差（默认 f10581c）

# 每一格的分母都印在它自己那一行旁边

本仓的纪律：报一个数就要同句给分母。下面每个 `§` 各自印自己的分母与口径，
**不要把某一格的分母拿去读另一格**（本工作区最贵的病是「量具的作用域对不上事实」）。

# 它买不到的（别读成证明）

- 剥法是**近似**：`strip_test_mods` 按「`#[cfg(test)]` 打头的 `mod` 块 + 列 0 右大括号收尾」剥，
  与 Rust 侧 `guard_core::production_code` **不是同一份实现** ⇒ 两边的数可能差。
  **要以判据自己印的数为准**，本脚本给的是给人读的旁证。
- 它数不到 `use` 别名、函数指针、宏里拼出来的调用（与 `K-P6-dial-census.py` 头注同一个洞）。
- 它**一个 target 都没编**：musl 交叉编译与 windows-msvc 原生编译两格，本脚本一个字都不证。
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def strip_test_mods(src: str) -> str:
    """剥掉 `#[cfg(test)] mod …{ … }` 块（列 0 右大括号收尾）。**近似，见模块头注。**"""
    lines = src.splitlines(keepends=True)
    out, i = [], 0
    while i < len(lines):
        if lines[i].lstrip().startswith("#[cfg(test)]"):
            j = i + 1
            while j < len(lines) and "mod " not in lines[j]:
                if lines[j].strip() and not lines[j].lstrip().startswith("#"):
                    break
                j += 1
            if j < len(lines) and "mod " in lines[j]:
                k = j + 1
                while k < len(lines) and lines[k].rstrip("\n") != "}":
                    k += 1
                i = k + 1
                continue
        out.append(lines[i])
        i += 1
    return "".join(out)


def git(*args):
    return subprocess.run(
        ["git", "-C", str(ROOT), *args], capture_output=True, text=True
    ).stdout


def call_sites(code: str, name: str) -> int:
    """`name(` 的**调用点**处数 —— 剔掉定义行（前面紧挨着 `fn `）。"""
    needle = f"{name}("
    n, at = 0, 0
    while True:
        i = code.find(needle, at)
        if i < 0:
            return n
        at = i + len(needle)
        if code[:i].endswith("fn "):
            continue
        n += 1


def main():
    base = sys.argv[1] if len(sys.argv) > 1 else "f10581c"
    head = git("rev-parse", "--short", "HEAD").strip()
    print(f"# K-P6b r3 读数 · 量于 {head}（对照 base {base}）")
    print(f"# 仓根 = {ROOT}")

    # ── §① 界面侧拨号调用点：7 处 / 3 份 ──────────────────────────────────────
    print("\n## §① 界面侧 `connect_session` 的生产调用点")
    print("  分母 = `src-tauri/src` 下这三份文件的生产段（剥测试段），剔掉定义行")
    total = 0
    for f in ["ssh_source.rs", "sftp.rs", "port_forward.rs"]:
        code = strip_test_mods((ROOT / "src-tauri" / "src" / f).read_text(encoding="utf-8"))
        n = call_sites(code, "connect_session")
        total += n
        print(f"  {f}: {n}")
    print(f"  合计 = {total}  ← 「7 处里搬走 1 处」那句话里的 7")
    print("  ⚠ **本件一处都没删** —— 搬走的是「daemon 长连接流的入口还走不走到它」，")
    print("     不是「这几处少了一处」。量名字量到的会是「什么都没变」。")

    # ── §② 代理侧拨号锚点：只许住 dial/ ─────────────────────────────────────
    print("\n## §② 代理侧（daemon crate）的拨号锚点")
    anchors = ["client::connect(", "client::connect_stream(", "channel_open_direct_tcpip("]
    print(f"  锚点 = {anchors}")
    print("  分母 = `remote-daemon-proto/src` 递归全部 `.rs` 的生产段")
    hits, files = 0, 0
    strays = []
    for p in sorted((ROOT / "remote-daemon-proto" / "src").rglob("*.rs")):
        code = strip_test_mods(p.read_text(encoding="utf-8", errors="replace"))
        files += 1
        rel = p.relative_to(ROOT / "remote-daemon-proto" / "src").as_posix()
        n = sum(code.count(a) for a in anchors)
        if n:
            hits += n
            print(f"  {rel}: {n}")
            if not rel.startswith("dial/"):
                strays.append(rel)
    print(f"  扫了 {files} 份 · 命中合计 {hits} · 住 `dial/` 外面的 {len(strays)} 份 {strays}")

    # ── §③ `--dial` 在 main.rs 生产段的两处 ─────────────────────────────────
    print("\n## §③ `--dial` 在 `main.rs` 生产段的落点（表 1 处 + 臂 1 处 = 2）")
    main_prod = strip_test_mods((ROOT / "remote-daemon-proto" / "src" / "main.rs").read_text(encoding="utf-8"))
    print(f'  "--dial" 处数 = {main_prod.count(chr(34) + "--dial" + chr(34))}  （分母 = main.rs 生产段全文）')
    print(f"  dial::run( 处数 = {main_prod.count('dial::run(')}")
    print("  ⚠ 摘掉臂那一处，daemon 侧 596 条判据**一条都不红**（本轮 7u 探针实打）——")
    print("     那一格现由 `dial::tests::the_dial_arm_is_actually_wired_into_the_dispatch` 补上。")

    # ── §④ 两份 lock 里 russh 那棵子树 ──────────────────────────────────────
    print("\n## §④ 两份 `Cargo.lock`（分母 = 各自文件里全部 `^name = \"…\"` 行）")
    for name, rel in [("daemon", "remote-daemon-proto/Cargo.lock"), ("monitor", "src-tauri/Cargo.lock")]:
        txt = (ROOT / rel).read_text(encoding="utf-8")
        pkgs = re.findall(r'^name = "(.*)"$', txt, re.M)
        russh = [p for p in pkgs if p.startswith("russh")]
        cb = re.findall(r'^name = "crypto-bigint"\n^version = "(.*)"$', txt, re.M)
        print(f"  {name}: 包 {len(pkgs)} 个 · russh* {len(russh)} 个 {sorted(set(russh))} · crypto-bigint {cb}")
    old = git("show", f"{base}:remote-daemon-proto/Cargo.lock")
    if old:
        print(f"  daemon@{base}: 包 {len(re.findall(chr(94) + 'name = ', old, re.M))} 个"
              f" · russh* {len([1 for m in re.findall(r'^name = ' + chr(34) + '(.*)' + chr(34) + '$', old, re.M) if m.startswith('russh')])} 个")
    print("  🔴 `crypto-bigint 0.7.3` 在 crates.io 上是 **yanked**（0.7.0…0.7.4 全 yanked，只 0.7.5 没被 yank）；")
    print("     断网沙箱的缓存里只有 0.7.3 那一份 ⇒ `cargo` 解析时拒选 ⇒ lock 由")
    print("     `evidence/K-P6b-r3-lock-prime.py` 从 monitor 那份播种。两侧从此钉在同一版上。")

    # ── §⑤ 射程钉死（D5）：协议面 / 六根针 / detach 闸 ────────────────────────
    # 🔴 比的是 `base` 与**工作树**，不是 `base` 与 `HEAD`。
    #    第一版写的是后者，而落盘还没提交时 `HEAD == base` ⇒ 每一面都 0 字节、
    #    **连非空对照也 0 字节** —— 那是一次「命令跑了但两边是同一个东西」的假绿，
    #    正是本仓那条「差集为空要附非空对照」的纪律要逮的形状。**本轮自己撞到过一次。**
    print(f"\n## §⑤ `D5` 射程（`git diff {base} -- <面>`，base ↔ **工作树**；分母 = 各面点名的那几份文件）")
    faces = [
        ("协议面", ["remote-daemon-proto/src/wire.rs", "remote-daemon-proto/src/inbound.rs"]),
        ("六根针", ["remote-daemon-proto/src/single_stream_guard.rs"]),
        ("detach 闸", ["src-tauri/src/local_daemon.rs"]),
        ("agent 局部性", ["remote-daemon-proto/src/agent_locality_guard.rs"]),
    ]
    for label, paths in faces:
        d = git("diff", base, "--", *paths)
        print(f"  {label}: diff {len(d)} 字节 {'（空）' if not d else '🔴 非空'}  ← {paths}")
    ctrl = git("diff", base, "--", "remote-daemon-proto/src/dial/mod.rs")
    print(f"  **非空对照**（同一把尺子量 dial/mod.rs）: {len(ctrl)} 字节")
    print("  ⇒ 上面那几个 0 是「跑了、结果是空」，不是「命令没跑」。")

    # ── §⑥ 改动面：逐函数 md5（base ↔ 工作树）─────────────────────────────────
    print("\n## §⑥ 改动面 · **顶层函数逐个 md5**")
    print("  ⚠ **口径先说清**：`brief.md` 要的是「`ast` 逐函数 md5」，")
    print("     而本仓与 skill 里**都没有**一个叫 `ast` 的量具（`~/.claude/skills/planned-build/bin/`")
    print("     与 `scripts/` 都查过，零命中）⇒ 这里是**我自己按文本切**的顶层函数体再取 md5，")
    print("     **不是 AST**：宏里生成的函数、`impl` 块里的方法都切不出来。别读成等价物。")
    print("  分母 = 每份文件生产段里**列 0 起头**的 `fn` / `pub fn` / `pub(crate) fn` / `async fn`，")
    print("        函数体到第一行**恰好是 `}`** 为止（与 `local_daemon.rs::body_of` 同一把尺子）。")
    for rel in ["src-tauri/src/ssh_source.rs", "remote-daemon-proto/src/dial/mod.rs"]:
        now = strip_test_mods((ROOT / rel).read_text(encoding="utf-8"))
        old = strip_test_mods(git("show", f"{base}:{rel}"))
        a, b = top_level_fns(old), top_level_fns(now)
        added = sorted(set(b) - set(a))
        removed = sorted(set(a) - set(b))
        changed = sorted(k for k in set(a) & set(b) if a[k] != b[k])
        same = len(set(a) & set(b)) - len(changed)
        print(f"  {rel}: base {len(a)} 个 · 工作树 {len(b)} 个 · 新增 {len(added)} · 消失 {len(removed)} · 变了 {len(changed)} · 一字未动 {same}")
        for k in added:
            print(f"    新增 {k}  md5={b[k]}")
        for k in removed:
            print(f"    消失 {k}  md5(base)={a[k]}")
        for k in changed:
            print(f"    变了 {k}  {a[k]} → {b[k]}")

    return 0


FN_HEAD = re.compile(r"^(?:pub(?:\([a-z]+\))? )?(?:async )?fn ([A-Za-z0-9_]+)")


def top_level_fns(src: str) -> dict:
    """列 0 起头的顶层函数 → 函数体 md5。**按文本切，不是 AST**（见 §⑥ 的口径行）。"""
    import hashlib

    out, lines, i = {}, src.splitlines(keepends=True), 0
    while i < len(lines):
        m = FN_HEAD.match(lines[i])
        if not m:
            i += 1
            continue
        j = i
        body = []
        while j < len(lines):
            body.append(lines[j])
            if lines[j].rstrip("\n") == "}":
                break
            j += 1
        out[m.group(1)] = hashlib.md5("".join(body).encode()).hexdigest()[:12]
        i = j + 1
    return out


if __name__ == "__main__":
    raise SystemExit(main())
