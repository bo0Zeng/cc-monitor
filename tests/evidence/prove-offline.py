#!/usr/bin/env python3
"""**真断网跑一次** —— 证明缓存里那些 `.crate` 真的能把依赖喂饱。

# 为什么不只信 `--offline`

`cargo --offline` 是 cargo **自己**答应不上网，它证的是「cargo 不想上网」，
不是「上不了网也行」。门禁买的是后者（`.claude/devbox/gate` 默认 `--network none`）。
本脚本用 `bwrap --unshare-net` 造一个**真的没有网络命名空间**的壳，
里面跑的 `cargo` **不带 `--offline`** —— 它想上网也上不去。RC=0 才算数。
（反向对照那一格里 cargo 逐字报的是 `Could not resolve host: static.crates.io`
 ⇒ 那个壳里网**真的是死的**，不是 cargo 在自律。）

⚠ 本机上 `unshare -rn` 跑不动（`/proc/self/uid_map: 不允许的操作`），
  `docker` 也没有 `ccmon-devbox:latest` 那个镜像 ⇒ `bwrap` 是唯一能造真断网的家伙。
  没有 `bwrap` 时本脚本**退 2 并说清楚**，不回落到 `--offline` 装作验过了。

# 🔴 为什么它是 `.py` 而不是更自然的 `.sh`

`ci.yml` 的 `e2e-smoke` job 把 `tests/evidence/*.sh` 纳进 shellcheck，并且那条覆盖面
地板被 `tests/frontend/shell/shell_lint_registry.rs` 钉成**恒等式**（不是「≥」）——
本目录多一个 `.sh`，那条判据当场红。改地板要同时动 `ci.yml` 与那棵 Rust 树 ⇒ 换成 `.py`。

# 做什么

两棵树（`src/frontend/shell` · `src/backend`）各在断网壳里跑 `cargo fetch --locked`（不动仓库）。
缓存缺了包 ⇒ 喂之前在有网的地方对那棵树跑一次 `cargo fetch`（**不带 `--target` = 抓全 target**：
只为别的 target 编的依赖也进 lock、也要进缓存，只跑 `cargo build` 的人本机永远抓不到它们）。

# 反向对照（必须跑，否则不知道这把尺子是不是恒绿）

  python3 tests/evidence/prove-offline.py --negative wasm-bindgen-0.2.128.crate

把那一份从缓存里挪走 ⇒ 两棵树至少一棵必须**红** ⇒ 再原样挪回来（`finally` 保证放回）。

# 跑法

  python3 tests/evidence/prove-offline.py
  python3 tests/evidence/prove-offline.py --negative <crate 文件名>

🔴 **它不写仓库一个字节**：`--locked` 保证 cargo 不去改任何 `Cargo.lock`。
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TREES = ("src/frontend/shell", "src/backend")
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


def fetch_all(quiet: bool = False) -> list[tuple[str, subprocess.CompletedProcess]]:
    out = []
    for tree in TREES:
        if not quiet:
            print(f"── {tree}（真断网 cargo fetch --locked，不带 --offline）")
        out.append((tree, netless(ROOT / tree, "cargo", "fetch", "--locked")))
    return out


def main() -> int:
    argv = sys.argv[1:]
    if not shutil.which("bwrap"):
        print("❌ 没有 bwrap —— 造不出真断网壳。**不许**退回 `--offline` 冒充验过了。")
        return 2

    failed = False
    for tree, p in fetch_all():
        if p.returncode == 0:
            print(f"   ✅ {tree} RC=0 —— 这棵树的 lock 断网喂得饱")
        else:
            failed = True
            print(f"   🔴 {tree} RC≠0 —— 断网喂不饱。原样贴 cargo 的话：")
            print(tail(p.stderr))

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
        quarantine = Path(tempfile.gettempdir()) / f"prove-offline.{name}.quarantine"
        print(f"── 反向对照 · 把 {name} 挪走，至少一棵树必须红")
        shutil.move(str(src), str(quarantine))
        try:
            reds = [(t, p) for t, p in fetch_all(quiet=True) if p.returncode != 0]
            if not reds:
                failed = True
                print("   🔴🔴 **它没红** —— 这把尺子是恒绿的，别信上面的绿")
            else:
                print(f"   ✅ {reds[0][0]} 红了，cargo 逐字：")
                print(tail(reds[0][1].stderr, 6))
        finally:
            shutil.move(str(quarantine), str(src))
            print(f"   ✅ 已原样放回 {src}")

    print("prove-offline: FAIL" if failed else "prove-offline: OK")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
