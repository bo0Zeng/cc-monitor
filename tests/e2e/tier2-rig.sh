#!/usr/bin/env bash
# tier-2 台架搭建器 —— 把 `tests/e2e/README.md` 的「全链套件怎么跑」那份**散文配方**
# 变成一条能跑的命令。
#
# 🔴 为什么要有它：那份配方逐字记着四个「踩过才知道」的前提，其中第 2 条的代价是
# **后端会用真 `~/.claude` 起来**（`.build_id` 文件名写错 ⇒ app 判「远端无版本标记」
# ⇒ 把 wrapper 覆盖成内嵌二进制）。让人照散文手搓，就是让下一个人再踩一次。
# 本仓自己的定框 E12 说得最清楚：**散文纪律等于没有纪律。**
#
# 用法：
#   bash tests/e2e/tier2-rig.sh setup     # 建沙箱 ＋ 写 config.json ＋ 起 Xvfb
#   bash tests/e2e/tier2-rig.sh dev       # 起 dev 实例（前台；或自己加 &）
#   bash tests/e2e/tier2-rig.sh run       # 跑 graylight-suite ＋ f40-suite
#   bash tests/e2e/tier2-rig.sh teardown  # 收 dev ＋ vite ＋ Xvfb（按 pid，不用 pkill -f）
#
# 环境变量：SBX（沙箱根，默认 /tmp/e2e-sandbox）· DISP（默认 :80）
set -euo pipefail

REPO="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
SBX="${SBX:-/tmp/e2e-sandbox}"
DISP="${DISP:-:80}"
RIG="$SBX/rig"
BACKEND="$REPO/.build/backend/debug/cc-monitor-remote"
# 真 HOME —— 前提 1：沙箱 HOME 会把 rustup 的家一起换掉，这两个要显式指回来。
REAL_HOME="$(getent passwd "$(id -un)" | cut -d: -f6)"

die() { echo "❌ $*" >&2; exit 1; }

_build_id() {
  # 单一事实源 = 后端源码里那个 const，不抄字面量（抄了就会漂）。
  grep -oP '(?<=const BUILD_ID: &str = ")[^"]+' "$REPO/src/backend/main.rs" \
    || die "抠不到后端的 const BUILD_ID —— 改名了？"
}

setup() {
  [ -x "$BACKEND" ] || die "后端 debug 二进制不在：$BACKEND
   先跑：cd src/backend && cargo build"
  local bid; bid="$(_build_id)"

  # 台架自证 ①：loopback SSH 必须通，否则整套件测不到任何东西。
  ssh -o BatchMode=yes -o IdentitiesOnly=yes -i "$REAL_HOME/.ssh/id_ed25519" \
      -o StrictHostKeyChecking=no localhost true 2>/dev/null \
    || die "ssh localhost 不通（要 sshd + id_ed25519 在 authorized_keys 里）"

  local fp
  fp="$(ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub | awk '{print $2}')"
  [ -n "$fp" ] || die "取不到本机 ed25519 host key 指纹"

  mkdir -p "$SBX/.claude/claudecode-frontend" "$RIG"
  cp "$REPO/tests/e2e/daemon-wrapper.sh" "$RIG/daemon-wrapper.sh"
  chmod +x "$RIG/daemon-wrapper.sh"
  # 🔴 前提 2：文件名逐字是 `.build_id`（同目录隐藏文件），**不是** `<二进制名>.build_id`。
  #    `sftp.rs::marker_path` 是 `format!("{dir}/.build_id")`。写错 ⇒ wrapper 被覆盖。
  printf '%s' "$bid" > "$RIG/.build_id"
  # wrapper 离开仓之后 `$REPO` 解析到别处 ⇒ 它会走「自愈」静默换二进制。
  # 旁边这个 `daemon-path` 钉死用哪个（env 传不进来，SSH exec 不带）。
  printf '%s' "$BACKEND" > "$RIG/daemon-path"

  python3 - "$SBX" "$RIG" "$fp" "$REAL_HOME" <<'PY'
import json, os, sys
sbx, rig, fp, real_home = sys.argv[1:5]
cfg = {
    "remote": {
        "enabled": True,
        "legacyNoBackend": [],
        "hosts": [{
            "label": "e2e-loopback",
            "host": "127.0.0.1",
            "port": 22,
            "user": os.environ.get("USER") or os.getlogin(),
            "keyPath": f"{real_home}/.ssh/id_ed25519",
            "daemonPath": f"{rig}/daemon-wrapper.sh",
            "hostKeyFingerprint": fp,
            "addresses": [],
            "jump": "",
            "resumeCommand": "",
        }],
    }
}
p = f"{sbx}/.claude/claudecode-frontend/config.json"
with open(p, "w", encoding="utf-8") as f:
    json.dump(cfg, f, ensure_ascii=False, indent=2)
print(f"  写了 {p}")
PY

  # Xvfb —— 用 setsid 起，并且**不用 `pkill -f`** 找它：那个模式会匹配到执行 pkill 的
  # 那条 shell 自己的命令行，把自己杀掉（本轮实发一次）。按 :N 的 socket 判在不在。
  if [ -S "/tmp/.X11-unix/X${DISP#:}" ]; then
    echo "  Xvfb $DISP 已在跑"
  else
    setsid /usr/bin/Xvfb "$DISP" -screen 0 1920x1080x24 >"$SBX/xvfb.log" 2>&1 < /dev/null &
    sleep 2
    [ -S "/tmp/.X11-unix/X${DISP#:}" ] || die "Xvfb $DISP 起不来，看 $SBX/xvfb.log"
    echo "  起了 Xvfb $DISP"
  fi

  echo "✅ 台架就绪：SBX=$SBX  DISPLAY=$DISP  build_id=$bid"
  echo "   下一步：bash $0 dev    （另开一个终端/后台）"
}

dev() {
  [ -f "$SBX/.claude/claudecode-frontend/config.json" ] || die "先跑 setup"
  cd "$REPO"
  # 前提 1：RUSTUP_HOME / CARGO_HOME 显式指回真路径（它们不是账号数据）。
  DISPLAY="$DISP" HOME="$SBX" CLAUDE_CONFIG_DIR="$SBX/.claude" \
    RUSTUP_HOME="$REAL_HOME/.rustup" CARGO_HOME="$REAL_HOME/.cargo" \
    CCM_NO_DEVTOOLS=1 exec npx tauri dev
}

run() {
  local log
  log="$(ls -t "$SBX"/.claude/claudecode-frontend/logs/monitor.*.log 2>/dev/null | head -1 || true)"
  [ -n "$log" ] || die "沙箱里没有 monitor 日志 —— dev 实例在跑吗？（bash $0 dev）"
  echo "  日志：$log"
  local rc=0
  for s in graylight-suite f40-suite; do
    printf '▼ %s\n' "$s"
    E2E_DISPLAY="$DISP" E2E_LOG="$log" HOME="$SBX" CLAUDE_CONFIG_DIR="$SBX/.claude" \
      bash "$REPO/tests/e2e/$s.sh" || { rc=$?; echo "  rc=$rc"; }
  done
  return "$rc"
}

teardown() {
  # 按 pid 精确收（含 vite —— 前提 4：只收 tauri 那个 node，vite 还占着 devUrl 端口）。
  local pids
  pids="$(ps -eo pid,args | awk '/[c]argo run --no-default-features/ || /[t]auri dev/ {print $1}')"
  local vite
  vite="$(ss -ltnp 2>/dev/null | awk '/:24174/{match($0,/pid=([0-9]+)/,m); print m[1]}' | head -1)"
  for p in $pids $vite; do kill "$p" 2>/dev/null && echo "  收了 $p"; done
  local sock="/tmp/.X11-unix/X${DISP#:}"
  if [ -S "$sock" ]; then
    local xp; xp="$(ps -eo pid,comm,args | awk -v d="$DISP" '$2=="Xvfb" && $0 ~ d {print $1}')"
    for p in $xp; do kill "$p" 2>/dev/null && echo "  收了 Xvfb $p"; done
  fi
  echo "✅ 收尾完"
}

case "${1:-}" in
  setup) setup ;; dev) dev ;; run) run ;; teardown) teardown ;;
  *) echo "用法: $0 {setup|dev|run|teardown}"; exit 64 ;;
esac
