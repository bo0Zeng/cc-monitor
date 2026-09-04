#!/usr/bin/env python3
"""K-R2 摸底 · 三个数里的头两个：**编不编得动** 与 **体积增量**。

⚠ 为什么是 `.py` 而不是 `.sh`：本目录 15 个量具全是 `.py`，那不是巧合 ——
   `shell_lint_registry::every_shell_script_is_either_linted_or_registered_as_exempt`
   要求仓里每个 `.sh` 要么进 CI 的 shellcheck 表达式、要么登记豁免。
   09-04 本拍第一版写成 `.sh`，门禁当场红（逐字点名这两个文件）。**这条是现打的。**

口径（写在最前面，因为它与盘上既有那几个数**不是同一把尺子**）
------------------------------------------------------------
· 尺子 = `K30` 逐字那把，也是 `.github/workflows/release.yml:30-45` 在用的那把：
  zig **0.14.0** + cargo-zigbuild **0.23.0**，
  `cargo zigbuild --release --target {x86_64,aarch64}-unknown-linux-musl`。
· 分母 = **同一趟、同一工具链、同一提交**编出来的基线 daemon（**不是**盘上那几个历史数）。
· 变量 = 两处，缺一不可：
    ① `remote-daemon-proto/Cargo.toml` 加 `code-picture-core = { path = "../src-tauri/vendor/code-picture-core" }`
    ② `main.rs` 里**真调一次** `Engine::open` + `index()`
  🔴 只加依赖不调用，rustc 根本不把那个 rlib 链进来 —— 量出来会是 0，那是个**假的便宜**。
· 全程在沙箱容器里跑（`K31`），仓副本落 scratchpad，**工作树一个字没改**。

09-04 实测读数（量于 `e1944e8`，本树未铺 `src-tauri/embedded-daemons/`）
------------------------------------------------------------------------
  | target                     | 基线      | 加引擎     | 增量        | 倍数  | 编译秒 |
  |----------------------------|-----------|------------|-------------|-------|--------|
  | x86_64-unknown-linux-musl  | 4,548,064 | 23,430,072 | +18,882,008 | ×5.15 | 56→63  |
  | aarch64-unknown-linux-musl | 4,070,240 | 22,941,288 | +18,871,048 | ×5.64 | 30→54  |

· **两个架构都 EXIT=0** —— 9 门 tree-sitter 的 C + `rusqlite(bundled)` 的 sqlite3.c
  在 aarch64-musl 下由 zig 当交叉 linker **编得动**。⇒ `K30` 第②项不是本件的拦路虎。
· 🔴 增量要 **×2** 才是产品面的代价：`src-tauri/build.rs:256` 逐字 `for arch in ["x86_64","aarch64"]`
  —— 安装包把两个架构的 daemon 都内嵌 ⇒ **+37,753,056 B ≈ +37.75 MB**。
· strip 前后**同值**是真的，不是量具坏了：`readelf -S` 实测这两个二进制**没有 symtab/debug 段**
  （zig 的 linker 出来就这样）。本脚本每趟都把这一格印出来，别让「两个数相等」自己说话。
· ⚠ 本趟基线（4.55 / 4.07 MB）**低于**盘上那几个（CI 5.56 / 5.75 · `K-P4` 本机 5.94 未 strip / 4.77 strip 后）
  —— 别混用：那几个是别的工具链、别的机器、别的时刻量的。
  **本文件唯一敢担保的是同一趟里两个数之差。**

用法
----
  python3 evidence/K-R2-size-and-arch.py <scratch 目录> <工作树绝对路径>
⚠ 变量组那两处改动本脚本**不替你改**（改法见上面「口径」②）；
  它建好 `<scratch>/repo-engine/` 之后会停下来提示，你改完再加 `--go` 跑第二组。
"""

import shutil
import subprocess
import sys
from pathlib import Path

IMG = "ccmon-devbox:latest"
TARGETS = ["x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"]

SETUP = r"""
set -euo pipefail
M=/m
[ -x "$M/tools/zig/zig" ] || { curl -fsSL -o /tmp/z.tar.xz \
    https://ziglang.org/download/0.14.0/zig-linux-x86_64-0.14.0.tar.xz
  mkdir -p "$M/tools/zig"; tar -xJf /tmp/z.tar.xz -C "$M/tools/zig" --strip-components=1; }
[ -x "$M/tools/bin/cargo-zigbuild" ] || { mkdir -p "$M/tools/bin"
  curl -fsSL -o /tmp/c.txz https://github.com/rust-cross/cargo-zigbuild/releases/download/v0.23.0/cargo-zigbuild-x86_64-unknown-linux-musl.tar.xz
  tar -xJf /tmp/c.txz -C "$M/tools/bin" --strip-components=1 --wildcards "*/cargo-zigbuild"; }
[ -d "$M/rustup/toolchains" ] || cp -a /opt/rust/rustup "$M/rustup"
RUSTUP_HOME=$M/rustup rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl
"$M/tools/zig/zig" version
"$M/tools/bin/cargo-zigbuild" --version
"""

BUILD = r"""
set -uo pipefail
M=/m
export CARGO_HOME=$M/cargo-home RUSTUP_HOME=$M/rustup
export PATH="$M/tools/bin:$M/tools/zig:$PATH"
export ZIG_GLOBAL_CACHE_DIR=$M/zig-cache XDG_CACHE_HOME=$M/cache
export CARGO_TARGET_DIR=$M/target-%(tag)s
cd $M/%(dir)s/remote-daemon-proto
for T in %(targets)s; do
  echo "===== %(tag)s / $T ====="
  S=$(date +%%s)
  cargo zigbuild --release --target "$T" > $M/out/%(tag)s-$T.log 2>&1
  echo "EXIT=$?  秒=$(( $(date +%%s) - S ))"
  B=$CARGO_TARGET_DIR/$T/release/cc-monitor-remote
  if [ -f "$B" ]; then
    echo "字节=$(stat -c %%s "$B")"
    # 量具自检：strip 无增益是真的吗 —— 看段表，别只看「两个数相等」。
    readelf -S "$B" | grep -E 'symtab|debug' || echo "(无 symtab/debug 段 ⇒ strip 本就无增益)"
  else
    tail -25 $M/out/%(tag)s-$T.log
  fi
done
"""


def docker(scratch: Path, script: str, net: str = "host") -> int:
    cmd = ["docker", "run", "--rm", "--network", net, "--user", "root",
           "-v", f"{scratch}:/m", "-e", "HOME=/root", IMG,
           "bash", "-o", "pipefail", "-c", script]
    return subprocess.call(cmd)


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    scratch, wt = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
    go = "--go" in sys.argv
    for sub in ("out", "cargo-home", "tools"):
        (scratch / sub).mkdir(parents=True, exist_ok=True)

    if not (scratch / "repo").exists():
        for dst in ("repo", "repo-engine"):
            subprocess.check_call(["rsync", "-a", "--exclude", "node_modules",
                                   "--exclude", "target", "--exclude", ".git",
                                   f"{wt}/", str(scratch / dst) + "/"])
    if shutil.which("docker") is None:
        print("❌ 没有 docker —— 本量具只在沙箱里跑（K31）")
        return 3

    print("· 备工具链（幂等）")
    if docker(scratch, SETUP) != 0:
        print("❌ 工具链没备齐，先修再跑（别当没事）")
        return 3

    print("\n· 基线组")
    docker(scratch, BUILD % {"tag": "base", "dir": "repo", "targets": " ".join(TARGETS)}, net="host")

    if not go:
        print(f"\n⚠ 变量组要你先改两处（见本文件头注「口径」②），改在：{scratch/'repo-engine'}")
        print("  改完加 --go 再跑一次，本脚本会跳过已备好的工具链、只编变量组。")
        return 0

    print("\n· 变量组（加引擎）")
    docker(scratch, BUILD % {"tag": "engine", "dir": "repo-engine", "targets": " ".join(TARGETS)}, net="host")
    print("\n⇒ 增量 = 变量组字节 − 基线字节（同一趟、同一工具链、同一提交）。"
          "\n  产品面的代价 = 增量 ×2（两个架构都被内嵌进安装包，build.rs:256）。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
