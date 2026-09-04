#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R21 落地拍的**死值验尺子** —— 把新拆出来的那一态掏掉，看哪几条判据会红。

# 它量什么（两条，PM 裁定 ㈥ 要求缺一不可）

  1. **接没接上**：把 `EnvRead::Unreadable` 那一支的信息压回去（当成「没设」），
     新落的那条活体判据必须**回到会红的状态**。掏了不红 = 拆分根本没接到判据上。
  2. **那句假话没了**：不掏的时候，那条活体判据必须**绿**，且它内部的对照那一格
     （真裸起仍归账号 0）也必须绿 —— 否则「把归属整个摘掉」也会是同一个读数。

  ⚠ 顺带把**那条 flaky 判据**（`an_inherited_launch_id_is_never_reported_as_the_childs_own_identity`）
  一起量：掏与不掏各跑同样多趟，看它的红率有没有动。
  🔴 **预期是没动**：本拍治的是 `CLAUDE_CONFIG_DIR` 那条读回路（`:633`），
  而那条 flaky 红在 `CCM_LAUNCH_ID` 那条（`:642`），本拍刻意没动它。
  这一格是**顶回 PM 的读数**，不是「买到了」。

# 尺子怎么切的（报数之前先说清）

- **分母**：每种模式下**同一条测试跑多少趟**（`--reps`，默认 200）。
  一趟 = 一次完整的夹具重演（起活体 → 写 pidfile → 跑 `session_accounts` → 断言）。
- **量于哪个提交**：脚本自己打印 `git rev-parse HEAD`，读数照抄那个 SHA。
- **跑法**：先 `cargo test --no-run` 编一次，拿到测试可执行文件，之后**直接反复跑那个
  可执行文件**（不经 cargo）—— 免掉每趟几百毫秒的 cargo 开销，也免得把编译算进分母。
- **变异怎么施加**：对 `remote-daemon-proto/src/observe/accounts_query.rs` 做一次
  **逐字**字符串替换（下面 `MUTATIONS` 里写死了 before/after），跑完在 `finally` 里
  按原字节还原，并在最后自检「还原后的字节 == 原字节」。
  🔴 用完请自己再跑一次 `git status --short` 确认树是干净的 —— 尺子自检不能替代它。

# 🔴 只许在沙箱里跑（`K31`）

判据是 `/.dockerenv`。它会起真进程（活体夹具就是干这个的），宿主上不跑。

# 复算

    python3 evidence/K-R21-deadvalue.py --mode=baseline --reps=200
    python3 evidence/K-R21-deadvalue.py --mode=M1 --reps=200

（在沙箱里、工作树根下跑。`--mode=all` 两种都跑。）
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

TARGET_REL = "remote-daemon-proto/src/observe/accounts_query.rs"

# 新落的那条活体判据（本拍买到的那一格）与那条 flaky 判据（本拍**没**买到的那一格）。
T_NEW = "observe::accounts_query::tests::an_unreadable_environ_is_never_reported_as_the_zero_account"
T_FLAKY = (
    "observe::accounts_query::tests::"
    "an_inherited_launch_id_is_never_reported_as_the_childs_own_identity"
)

# ── 变异表：`(名字, 说明, before, after)` ────────────────────────────────────
# 🔴 before 一律写**逐字**原文；对不上就报错退出，绝不「找个近似的凑合」——
#    那正是这一族最容易长出来的假绿（掏了个别的东西，然后说「掏了不红」）。
MUTATIONS = {
    "M1": (
        "把「环境这一刻取不到」压回「没设」",
        # 拆出来的那一态**照样算出来了**，只是它的信息被丢掉 —— 这正是「压回 None」的
        # 最小形状：不是把 `EnvRead` 删掉（那样编译就红了，证明不了判据接没接上）。
        "                EnvRead::Unreadable => (None, true),",
        "                EnvRead::Unreadable => (None, false),",
    ),
}


def die(msg: str) -> None:
    print(f"❌ {msg}", file=sys.stderr)
    raise SystemExit(2)


def sh(args: list[str], cwd: Path, timeout: int = 1800) -> subprocess.CompletedProcess:
    return subprocess.run(
        args, cwd=str(cwd), capture_output=True, text=True, timeout=timeout
    )


def test_executable(root: Path) -> Path:
    """编一次测试二进制，返回它的路径（不跑）。"""
    p = sh(
        [
            "cargo",
            "test",
            "--offline",
            "--no-run",
            "--message-format=json",
            "--manifest-path",
            "remote-daemon-proto/Cargo.toml",
        ],
        root,
    )
    if p.returncode != 0:
        print(p.stdout[-4000:])
        print(p.stderr[-4000:], file=sys.stderr)
        die("`cargo test --no-run` 非零退出 —— 编不过就不许往下量")
    cands = []
    for line in p.stdout.splitlines():
        try:
            m = json.loads(line)
        except Exception:
            continue
        if m.get("reason") != "compiler-artifact" or not m.get("executable"):
            continue
        if m.get("profile", {}).get("test") is not True:
            continue
        if "remote-daemon-proto" not in str(m.get("package_id", "")):
            continue
        cands.append(m["executable"])
    if len(cands) != 1:
        die(
            f"抽到 {len(cands)} 个测试可执行文件（要恰好 1 个）：{cands}"
            " —— 抽取器坏了，别当零红"
        )
    return Path(cands[0])


def run_one(exe: Path, test: str) -> bool:
    """跑一趟那条测试。True = 绿。

    🔴 **反空真自检就在这里**：只看退出码是不够的 —— 过滤器打偏时
    `0 passed; 0 failed; N filtered out` 的退出码**也是 0**，于是「没跑」
    与「跑了没红」在读数上一模一样。本函数要求那一行逐字是「恰好一条」，
    对不上就当场炸，不把一个 0 当成绿。
    """
    p = subprocess.run(
        [str(exe), "--exact", test, "--test-threads=1"],
        capture_output=True,
        text=True,
        timeout=300,
    )
    out = p.stdout + p.stderr
    green = "test result: ok. 1 passed; 0 failed" in out
    red = "test result: FAILED. 0 passed; 1 failed" in out
    if not (green or red):
        print(out[-3000:], file=sys.stderr)
        die(f"这一趟既不是「1 passed」也不是「1 failed」（rc={p.returncode}）—— 过滤器可能打偏了，不许当读数")
    return green


def spawn_load(n: int) -> list:
    """起 n 路 CPU 负载 —— 那个 exec 窗口的竞态在有负载时才容易撞上。"""
    return [
        subprocess.Popen(
            ["sh", "-c", "while :; do :; done"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        for _ in range(n)
    ]


def measure(exe: Path, reps: int, only: str, load: int) -> dict:
    which = {
        "new": [("新判据", T_NEW)],
        "flaky": [("flaky 判据", T_FLAKY)],
        "both": [("新判据", T_NEW), ("flaky 判据", T_FLAKY)],
    }[only]
    procs = spawn_load(load)
    try:
        out = {}
        for name, test in which:
            red = 0
            t0 = time.time()
            for _ in range(reps):
                if not run_one(exe, test):
                    red += 1
            secs = round(time.time() - t0, 1)
            out[name] = {"红": red, "趟": reps, "并发负载": load, "秒": secs}
            print(
                f"    {name}: 红 {red} / {reps} 趟 · 负载 {load} 路（{secs}s，"
                f"每趟 {round(secs * 1000 / reps, 1)}ms）",
                flush=True,
            )
        return out
    finally:
        for q in procs:
            q.kill()
            q.wait()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--mode", default="all", choices=["baseline", "M1", "all"])
    ap.add_argument("--reps", type=int, default=200)
    ap.add_argument("--only", default="both", choices=["new", "flaky", "both"])
    ap.add_argument(
        "--load", type=int, default=0, help="并发 CPU 负载路数（exec 窗口竞态要它才容易撞上）"
    )
    ap.add_argument("--repo", default=".")
    a = ap.parse_args()

    if not Path("/.dockerenv").exists():
        die("不在容器里 —— 本尺子起真进程，`K31` 明令宿主上不跑（判据 /.dockerenv）")

    root = Path(a.repo).resolve()
    if not (root / TARGET_REL).is_file():
        die(f"{TARGET_REL} 不在 {root} 下 —— 跑错目录了")
    head = sh(["git", "rev-parse", "HEAD"], root).stdout.strip()
    dirty = sh(["git", "status", "--short"], root).stdout.strip()
    print(f"量于提交：{head}")
    print(f"工作树脏否：{'脏 —— ' + dirty if dirty else '干净'}")
    print(f"分母：每条判据每种模式 {a.reps} 趟\n")

    src = root / TARGET_REL
    original = src.read_bytes()
    results = {}
    try:
        if a.mode in ("baseline", "all"):
            print("── baseline（不掏）──────────────────────────────")
            exe = test_executable(root)
            results["baseline"] = measure(exe, a.reps, a.only, a.load)

        if a.mode in ("M1", "all"):
            name, why, before, after = "M1", *MUTATIONS["M1"]
            text = original.decode("utf-8")
            n = text.count(before)
            if n != 1:
                die(f"{name} 的 before 在 {TARGET_REL} 里命中 {n} 次（要恰好 1 次）—— 语料挪了，回来重判")
            print(f"── {name}（{why}）──────────────────────────────")
            src.write_bytes(text.replace(before, after, 1).encode("utf-8"))
            exe = test_executable(root)
            results[name] = measure(exe, a.reps, a.only, a.load)
    finally:
        src.write_bytes(original)
        if src.read_bytes() != original:
            die("🔴 还原失败 —— 树被我改脏了，立刻 `git checkout` 那个文件")
        print("\n· 已按原字节还原（自检通过）。请自己再跑一次 `git status --short` 复核。")

    print("\n== 读数 ==")
    print(json.dumps(results, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
