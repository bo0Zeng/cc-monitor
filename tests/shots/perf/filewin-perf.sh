#!/usr/bin/env bash
# 文件窗口（egui）性能台架：铺一个几千项的合成目录，起私有 Xvfb，在里面点起 `perf_rig_worker`
# （生产开窗那一条 ＋ 合成后端），印每一段的 CPU 毫秒与帧数。
#   filewin-perf.sh <测试二进制> [项数]
# 环境：白名单（PATH · LANG · 临时 HOME / XDG_RUNTIME_DIR）、无会话总线；不碰真家目录。
set -euo pipefail
exe=$1
n=${2:-5000}
repo=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
box=$(mktemp -d "${TMPDIR:-/tmp}/filewin-perf.XXXXXX")
trap 'kill "${xpid:-0}" 2>/dev/null || true; [ -n "${dnum:-}" ] && bash "$repo/tests/scripts/xvfb-free.sh" release "$dnum" "$xpid"; rm -rf "$box"' EXIT
mkdir -p "$box/home" "$box/run" "$box/dir/big"
chmod 700 "$box/run"
# 合成目录：`n` 个文件（不同扩展名，名字按序号）＋ 几十个子目录
( cd "$box/dir/big"
  for ((i = 0; i < n; i++)); do
    case $((i % 5)) in 0) e=rs ;; 1) e=ts ;; 2) e=md ;; 3) e=json ;; *) e=log ;; esac
    printf -v f 'f%05d.%s' "$i" "$e"; : > "$f"
  done
  for ((i = 0; i < 40; i++)); do mkdir "d$i"; done )
exec 3< <(bash "$repo/tests/scripts/xvfb-free.sh")
xpid=$!
read -r dnum <&3
env -i PATH=/usr/bin:/bin LANG=C.UTF-8 HOME="$box/home" XDG_RUNTIME_DIR="$box/run" \
  DISPLAY=":$dnum" CCM_PERF_FILEWIN_DIR="$box/dir" CCM_FILEWIN_FRAME_LOG="$box/frames.log" \
  CCM_FILEWIN_XVFB_CHILD=1 \
  timeout 180 "$exe" --ignored --exact workspace::tests::perf_rig_worker --nocapture --test-threads 1 2>&1 \
  | grep -E '^perf\.|panicked|FAILED|error' || true
[ -n "${KEEP:-}" ] && cp "$box/frames.log" "$KEEP" || true
