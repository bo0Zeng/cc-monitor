#!/usr/bin/env bash
# K-R2 摸底 · 三个数里的头两个：**编不编得动** 与 **体积增量**。
#
# 量的是什么（口径写在最前面，因为它和盘上既有的几个数**不是同一把尺子**）：
#   · 尺子 = `K30` 逐字那把：`cargo zigbuild --release --target {x86_64,aarch64}-unknown-linux-musl`，
#     zig **0.14.0** + cargo-zigbuild **0.23.0**（与 `.github/workflows/release.yml:30-45` 同版本）。
#   · 分母 = **同一趟、同一工具链、同一提交**编出来的基线 daemon（不是盘上那几个历史数）。
#   · 变量 = `remote-daemon-proto/Cargo.toml` 加一条 `code-picture-core` path 依赖，
#     且 `main.rs` **真调一次** `Engine::open` + `index()`。
#     🔴 只加依赖不调用，rustc 根本不把那个 rlib 链进来 —— 量出来会是 0，那是个**假的便宜**。
#   · 全程在沙箱容器里跑（`K31`），仓副本落 scratchpad，**工作树一个字没改**。
#
# 09-04 实测读数（量于 e1944e8，本树未铺 src-tauri/embedded-daemons/）：
#
#   | target                     | 基线      | 加引擎     | 增量        | 倍数  | 编译秒 |
#   |----------------------------|-----------|------------|-------------|-------|--------|
#   | x86_64-unknown-linux-musl  | 4,548,064 | 23,430,072 | +18,882,008 | ×5.15 | 56→63  |
#   | aarch64-unknown-linux-musl | 4,070,240 | 22,941,288 | +18,871,048 | ×5.64 | 30→54  |
#
#   · **两个架构都 EXIT=0** —— 9 门 tree-sitter 的 C + `rusqlite(bundled)` 的 sqlite3.c
#     在 aarch64-musl 下由 zig 当交叉 linker **编得动**。`K30` 的第②项不是拦路虎。
#   · strip 前后**同值**：`readelf -S` 实测这两个二进制**没有 symtab/debug 段**
#     （zig 的 linker 出来就是这样）⇒ 不是 strip 工具坏了。
#   · ⚠ 这里的基线（4.55 / 4.07 MB）**低于**盘上那几个数（CI 5.56/5.75 · `K-P4` 本机 5.94 未 strip
#     / 4.77 strip 后）。**别混用**：那几个是别的工具链、别的机器、别的时刻量的。
#     本文件唯一敢担保的是**同一趟里的两个数之差**。
#
# 用法：bash evidence/K-R2-size-and-arch.sh <scratch 目录> <工作树绝对路径>
set -uo pipefail
SCRATCH="${1:?用法: K-R2-size-and-arch.sh <scratch 目录> <工作树绝对路径>}"
WT="${2:?}"
IMG=ccmon-devbox:latest

mkdir -p "$SCRATCH/out" "$SCRATCH/cargo-home" "$SCRATCH/tools"
rsync -a --exclude node_modules --exclude target --exclude .git "$WT/" "$SCRATCH/repo/"
rsync -a --exclude node_modules --exclude target --exclude .git "$WT/" "$SCRATCH/repo-engine/"

# ── 变量组：加依赖 + 真调一次 ──────────────────────────────────────────
cat >> "$SCRATCH/repo-engine/remote-daemon-proto/Cargo.toml.add" <<'EOF'
code-picture-core = { path = "../src-tauri/vendor/code-picture-core" }
EOF
echo "⚠ 变量组的两处改动（Cargo.toml 加依赖 · main.rs 加一个真调引擎的 --panorama-probe 分支）"
echo "  本脚本不替你改 —— 见本文件头注的口径，照着改在 $SCRATCH/repo-engine/ 里再跑。"

# ── 工具链（幂等） ────────────────────────────────────────────────────
docker run --rm --network host --user root -v "$SCRATCH:/m" -e HOME=/root "$IMG" bash -o pipefail -c '
set -euo pipefail
M=/m
[ -x "$M/tools/zig/zig" ] || { curl -fsSL -o /tmp/z.tar.xz https://ziglang.org/download/0.14.0/zig-linux-x86_64-0.14.0.tar.xz
  mkdir -p "$M/tools/zig"; tar -xJf /tmp/z.tar.xz -C "$M/tools/zig" --strip-components=1; }
[ -x "$M/tools/bin/cargo-zigbuild" ] || { mkdir -p "$M/tools/bin"
  curl -fsSL -o /tmp/c.txz https://github.com/rust-cross/cargo-zigbuild/releases/download/v0.23.0/cargo-zigbuild-x86_64-unknown-linux-musl.tar.xz
  tar -xJf /tmp/c.txz -C "$M/tools/bin" --strip-components=1 --wildcards "*/cargo-zigbuild"; }
[ -d "$M/rustup/toolchains" ] || cp -a /opt/rust/rustup "$M/rustup"
RUSTUP_HOME=$M/rustup rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl
"$M/tools/zig/zig" version; "$M/tools/bin/cargo-zigbuild" --version'

# ── 两组各编两个架构 ──────────────────────────────────────────────────
for pair in "base:repo" "engine:repo-engine"; do
  TAG=${pair%%:*}; DIR=${pair##*:}
  docker run --rm --network host --user root -v "$SCRATCH:/m" -e HOME=/root "$IMG" bash -o pipefail -c "
set -uo pipefail
M=/m
export CARGO_HOME=\$M/cargo-home RUSTUP_HOME=\$M/rustup
export PATH=\"\$M/tools/bin:\$M/tools/zig:\$PATH\"
export ZIG_GLOBAL_CACHE_DIR=\$M/zig-cache XDG_CACHE_HOME=\$M/cache
export CARGO_TARGET_DIR=\$M/target-$TAG
cd \$M/$DIR/remote-daemon-proto
for T in x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do
  echo \"===== $TAG / \$T =====\"
  cargo zigbuild --release --target \"\$T\" > \$M/out/$TAG-\$T.log 2>&1
  echo \"EXIT=\$?\"
  B=\$CARGO_TARGET_DIR/\$T/release/cc-monitor-remote
  [ -f \"\$B\" ] && echo \"字节=\$(stat -c %s \"\$B\")\" || tail -20 \$M/out/$TAG-\$T.log
  # 量具自检：strip 无增益是真的吗 —— 看有没有 symtab 段，别只看两个数相等。
  [ -f \"\$B\" ] && (readelf -S \"\$B\" | grep -E 'symtab|debug' || echo '(无 symtab/debug 段 ⇒ strip 本就无增益)')
done" 2>&1 | tee "$SCRATCH/out/$TAG.txt"
done
