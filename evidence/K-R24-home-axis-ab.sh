#!/usr/bin/env bash
# K-R24 下一拍㈡ · `HOME` 这条轴的 A/B 量具
#
# ── 尺子（逐字沿用上一拍那把，只换轴）────────────────────────────────────
#   「一条判据，它的**判决**随一条**环境事实**翻转，而它**既不建立、也不检查**那条事实。」
#   判法：**同一棵树、同一个提交、同一个镜像、同一份挂载、同一个网络口径，
#         只换 `HOME` 这一条环境事实，看哪些判决翻转。**
#   🔴 刻意**不切成**「代码里出现了 `HOME` / `dirs::home_dir()`」—— 那是词表的形状，
#      只认它记得的写法；上面这个是行为的形状，机器判得了。
#
# ── 为什么是 `HOME` 这条轴 ────────────────────────────────────────────
#   `.claude/devbox/gate` 头注**自己写着**〔`K-H2b` `D9` 08-29〕：
#   容器里 `HOME=/home/zbl` **存在但几乎是空的**（只有 `文档/claudecode-frontend` 挂进来）
#   ⇒ 任何读真实家目录的判据，**沙箱与宿主未必同值**。
#   ⇒ 这条轴仓里有账，但从没人拿尺子量过它上面有几条。
#
# ── 四格怎么切的 ───────────────────────────────────────────────────
#   A  HOME=/home/zbl          + mkdir $HOME/.claude/projects   ← `.claude/devbox/gate` 逐字那套（基线）
#   B  HOME=/home/zbl          + **不** mkdir                    ← 只撤掉「门禁替判据建的那个目录」
#   C  HOME=/tmp/home-elsewhere + mkdir $HOME/.claude/projects   ← 家目录换个地方（目录仍在）
#   D  HOME=/tmp/home-elsewhere + **不** mkdir                    ← 两样一起换
#   ⚠ C/D 那条轴其实动了**两样**：家目录的**路径**变了，而且项目不再落在它下面。
#     这是这条轴本身的形状，**记在这里，别读成只动了路径**。
#
# ── 每格跑两趟，因为两趟看见的粒度不同 ─────────────────────────────────
#   ① `bash scripts/gate.sh`  —— 门禁自己那九格的裁决（npm / e2e / pb check 只到**格**这一级）
#   ② 两条 cargo 命令的**全量逐条输出** —— Rust 那两格要**逐条**判决才看得出是哪一条翻的
#
# 🔴 本脚本**不改 `.claude/devbox/gate`**，也不给门禁放网络（四格一律 `--network none`）。
#    它是**另起一个 docker run**，逐字复刻 gate 的挂载与环境，**只把 `HOME` 那一维换掉**。
#
# 用法: evidence/K-R24-home-axis-ab.sh <A|B|C|D> <输出目录>
set -o pipefail

CELL="${1:?用法: K-R24-home-axis-ab.sh <A|B|C|D> <输出目录>}"
OUT="${2:?用法: K-R24-home-axis-ab.sh <A|B|C|D> <输出目录>}"

PROJ=/home/zbl/文档/claudecode-frontend
SKILL=/home/zbl/.claude-accts/z/skills/planned-build
WT="$PROJ/.claude/worktrees/k-r24b"
TARGETS="$PROJ/.claude/pm-targets/k-r24b"

case "$CELL" in
  A) HOME_VAL=/home/zbl            ; PRE='mkdir -p "$HOME/.claude/projects" && ' ;;
  B) HOME_VAL=/home/zbl            ; PRE='' ;;
  C) HOME_VAL=/tmp/home-elsewhere  ; PRE='mkdir -p "$HOME/.claude/projects" && ' ;;
  D) HOME_VAL=/tmp/home-elsewhere  ; PRE='' ;;
  *) echo "不认识的格：$CELL（只有 A/B/C/D）" >&2; exit 3 ;;
esac

mkdir -p "$OUT"

run_in_box() {  # run_in_box <容器内 bash -c 的命令串>
  docker run --rm \
    --network none \
    -v "$PROJ:$PROJ" \
    -v "$SKILL:$SKILL:ro" \
    -v "ccmon-cargo-registry:/opt/rust/cargo/registry" \
    -e "CARGO_TARGET_DIR=$TARGETS" \
    -e "HOME=$HOME_VAL" \
    -e PB_WS \
    -w "$WT" \
    ccmon-devbox:latest \
    bash -o pipefail -c "$1"
}

echo "── 格 $CELL：HOME=$HOME_VAL ; mkdir=${PRE:+有}${PRE:-无} ; --network none ──"

# ① 门禁九格
run_in_box "${PRE}bash scripts/gate.sh" >"$OUT/$CELL.gate.log" 2>&1
echo "  gate rc=$?  ⇒ $OUT/$CELL.gate.log"

# ② Rust 两格的逐条判决
run_in_box "${PRE}cd src-tauri && cargo test --workspace --exclude code-picture-core --lib" \
  >"$OUT/$CELL.cargo.log" 2>&1
echo "  cargo rc=$?  ⇒ $OUT/$CELL.cargo.log"

run_in_box "${PRE}cd remote-daemon-proto && cargo test" \
  >"$OUT/$CELL.daemon.log" 2>&1
echo "  daemon rc=$? ⇒ $OUT/$CELL.daemon.log"
