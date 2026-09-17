#!/usr/bin/env python3
"""K-W2D `KW2D2` · 量 **S** —— sidecar 那个二进制每架构多大，且与「编进去」那个数**可比**。

⚠ 为什么是 `.py` 而不是 `.sh`：`shell_lint_registry` 要求仓里每个 `.sh` 要么进 CI 的
   shellcheck 表达式、要么登记豁免；本目录的量具全是 `.py`。（`K-R2` 09-04 现打踩过一次。）

尺子（写在最前面，因为「可比」这件事全压在它上面）
--------------------------------------------------
· 工具链 = `K30` 那把，也是 `.github/workflows/release.yml` 在用的那把：
  zig **0.14.0** + cargo-zigbuild **0.23.0**，`cargo zigbuild --release --target
  {x86_64,aarch64}-unknown-linux-musl`。**与 `evidence/K-R2-size-and-arch.py` 逐字同一把。**
· 分母 = **同一趟、同一工具链、同一提交**编出来的四组（见下），**不是**盘上那几个历史数。
· 全程在沙箱容器里跑（`K31`），仓副本落 scratchpad，**工作树一个字没改**。
· 量具住址：本文件（`evidence/K-W2D-sidecar-size.py`，K-W2D 独占的名字）；
  被测对象 = 调用时传进来的那棵工作树的**副本**（`<scratch>/repo-*`），交回时连提交一起报。

四组，缺一组就报不出「可比」
--------------------------
| 组 | 是什么 | 它回答哪一格 |
|---|---|---|
| `base`        | 工作树原样的 daemon | 同一趟的基线（`K-R2` 那个基线是**另一个提交**上量的，不许混用） |
| `engine`      | daemon + 引擎依赖 + **真调** `Engine::open`+`index()` | 甲（编进 daemon）的每架构增量 |
| `side-full`   | 一个薄 sidecar crate，链 vendored 引擎并**真调** | **S** |
| `side-deponly`| 同一个 crate，依赖在、**一行都不调** | 证「只加依赖不调用会量出一个假的便宜」 |
| `side-nofeat` | 同一个 crate，`--no-default-features`（引擎依赖整条不进） | 薄 CLI 自己的地板 ⇒ S 的分解 |

🔴 `side-deponly` 那一组不是凑数：`K-R2` 逐字踩过 —— 不真调 `Engine::open`+`index()`，
   rustc 根本不链那个 rlib。本组把那句话变成本趟自己的读数（**非空对照**）。

产品面怎么换算（别把每架构的数直接拿去比预算）
----------------------------------------------
`src-tauri/build.rs` 那个 `for arch in ["x86_64", "aarch64"]` 把两个架构都内嵌进安装包
⇒ **安装包增量 = 每架构增量 × 2**。
· 甲 = 2 ×（`engine` − `base`）
· 乙 = 2 × S（sidecar 是一个**新文件**，整份都是新增字节 —— 不是「增量」，是全量）

用法
----
  第一趟（备副本，不编）：
    python3 evidence/K-W2D-sidecar-size.py <scratch 目录> <工作树绝对路径>
  然后**按头注「变量组怎么造」用编辑工具改副本**（本脚本刻意不替你改源码），再：
    python3 evidence/K-W2D-sidecar-size.py <scratch 目录> <工作树绝对路径> --go

变量组怎么造（三处，逐字；不齐本脚本 fail-closed 不编）
------------------------------------------------------
① `<scratch>/repo-engine/remote-daemon-proto/Cargo.toml` 末尾加一行
   `code-picture-core = { path = "../src-tauri/vendor/code-picture-core" }`；
   `.../src/main.rs` 里加一个真调 `Engine::open` + `index()` 的子命令分支。
② `<scratch>/repo-side/sidecar-probe/{Cargo.toml,src/main.rs}`：薄 sidecar crate，
   引擎依赖 **optional**，`engine` / `call` 两个 feature 分别管「链进来」与「真调」。
③ 两处都不许碰 vendor 本体（`C7`）。

fail-closed 自检（本脚本会先核，核不过就不编）
----------------------------------------------
· `repo-engine` 里必须同时出现「依赖行」与「真调点」——只有依赖行 ⇒ 拒编（那会量出假的便宜）；
· `repo-side/sidecar-probe/Cargo.toml` 必须存在且声明 optional 依赖；
· 每组编完必须真有产物，没有就把日志尾巴印出来 —— **不许静默算成 0 字节**。
"""

import shutil
import subprocess
import sys
from pathlib import Path

IMG = "ccmon-devbox:latest"
CARGO_VOLUME = "ccmon-cargo-registry"
TARGETS = ["x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"]

# 备工具链：优先用 `--ro <目录>` 里已经下好的那份（K-R2 那趟留下的），
# 没有就现下（要网）。**幂等**。
# ⚠ 这里是 `cp -a` 而不是 `cp -al`：`--ro` 那份与 `<scratch>` 常常**不在同一个文件系统**上
#   （一个 tmpfs、一个磁盘）⇒ 硬链接当场失败；而就算同盘，硬链接会让两份共用 inode，
#   别的 agent 那份被就地写坏就成了一次静默的假读数（`brief` 第 12 条 `5k`）。
SETUP = r"""
set -euo pipefail
M=/m
if [ ! -x "$M/tools/zig/zig" ]; then
  if [ -x /ro/tools/zig/zig ]; then mkdir -p "$M/tools"; cp -a /ro/tools/zig "$M/tools/zig"
  else curl -fsSL -o /tmp/z.tar.xz \
      https://ziglang.org/download/0.14.0/zig-linux-x86_64-0.14.0.tar.xz
    mkdir -p "$M/tools/zig"; tar -xJf /tmp/z.tar.xz -C "$M/tools/zig" --strip-components=1; fi
fi
if [ ! -x "$M/tools/bin/cargo-zigbuild" ]; then
  if [ -x /ro/tools/bin/cargo-zigbuild ]; then mkdir -p "$M/tools/bin"
    cp -a /ro/tools/bin/cargo-zigbuild "$M/tools/bin/cargo-zigbuild"
  else mkdir -p "$M/tools/bin"
    curl -fsSL -o /tmp/c.txz https://github.com/rust-cross/cargo-zigbuild/releases/download/v0.23.0/cargo-zigbuild-x86_64-unknown-linux-musl.tar.xz
    tar -xJf /tmp/c.txz -C "$M/tools/bin" --strip-components=1 --wildcards "*/cargo-zigbuild"; fi
fi
if [ ! -d "$M/rustup/toolchains" ]; then
  if [ -d /ro/rustup/toolchains ]; then cp -a /ro/rustup "$M/rustup"
  else cp -a /opt/rust/rustup "$M/rustup"
    RUSTUP_HOME=$M/rustup rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl; fi
fi
RUSTUP_HOME=$M/rustup rustup target list --installed | grep musl
"$M/tools/zig/zig" version
"$M/tools/bin/cargo-zigbuild" --version
"""

# 一组 = 一个仓副本 + 一个 manifest 路径 + 一串 cargo 旗标。
BUILD = r"""
set -uo pipefail
M=/m
export CARGO_HOME=$M/cargo-home RUSTUP_HOME=$M/rustup
export PATH="$M/tools/bin:$M/tools/zig:$PATH"
export ZIG_GLOBAL_CACHE_DIR=$M/zig-cache XDG_CACHE_HOME=$M/cache
export CARGO_TARGET_DIR=$M/target-%(tag)s
cd $M/%(dir)s
for T in %(targets)s; do
  echo "===== %(tag)s / $T ====="
  S=$(date +%%s)
  # `--offline` 是刻意的：本量具跑在断网容器里（crates 走那个具名卷的缓存），
  # 而 sidecar 那个 crate 没有 lock ⇒ 不给 `--offline` 时 cargo 会去更新索引、当场失败。
  cargo zigbuild --release --offline --target "$T" %(flags)s > $M/out/%(tag)s-$T.log 2>&1
  echo "EXIT=$?  秒=$(( $(date +%%s) - S ))"
  B=$CARGO_TARGET_DIR/$T/release/%(bin)s
  if [ -f "$B" ]; then
    cp "$B" /tmp/b && strip /tmp/b 2>/dev/null || true
    echo "未strip=$(stat -c %%s "$B")  strip后=$(stat -c %%s /tmp/b)"
    # 量具自检：「strip 无增益」是真的吗 —— 看段表，别让两个相等的数自己说话。
    readelf -S "$B" | grep -E 'symtab|debug' || echo "(无 symtab/debug 段 ⇒ strip 本就无增益)"
  else
    echo "❌ 没有产物 —— 日志尾巴："
    tail -25 $M/out/%(tag)s-$T.log
  fi
done
echo "BUILD-DONE-%(tag)s"
"""

# (tag, 副本目录, manifest 所在子目录, cargo 旗标, 产物名)
GROUPS = [
    ("base", "repo-base/remote-daemon-proto", "", "cc-monitor-remote"),
    ("engine", "repo-engine/remote-daemon-proto", "", "cc-monitor-remote"),
    ("side-full", "repo-side/sidecar-probe", "--features call", "cp-sidecar-probe"),
    # 🔴 `--no-default-features` 不能省：默认 feature 就是 `call`，
    # 只写 `--features engine` **不会**把它关掉 ⇒ 这一组会与 `side-full` 编出
    # **逐字节相同**的二进制，而两个相等的数看起来正像一个「读数」。
    # （09-04 第一趟就是这样：aarch64 两组都是 18,895,456 B。**那不是发现，是量错了。**）
    ("side-deponly", "repo-side/sidecar-probe",
     "--no-default-features --features engine", "cp-sidecar-probe"),
    ("side-nofeat", "repo-side/sidecar-probe", "--no-default-features", "cp-sidecar-probe"),
]


def docker(scratch: Path, script: str, net: str, ro: Path | None) -> int:
    cmd = ["docker", "run", "--rm", "--network", net, "--user", "root",
           "-v", f"{scratch}:/m",
           "-v", f"{CARGO_VOLUME}:/m/cargo-home/registry",
           "-e", "HOME=/root"]
    if ro is not None and ro.exists():
        cmd += ["-v", f"{ro}:/ro:ro"]
    cmd += [IMG, "bash", "-o", "pipefail", "-c", script]
    return subprocess.call(cmd)


def prepared(scratch: Path) -> list[str]:
    """哪几组的变量真的造好了 —— **fail-closed**：造不齐的组不编，别量出假读数。"""
    bad: list[str] = []
    eng = scratch / "repo-engine" / "remote-daemon-proto"
    dep = "code-picture-core"
    if not (eng / "Cargo.toml").exists() or dep not in (eng / "Cargo.toml").read_text():
        bad.append("engine：Cargo.toml 没有引擎依赖行")
    main = eng / "src" / "main.rs"
    if not main.exists() or "Engine::open" not in main.read_text():
        bad.append("engine：main.rs 里没有真调点（只加依赖不调用 ⇒ 假的便宜）")
    side = scratch / "repo-side" / "sidecar-probe"
    if not (side / "Cargo.toml").exists():
        bad.append("side：sidecar-probe/Cargo.toml 不存在")
    elif "optional = true" not in (side / "Cargo.toml").read_text():
        bad.append("side：引擎依赖不是 optional ⇒ 三组 feature 分不开")
    elif "Engine::open" not in (side / "src" / "main.rs").read_text():
        bad.append("side：src/main.rs 里没有真调点")
    return bad


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    scratch, wt = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
    go = "--go" in sys.argv
    net = "host" if "--net" in sys.argv else "none"
    ro = Path(sys.argv[sys.argv.index("--ro") + 1]).resolve() if "--ro" in sys.argv else None
    for sub in ("out", "cargo-home", "tools"):
        (scratch / sub).mkdir(parents=True, exist_ok=True)

    for dst in ("repo-base", "repo-engine", "repo-side"):
        if not (scratch / dst).exists():
            subprocess.check_call(["rsync", "-a", "--exclude", "node_modules",
                                   "--exclude", "target", "--exclude", ".git",
                                   f"{wt}/", str(scratch / dst) + "/"])
            print(f"· 备副本 {dst}")
    if shutil.which("docker") is None:
        print("❌ 没有 docker —— 本量具只在沙箱里跑（K31）")
        return 3

    bad = prepared(scratch)
    if not go:
        print(f"\n⚠ 变量组要你先改（见头注「变量组怎么造」），改在：{scratch}")
        for b in bad:
            print(f"  · 还差 —— {b}")
        print("  齐了之后加 --go 再跑。")
        return 0
    if bad:
        print("❌ 变量组没造齐，**拒编**（编下去会量出一个假的便宜）：")
        for b in bad:
            print(f"  · {b}")
        return 3

    print("· 备工具链（幂等）")
    if docker(scratch, SETUP, net if ro is None else net, ro) != 0:
        print("❌ 工具链没备齐，先修再跑（别当没事）")
        return 3

    for tag, d, flags, binname in GROUPS:
        print(f"\n· 组 {tag}")
        docker(scratch, BUILD % {"tag": tag, "dir": d, "flags": flags,
                                 "bin": binname, "targets": " ".join(TARGETS)}, net, ro)
    print("\n⇒ 甲（编进 daemon）= 2 ×（engine − base）· 乙（sidecar 随包发）= 2 × S。"
          "\n  两个数只在**同一趟**里可比；跨趟别混用。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
