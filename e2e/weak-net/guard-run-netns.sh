#!/usr/bin/env bash
# 判据：**运行那一步不许把网况打到用户这台机上**（件 `W-F1` 的 `WF1D3③`，定框 `W3`）。
#
# ## 它判什么
#
# 台架的**运行面**（`e2e/weak-net/` 下除 `build-image.sh` 之外的全部 `.sh`）里，
# 出现下面任何一样就判红：
#   · 宿主网络（`--net host` / `--network host` / `--network=host` 这一族）
#     —— 那样容器与宿主同一个 netns，`tc qdisc add` 直接打在用户的真网卡上。
#   · `--privileged` —— 改本容器 netns 的 qdisc 只需要 `CAP_NET_ADMIN` 一项，
#     给全权限是把「只想改自己的网」换成「什么都能改」。
# `build-image.sh` 单独一档：宿主网络在它那儿是**唯一可行的装包路**（理由写在它的头注里），
# 但 `--privileged` 在**任何**一档里都不许出现，所以它仍受第二条约束。
#
# ## 两处刻意的做法，别"简化"掉
#
# 1. **整行注释先剥掉再扫**。理由不是宽容，是**语义**：整行注释不会被执行，
#    而这几份文件的头注非写清楚这条红线不可（不剥的话，写下"为什么不许用宿主网络"
#    这句话本身就会把判据打红 —— 那是典型的假阳，而假阳会训练人把判据关掉）。
#    ⚠ 刻意**只剥整行**、不剥行尾注释：shell 里 `#` 会出现在字符串中间，按 marker 截断会误伤。
# 2. **needle 用 `[x]` 形写**（如 `--privile[g]ed`）。本文件自己也在被扫的人群里，
#    直接写字面量会**恒红**；而把自己从人群里摘出去，就等于「判据管不到自己」。
#    `[g]` 这个写法匹配得中 `--privileged`，却匹配不中本行 —— 人群一个都不用摘。
#
# ## 🔴 诚实边界（件文件 `§4 w4a` 登记的那条，别把它读大）
#
# 这是**文本判据**，射程只到「字面量写在运行面的 shell 源码里」这一形。它**判不了**：
#   · `NET=host; docker run --network "$NET"` 这类**拼变量**；
#   · 从环境变量读进来的值；
#   · 不经本目录的第三方脚本 / 手打的 `docker run`。
# 真要钉死，得在**跑的时候**读容器自己的 netns（比对容器与宿主的 `/proc/*/ns/net`），
# 那是另一件事，归 `WF1D3` 的解锁条件。这里买到的是「顺手写坏会被逮住」，不是「防得住恶意」。
#
# 用法：bash e2e/weak-net/guard-run-netns.sh   （0 = 干净；1 = 有命中并逐行点名）
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"

# needle：见头注第 2 点，`[x]` 是为了不自匹配，不是笔误。
RE_HOSTNET='--net(work)?[=[:space:]]+hos[t]([[:space:]]|"|'\''|$)'
RE_PRIV='--privile[g]ed'

hits=0
scanned=0

# 只留会被执行的行（剥整行注释），并保留原始行号。
live_lines() {
  grep -nv '^[[:space:]]*#' "$1"
}

report() { # $1=文件 $2=正则 $3=这条红线的一句话
  local f="$1" re="$2" why="$3" out
  # 🔴 `-e` 不是装饰：这两条 needle 都以 `--` 打头，不加 `-e` 时 grep 会把它当成**选项**，
  #    印一行「未识别的选项」然后**返回非零** ⇒ 上层看见的是「没命中」，整条判据零命中地绿。
  #    09-05 第一版就是这么写的，头一趟真跑时它一边刷 grep 报错一边印「干净」。
  out="$(live_lines "$f" | grep -E -e "$re")"
  [ -z "$out" ] && return 0
  while IFS= read -r l; do
    echo "::error::${f#"$HERE"/}: $l"
    echo "::error::  ⇒ $why"
    hits=$((hits + 1))
  done <<<"$out"
}

# ★ 活体自检：**拿现造的坏行喂给同一个匹配器**，匹配不中就说明判据在空转。
#   这不是仪式 —— 上面那条 `-e` 的病正是「判据看起来在跑、其实一条都匹配不中」，
#   而全仓扫一遍**零命中**与**匹配器坏了**在终端上长得一模一样。
#   ⚠ 夹具串**拼出来**（`"--net""work host"`），不写成字面量：本文件自己也在被扫的人群里。
selfcheck() {
  local bad_host bad_priv clean
  bad_host="docker run $(printf '%s%s' '--net' 'work host') img"
  bad_priv="docker run $(printf '%s%s' '--privi' 'leged') img"
  clean='docker run --network "$NET" --cap-add=NET_ADMIN img'
  printf '%s\n' "$bad_host" | grep -qE -e "$RE_HOSTNET" || {
    echo "::error::自检失败：宿主网络那条 needle 匹配不中现造的坏行 ⇒ 判据在空转，按红算。" >&2; exit 2; }
  printf '%s\n' "$bad_priv" | grep -qE -e "$RE_PRIV" || {
    echo "::error::自检失败：全权限那条 needle 匹配不中现造的坏行 ⇒ 判据在空转，按红算。" >&2; exit 2; }
  # 反向对照：干净的那一行**不许**命中，否则这条判据只是一台恒红机器（假阳会让人把它关掉）。
  if printf '%s\n' "$clean" | grep -qE -e "$RE_HOSTNET"; then
    echo "::error::自检失败：合法的运行行被判成宿主网络 ⇒ 假阳，按红算。" >&2; exit 2
  fi
}
selfcheck

for f in "$HERE"/*.sh; do
  [ -e "$f" ] || continue
  scanned=$((scanned + 1))
  if [ "$(basename "$f")" = "build-image.sh" ]; then
    # 装包那一档：宿主网络是唯一可行路（见它的头注）；privileged 仍然不许。
    report "$f" "$RE_PRIV" "装包也不需要全权限"
  else
    report "$f" "$RE_HOSTNET" "跑的时候用宿主网络 = tc 打在用户真网卡上（定框 W3）"
    report "$f" "$RE_PRIV" "改本容器 netns 的 qdisc 只要 --cap-add=NET_ADMIN 一项"
  fi
done

# 抽取器自检：人群塌了的话，上面每一条都会零命中地绿 —— 那种绿比红更坏。
if [ "$scanned" -lt 3 ]; then
  echo "::error::只扫到 $scanned 个脚本（本目录至少该有 build/guard/rig 三份）—— 人群塌了，本判据在空转，按红算。" >&2
  exit 2
fi

if [ "$hits" -gt 0 ]; then
  echo "::error::运行面出现宿主网络 / privileged，共 $hits 处（扫了 $scanned 个脚本）。" >&2
  exit 1
fi
echo "[guard-run-netns] 干净：扫了 $scanned 个脚本，运行面零命中"
