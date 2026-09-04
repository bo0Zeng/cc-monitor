#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R21 第二把尺子：`KR21D3` ——「那一刻走的是三支里的哪一支」。

# 它要答什么

`K-G8` 把那条判据打到 **2 红 / 224 趟**，而且**两拍红在方向相反的两条断言上**：
  · 摸底拍 2 红在 `accounts_query.rs:2024`（**非空对照**：只剩父一条时 token 回不来）
  · 落地拍 2 红在 `accounts_query.rs:2007`（**撞了之后父那条也不许留**）
成因只看出一半（`proc_env_var` 的三义 `None`），**另一半 —— 那一刻为什么读回空 —— 没有观测点**。

🔴 **判据本体一个字都不许改**（件文件 `§3`）。所以本尺子**不碰那条测试**，
而是把它的**夹具形状与判定逻辑逐条在 Python 里重演**，在重演里**装上那个缺掉的观测点**：
每一次 `/proc/<pid>/environ` 读都记下它**落进了哪一支**。

# 🔴 三支之外还有第四态 —— 这是本尺子的主要发现，先写在这里

`proc.rs::proc_env_var` 的分支是：
  支一 `:87`  `fs::read(...).ok()?`      读**出错**            ⇒ None
  支二 `:95`  `v.is_empty()`             键在、值是**空串**     ⇒ None
  支三 `:100` 落到函数尾                  **压根没这个键**       ⇒ None

而 `/proc/<pid>/environ` 还有一个状态：**读成功、但回来 0 字节**。
`fs::read` 给 `Ok(vec![])` ⇒ `for entry in bytes.split(..)` 一圈都不转 ⇒ **落到 `:100`**。
⇒ 这一态**在代码里被当成支三「压根没这个键」**，而事实是「**这一刻读不出来**」。
本尺子把它单列成 `ZERO`，**不并进支三** —— 并进去就正好复刻了那个病。

# 分类（每一次读一个标签，分母 = 读的次数，不是趟数）

  `ERR:<errno>` 读抛异常          → 支一
  `ZERO`        读到 0 字节        → 落在支三的代码路上，事实是「读不出来」（第四态）
  `EMPTYVAL`    键在、值是空串      → 支二
  `MISS`        有内容但没这个键    → 支三（真的没有）
  `OK`          键在、值非空        → 唯一不返回 None 的一支

# 夹具：与判据本体逐条同形（同形的地方逐条列出来，不同的地方也列出来）

同形：
  · 父 `sh -c "sleep 60 & printf '%s\\n' \\"$!\\"; exec sleep 60"`，环境里带 TOKEN
  · 子 = 那个后台 `sleep`，TOKEN **继承**来
  · 第三条 `evil` = `sh -c "exec sleep 60"`，TOKEN 形状不合格
  · 读 pid → 读 `/proc/<pid>/stat` 取 starttime（判据本体 `write_pidfile` 那一步）→ 读 environ
  · `launch_id_is_safe` 与 `suppress_inherited_launch_ids` **逐条重写**（见下），
    再按判据本体那 6 条断言判红，**红的那条按它在源码里的真实行号报**

⚠ **不同形的地方（射程边界，别读宽）**：
  · 重演的是 Python，**比 Rust 慢** ⇒ 从 spawn 到 environ 读之间的墙钟**更长**
    ⇒ 若窗口是「exec 期间的一小段」，本尺子只会**低估**命中率，不会高估。
  · 判据本体跑在 `cargo test` 的**多线程并发**里（几百条测试抢 CPU/进程表），
    本尺子默认**单趟串行** ⇒ 用 `--load N` 可以加 N 个抢 CPU 的进程逼近那个条件。
  · 本尺子**不证明**那 2 红就是这个机制造成的 —— 那两趟当时**没有任何分支级观测点**，
    事后无法复原。它证明的是「**这个机制存在、可复现、且长出的正是那两个相反的面**」。
    🔴 **这条差别必须带着转述**：可复现的机制 ≠ 已证的成因。

# 用法（沙箱里，工作目录 = 工作树根）

    python3 evidence/K-R21-environ-branch-probe.py --runs 300 [--load 8] [--window]

`--window` 额外跑一段：spawn 之后**紧贴着**连读 environ，量那个 0 字节窗口有多长。
"""

from __future__ import annotations

import argparse
import collections
import os
import subprocess
import sys
import time

TOKEN = "kp5f-live-0198f0d2-1111-4222-8333"   # 判据本体 :1929 逐字
BAD = "not a token; rm -rf /"                  # 判据本体 :1958 逐字
KEY = b"CCM_LAUNCH_ID"

# 判据本体里那 6 条断言各自的行号（`accounts_query.rs`，量于 118259b）
LINES = {
    "both.parent.alive": 1992,
    "both.child.alive": 1996,
    "both.child.launchId==null": 2000,
    "both.parent.launchId==null": 2007,
    "both.badshape.alive": 2013,
    "both.badshape.launchId==null": 2017,
    "alone.parent.launchId==TOKEN": 2024,
}


def read_env_branch(pid: int, key: bytes) -> tuple[str, str | None]:
    """一次 environ 读 → (分支标签, 值)。**这就是那个缺掉的观测点。**"""
    try:
        with open(f"/proc/{pid}/environ", "rb") as fh:
            raw = fh.read()
    except OSError as e:
        return f"ERR:{e.errno}", None
    if len(raw) == 0:
        return "ZERO", None            # ← 第四态：代码把它当支三
    for entry in raw.split(b"\0"):
        if not entry:
            continue
        if entry.startswith(key + b"="):
            v = entry[len(key) + 1:]
            if v == b"":
                return "EMPTYVAL", None    # 支二
            return "OK", v.decode("utf-8", "replace")
    return "MISS", None                    # 支三（真的没有）


def starttime(pid: int) -> int | None:
    """判据本体 `write_pidfile` 那一步：`/proc/<pid>/stat` 的 field 22。"""
    try:
        with open(f"/proc/{pid}/stat", "r") as fh:
            stat = fh.read()
    except OSError:
        return None
    try:
        after = stat[stat.rindex(")") + 1:]
        return int(after.split()[22 - 3])
    except (ValueError, IndexError):
        return None


def launch_id_is_safe(v: str | None) -> bool:
    """`accounts_query.rs:367` 逐条重写。"""
    if not v or len(v.encode()) > 128:
        return False
    return all(c.isascii() and (c.isalnum() or c in "-_") for c in v)


def suppress(rows: dict[str, str | None]) -> dict[str, str | None]:
    """`accounts_query.rs:428` 逐条重写：撞了的**全部**置 None。"""
    cnt: collections.Counter = collections.Counter(v for v in rows.values() if v)
    return {k: (None if (v and cnt[v] > 1) else v) for k, v in rows.items()}


def one_run(load_procs: int) -> tuple[list[str], dict[str, str], float]:
    """跑一趟完整夹具，回 (红了哪几条断言, 每次读落在哪一支, spawn→pass② 的秒数)。

    ⚠ 那个秒数是为了验一条**独立的**假说：判据本体的活体是 `sleep 60`，
    若 spawn 到 pass ② 之间真能拖过 **60 秒**，父进程会自己死掉 ⇒ `alive:false`
    ⇒ `alone[parent].launchId` 变 `null` ⇒ **正好红在 `:2024`**（摸底拍那个面）。
    量它就是为了说清「这条路今天离得有多远」。
    """
    t_spawn = time.perf_counter()
    parent = subprocess.Popen(
        ["sh", "-c", "sleep 60 & printf '%s\\n' \"$!\"; exec sleep 60"],
        env={**os.environ, "CCM_LAUNCH_ID": TOKEN},
        stdout=subprocess.PIPE,
    )
    assert parent.stdout is not None
    cpid = int(parent.stdout.readline().strip())
    ppid = parent.pid
    evil = subprocess.Popen(
        ["sh", "-c", "exec sleep 60"],
        env={**os.environ, "CCM_LAUNCH_ID": BAD},
    )
    epid = evil.pid

    branches: dict[str, str] = {}
    alive: dict[str, bool] = {}
    # ── pass ①：三条 pidfile 都在 ──────────────────────────────────────
    #    判据本体的顺序：先 write_pidfile 三条（读 stat），再 session_accounts（读 environ）
    st = {n: starttime(p) for n, p in (("parent", ppid), ("child", cpid), ("badshape", epid))}
    raw1: dict[str, str | None] = {}
    for name, pid in (("parent", ppid), ("child", cpid), ("badshape", epid)):
        # `session_process_identity_ok`：pidfile 的 procStart 与现读的相等
        alive[name] = st[name] is not None and starttime(pid) == st[name]
        if alive[name]:
            br, v = read_env_branch(pid, KEY)
            branches[f"1.{name}"] = br
            raw1[name] = v if launch_id_is_safe(v) else None
        else:
            branches[f"1.{name}"] = "DEAD"
            raw1[name] = None
    both = suppress(raw1)

    # ── pass ②：删掉子那条 pidfile，只剩父 + badshape ──────────────────
    raw2: dict[str, str | None] = {}
    for name, pid in (("parent", ppid), ("badshape", epid)):
        a = st[name] is not None and starttime(pid) == st[name]
        if a:
            br, v = read_env_branch(pid, KEY)
            branches[f"2.{name}"] = br
            raw2[name] = v if launch_id_is_safe(v) else None
        else:
            branches[f"2.{name}"] = "DEAD"
            raw2[name] = None
    alone = suppress(raw2)
    elapsed = time.perf_counter() - t_spawn

    for p in (parent, evil):
        p.kill()
        p.wait()
    subprocess.run(["kill", str(cpid)], stderr=subprocess.DEVNULL)

    red: list[str] = []
    if not alive["parent"]:
        red.append("both.parent.alive")
    if not alive["child"]:
        red.append("both.child.alive")
    if both["child"] is not None:
        red.append("both.child.launchId==null")
    if both["parent"] is not None:
        red.append("both.parent.launchId==null")
    if not alive["badshape"]:
        red.append("both.badshape.alive")
    if both["badshape"] is not None:
        red.append("both.badshape.launchId==null")
    if alone["parent"] != TOKEN:
        red.append("alone.parent.launchId==TOKEN")
    return red, branches, elapsed


def window_probe(iters: int) -> None:
    """紧贴 spawn 连读 environ：那个「读得到但是空的」窗口到底有多长。"""
    print("## `--window`：spawn 之后紧贴着连读 `/proc/<pid>/environ`，量 0 字节窗口")
    print("   （每行一次 spawn；`reads` = 直到第一次读出内容为止读了几次，`us` = 墙钟微秒）")
    zero_runs = 0
    for i in range(iters):
        p = subprocess.Popen(["sh", "-c", "exec sleep 5"],
                             env={**os.environ, "CCM_LAUNCH_ID": TOKEN})
        t0 = time.perf_counter()
        seq: list[str] = []
        for _ in range(2000):
            br, _v = read_env_branch(p.pid, KEY)
            seq.append(br)
            if br == "OK":
                break
        dt = (time.perf_counter() - t0) * 1e6
        p.kill()
        p.wait()
        head = collections.Counter(seq[:-1])
        if head:
            zero_runs += 1
            print(f"  {i:>3}  reads={len(seq):<5} us={dt:>8.0f}  读到内容之前：{dict(head)}")
    print(f"\n  ⇒ {iters} 次 spawn 里，有 **{zero_runs}** 次在读到内容之前先读到过非 `OK` 的一支。")
    print()


RUST_CHECK = r'''
// K-R21：把 `proc.rs::proc_env_var` 的**读那一步**逐字照抄，问一件事 ——
// `/proc/<pid>/environ` 在 exec 窗口里回 0 字节时，`std::fs::read` 给的是
// `Err`（⇒ 支一 `.ok()?`）还是 `Ok(vec![])`（⇒ 一圈不转、落到函数尾 = 支三）？
// 这一步 Python 替不了：`.ok()?` 是 Rust 的语义，必须用 Rust 现打。
fn main() {
    let mut zero_ok = 0u32;   // Ok 且 0 字节 —— 落到支三
    let mut zero_err = 0u32;  // Err —— 落到支一
    let mut got = 0u32;
    for _ in 0..400 {
        let mut c = std::process::Command::new("sh")
            .arg("-c").arg("exec sleep 5")
            .env("CCM_LAUNCH_ID", "kp5f-live-0198f0d2-1111-4222-8333")
            .spawn().expect("spawn");
        let pid = c.id();
        // 与 proc.rs:87 逐字同一句
        match std::fs::read(format!("/proc/{pid}/environ")) {
            Ok(b) if b.is_empty() => zero_ok += 1,
            Ok(_) => got += 1,
            Err(_) => zero_err += 1,
        }
        let _ = c.kill();
        let _ = c.wait();
    }
    println!("Ok(空)={zero_ok}  Err={zero_err}  Ok(有内容)={got}");
    println!("⇒ `Ok(空)` 那一列走的是 `:100`（支三「压根没这个键」），不是 `:87`（支一）。");
}
'''


def rust_check() -> None:
    """🔴 用 Rust 现打：0 字节读到底落在哪一支。"""
    import tempfile
    print("## `--rust-check`：`std::fs::read` 在 0 字节 environ 上给 Ok 还是 Err")
    with tempfile.TemporaryDirectory() as td:
        src = os.path.join(td, "k_r21_check.rs")
        with open(src, "w", encoding="utf-8") as fh:
            fh.write(RUST_CHECK)
        exe = os.path.join(td, "k_r21_check")
        r = subprocess.run(["rustc", "-O", "-o", exe, src], capture_output=True, text=True)
        if r.returncode != 0:
            print("  ❌ rustc 编不过 —— 本格没有读数，别当成跑过了：")
            print("  " + (r.stderr or "").strip()[:800])
            return
        out = subprocess.run([exe], capture_output=True, text=True)
        for line in (out.stdout + out.stderr).splitlines():
            print("  " + line)
    print()


def main() -> int:
    ap = argparse.ArgumentParser(description="K-R21：那一刻走的是哪一支")
    ap.add_argument("--runs", type=int, default=300)
    ap.add_argument("--load", type=int, default=0, help="加 N 个抢 CPU 的进程逼近 cargo test 的并发条件")
    ap.add_argument("--window", action="store_true")
    ap.add_argument("--rust-check", action="store_true",
                    help="用 Rust 现打：0 字节读落在支一还是支三")
    args = ap.parse_args()

    if not os.path.exists("/.dockerenv"):
        print("❌ 本尺子只许在沙箱里跑（K31）。没看见 /.dockerenv ⇒ 拒绝执行。", file=sys.stderr)
        return 3

    load = []
    for _ in range(args.load):
        load.append(subprocess.Popen(["sh", "-c", "while :; do :; done"]))
    try:
        if args.rust_check:
            rust_check()
        if args.window:
            window_probe(30)

        print(f"## 主段：{args.runs} 趟完整夹具重演   并发负载 {args.load} 个")
        red_tally: collections.Counter = collections.Counter()
        branch_tally: collections.Counter = collections.Counter()
        red_runs = 0
        worst = 0.0
        for i in range(args.runs):
            red, branches, elapsed = one_run(args.load)
            worst = max(worst, elapsed)
            # 🔴 按**槽位**记，不只按分支记：一次 `ZERO` 落在 `badshape` 上是**看不见的**
            #    （它的 token 本来就过不了形状核），落在 `parent`/`child` 上才会长出红。
            #    第一版只按分支记 ⇒ 「34 次 ZERO 却 0 红」说不清是哪一格，等于没有观测点。
            branch_tally.update(f"{slot}={br}" for slot, br in branches.items())
            if red:
                red_runs += 1
                red_tally.update(red)
                labels = ",".join(f"{k}={v}" for k, v in branches.items() if v != "OK")
                names = " / ".join(f"{r}(:{LINES[r]})" for r in red)
                print(f"  🔴 趟 {i}: 红 {names}   非 OK 的读：{labels or '<无>'}")
        print()
        print(f"# 红 {red_runs} / 共 {args.runs} 趟")
        print(f"# 逐条断言红的次数（行号是 accounts_query.rs @118259b）：")
        for k in LINES:
            if red_tally[k]:
                print(f"    :{LINES[k]:<6} {k}  ×{red_tally[k]}")
        if not red_tally:
            print("    <一条都没红>")
        print()
        print(f"# 分支分布（分母 = environ 读的次数 = {sum(branch_tally.values())}）：")
        for k, n in branch_tally.most_common():
            print(f"    {k:<12} {n}")
        print()
        print(f"# spawn→pass② 最长墙钟 **{worst * 1000:.1f} ms**（活体是 `sleep 60`）")
        print(f"  ⇒ 离「父进程自己死掉、`:2024` 那个面」还差 {60 / max(worst, 1e-9):.0f}× 的余量。")
        print()
        print("⚠ 读法：本尺子给的是「机制可复现」，**不是**「那 2 红已证成因」——")
        print("  那两趟当时没有任何分支级观测点，事后不可复原。别把假说写成成因。")
    finally:
        for p in load:
            p.kill()
            p.wait()
    return 0


if __name__ == "__main__":
    sys.exit(main())
