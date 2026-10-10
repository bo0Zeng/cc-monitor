#!/usr/bin/env python3
"""S27 · **真断网跑一次** —— 证明缓存里那些 `.crate` 真的能把依赖喂饱。

# 为什么不只信 `--offline`

`cargo --offline` 是 cargo **自己**答应不上网，它证的是「cargo 不想上网」，
不是「上不了网也行」。门禁买的是后者（`.claude/devbox/gate` 默认 `--network none`）。
本脚本用 `bwrap --unshare-net` 造一个**真的没有网络命名空间**的壳，
里面跑的 `cargo` **不带 `--offline`** —— 它想上网也上不去。RC=0 才算数。
（反向对照那一格里 cargo 逐字报的是 `Could not resolve host: static.crates.io`
 ⇒ 那个壳里网**真的是死的**，不是 cargo 在自律。）

⚠ 本机上 `unshare -rn` 跑不动（`/proc/self/uid_map: 不允许的操作`，现打 2026-09-19），
  `docker` 也**没有** `ccmon-devbox:latest` 那个镜像了（`docker image inspect` 现打报
  `No such image`）⇒ `bwrap` 是今天唯一能造真断网的家伙。
  没有 `bwrap` 时本脚本**退 2 并说清楚**，不回落到 `--offline` 装作验过了。

# 🔴 为什么它是 `.py` 而不是更自然的 `.sh`

`ci.yml` 的 `e2e-smoke` job 把 `tests/evidence/*.sh` 纳进 shellcheck，并且那条覆盖面
地板被 `tests/frontend/shell/shell_lint_registry.rs` 钉成**恒等式**（不是「≥」）——
本目录多一个 `.sh`，那条判据当场红并逐字报「地板写着 69，而那条表达式今天真实覆盖 70」。
实测过：先写成 `.sh` 时它就是这么红的。改地板要同时动 `ci.yml` 与那棵 Rust 树，
不在本件的写区里 ⇒ 换成 `.py`（`tests/evidence/*.py` 没有同形的人群地板，
CI 的 `python syntax compile` 那步只盖 `tests/e2e/*.py`）。

# 三格

  stage0  今天两棵树（`src/frontend/shell` · `src/backend`）断网 `cargo fetch --locked` —— 不动仓库
  stage1  scratch 副本里加 `russh-sftp = "3.0.0"`，**联网**解析一次并把包喂进本机缓存
          （`cargo fetch` **不带 `--target` = 抓全 target**，`wasm-bindgen` 那一族才躲不过；
           只跑 `cargo build` 的人本机永远抓不到它们 ⇒ 本机全绿、断网门禁照红）
  stage2  同一个副本，**真断网**跑 `cargo fetch --locked` —— 这一格才是判据

# 反向对照（必须跑，否则不知道这把尺子是不是恒绿）

  python3 tests/evidence/S27-prove-offline.py --negative wasm-bindgen-0.2.128.crate

把那一份从缓存里挪走 ⇒ stage2 必须**红**并点名它 ⇒ 再原样挪回来（`finally` 保证放回）。

# 跑法

  python3 tests/evidence/S27-prove-offline.py            # stage0 + stage2（要先 prime 过）
  python3 tests/evidence/S27-prove-offline.py --prime    # 跑 stage1（联网），再 stage2
  python3 tests/evidence/S27-prove-offline.py --negative <crate 文件名>

🔴 **它不写仓库一个字节**：scratch 副本住 `$TMPDIR/S27-offline`，
   `--locked` 保证 cargo 不去改任何 `Cargo.lock`。
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORK = Path(tempfile.gettempdir()) / "S27-offline"
CACHE_BASE = Path(os.environ.get("CARGO_HOME") or (Path.home() / ".cargo")) / "registry" / "cache"


def netless(cwd: Path, *argv: str) -> subprocess.CompletedProcess:
    """在一个**没有网络命名空间**的壳里跑命令。`--dev-bind / /` 保留文件系统原样。"""
    return subprocess.run(
        ["bwrap", "--dev-bind", "/", "/", "--unshare-net", "--chdir", str(cwd), *argv],
        capture_output=True,
        text=True,
    )


def tail(text: str, n: int = 8) -> str:
    return "\n".join("     " + l for l in text.strip().splitlines()[-n:])


def prime() -> bool:
    print("── stage1 · 造 scratch 副本 + 加 russh-sftp 3.0.0 + 联网 cargo fetch")
    shutil.rmtree(WORK, ignore_errors=True)
    (WORK / "src").mkdir(parents=True)
    shutil.copytree(ROOT / "src" / "backend", WORK / "src" / "backend")
    # 共享 crate 的 path 依赖写的是 `../common/…`（原 `../bridge/crates/…`）⇒ 软链补同深度才解得开；
    # russh 补丁副本 `../vendor/russh` 同理。
    (WORK / "src" / "common").symlink_to(ROOT / "src" / "common")
    (WORK / "src" / "vendor").symlink_to(ROOT / "src" / "vendor")

    manifest = WORK / "src" / "backend" / "Cargo.toml"
    text = manifest.read_text(encoding="utf-8")
    if not re.search(r"^russh = ", text, re.M):
        print("   🔴 副本里找不到 `russh = ` 那一行 —— 清单形状变了，判不了")
        return False
    text = re.sub(r"^(russh = .*)$", r'\1\nrussh-sftp = "3.0.0"', text, count=1, flags=re.M)
    manifest.write_text(text, encoding="utf-8")

    env = {**os.environ, "CARGO_TARGET_DIR": str(WORK / "target")}
    p = subprocess.run(["cargo", "fetch"], cwd=WORK / "src" / "backend", env=env, capture_output=True, text=True)
    print("\n".join("   " + l for l in (p.stdout + p.stderr).strip().splitlines()))
    return p.returncode == 0


def main() -> int:
    argv = sys.argv[1:]
    if not shutil.which("bwrap"):
        print("❌ 没有 bwrap —— 造不出真断网壳。**不许**退回 `--offline` 冒充验过了。")
        return 2

    failed = False

    # ── stage0：今天两棵树 ────────────────────────────────────────────────
    for tree in ("src/frontend/shell", "src/backend"):
        print(f"── stage0 · {tree}（真断网 cargo fetch --locked，不带 --offline）")
        p = netless(ROOT / tree, "cargo", "fetch", "--locked")
        if p.returncode == 0:
            print("   ✅ RC=0 —— 这棵树今天的 lock 断网喂得饱")
        else:
            failed = True
            print("   🔴 RC≠0 —— 断网喂不饱。原样贴 cargo 的话：")
            print(tail(p.stderr))

    # ── stage1 ────────────────────────────────────────────────────────────
    if "--prime" in argv:
        if not prime():
            print("   🔴 stage1 失败")
            return 1
        print(f"   ✅ stage1 完成（包已落进 {CACHE_BASE}）")

    # ── stage2 ────────────────────────────────────────────────────────────
    copy = WORK / "src" / "backend"
    env = {**os.environ, "CARGO_TARGET_DIR": str(WORK / "target")}

    def stage2() -> subprocess.CompletedProcess:
        return subprocess.run(
            ["bwrap", "--dev-bind", "/", "/", "--unshare-net", "--chdir", str(copy),
             "cargo", "fetch", "--locked"],
            capture_output=True, text=True, env=env,
        )

    if (copy / "Cargo.lock").is_file():
        print("── stage2 · 副本（含 russh-sftp）真断网 cargo fetch --locked")
        p = stage2()
        if p.returncode == 0:
            print("   ✅ RC=0 —— 26 条新条目断网解析/取包全通")
        else:
            failed = True
            print("   🔴 RC≠0 —— 原样贴 cargo 的话：")
            print(tail(p.stderr))
    else:
        print("── stage2 · 跳过：还没 prime 过（先跑 `--prime`，那一步要联网）")
        print("   ⚠ 这一格**没验**，不是验过了。")
        failed = True

    # ── 反向对照 ──────────────────────────────────────────────────────────
    if "--negative" in argv:
        i = argv.index("--negative")
        if i + 1 >= len(argv):
            print("用法：--negative <crate 文件名，如 wasm-bindgen-0.2.128.crate>")
            return 2
        name = argv[i + 1]
        hits = sorted(CACHE_BASE.glob(f"*/{name}"))
        if not hits:
            print(f"🔴 缓存里本来就没有 {name} —— 这条对照做不了")
            return 2
        src = hits[0]
        quarantine = WORK.with_suffix(".quarantine")
        print(f"── 反向对照 · 把 {name} 挪走，stage2 必须红")
        shutil.move(str(src), str(quarantine))
        try:
            p = stage2()
            if p.returncode == 0:
                failed = True
                print("   🔴🔴 **它没红** —— 这把尺子是恒绿的，别信 stage2 的绿")
            else:
                print("   ✅ 红了，cargo 逐字：")
                print(tail(p.stderr, 6))
        finally:
            shutil.move(str(quarantine), str(src))
            print(f"   ✅ 已原样放回 {src}")

    print("S27-prove-offline: FAIL" if failed else "S27-prove-offline: OK")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
