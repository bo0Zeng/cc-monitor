#!/usr/bin/env bash
# 读会话台架一整组：同一个后端照 cold（头一次开）· warm（重启、记录没变）· grown（追加 3 000 行后再起）· live（起来之后一口气追加 3 000 行）
# 各跑一趟量时间，再照 cold · warm · grown 经 strace 各跑一趟量每个文件读了多少字节；每组开头把记录还原成刚造好的样子（第一次在工作目录里造：make-big.mjs）。
#   read-big.sh <后端二进制> <工作目录> <标签>
# 工作目录建议在仓里 `.build/` 下；读数写 <工作目录>/<标签>-<阶段>.jsonl，应答写 <工作目录>/<标签>-<阶段>/replies/。
# 两个后端比应答：node read-big.mjs --compare <工作目录>/<甲>-cold <工作目录>/<乙>-cold（grown 同理）。
set -euo pipefail
bin=$(realpath "$1"); work=$2; tag=$3
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$work"; work=$(realpath "$work")
if [ ! -d "$work/pristine" ]; then
  node "$here/make-big.mjs" "$work/box/home" > "$work/made.json"
  cp -r "$work/box/home/.claude/projects" "$work/pristine"
fi
restore() { rm -rf "$work/box/home/.claude/projects"; cp -r "$work/pristine" "$work/box/home/.claude/projects"; }
run() { timeout 900 node "$here/read-big.mjs" "$bin" "$work/box/home" "$work/$tag-$1" "${@:2}" > "$work/$tag-$1.jsonl" 2>&1; }
restore
run cold --phase cold
run warm --phase warm
run grown --phase grown
run live --phase warm --live 3000
# 字节那一组（经 strace，墙钟不看）：每个文件读了多少
restore
run cold-bytes --phase cold --strace
run warm-bytes --phase warm --strace
run grown-bytes --phase grown --strace
for ph in cold warm grown live cold-bytes warm-bytes grown-bytes; do echo "== $tag $ph"; cut -c1-220 "$work/$tag-$ph.jsonl"; done
