#!/usr/bin/env bash
# 建弱网台架镜像。**本仓唯一允许出现宿主网络的一步**（定框 `W3`）。
#
# ## 为什么 build 这一步非用宿主网络不可
#
# 这台开发机的出网靠宿主上的一个本地代理（`127.0.0.1:7890`）。容器**默认 bridge 网络里**
# 那个地址是容器自己的回环，走 `172.17.0.1:7890`（宿主在 docker0 上的地址）实测**超时不通**
# ⇒ 装包唯一可行路是让 build 容器直接用宿主的 netns，再指 `127.0.0.1:7890`。
#
# ## 为什么这在 W3 的红线之内
#
# W3 禁的是**跑的时候**用宿主网络 —— 那样 `tc`/`netem` 会打在用户这台机的真网卡上。
# `docker build` 只是装包，**一条 tc 规则都不加**；而 `rig.sh` 跑的时候一律自建 docker 网络
# + `--cap-add=NET_ADMIN`，不给 `--privileged`。两件事在同一份代码里的边界由
# `guard-run-netns.sh` 那条文本判据看着：**运行面的文件里出现宿主网络或 privileged 就判红**，
# 而本文件被排除在运行面之外（排除的理由就是上面这一段）。
#
# ## 代理是写死的，但不通的时候会大声说，不会静默
#
# 地址写死在下面 `PROXY_HOST` / `PROXY_PORT` 两个常量里（定框 `W3` 那条读数量于它们）。
# 探法是**裸 TCP 连一下那个端口**（不发 HTTP，也不依赖 curl 通不通）：
#   · 通  ⇒ 走代理（这台开发机）
#   · 不通 ⇒ 印一行「代理不通，改直连」再继续（GitHub runner 走的是这条：它本来就能出网）
# 刻意不做成「不通就退」：那会让本套件在任何没有这个代理的机器上永远建不出镜像。
# 也刻意不做成静默回落：静默回落时「代理挂了」与「本来就不需要代理」长得一模一样。
#
# 用法：
#   bash e2e/weak-net/build-image.sh            # 镜像已在就跳过
#   bash e2e/weak-net/build-image.sh --force    # 强制重建
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
IMG="${WEAKNET_IMAGE:-ccmon-weaknet:latest}"

# 🔴 写死的代理（定框 W3 的那一个）。给了 `WEAKNET_BUILD_PROXY_HOST=` 空串就是「别用代理」。
PROXY_HOST="${WEAKNET_BUILD_PROXY_HOST-127.0.0.1}"
PROXY_PORT="${WEAKNET_BUILD_PROXY_PORT-7890}"

FORCE=0
[ "${1:-}" = "--force" ] && FORCE=1

command -v docker >/dev/null 2>&1 || {
  echo "需要 docker（本套件全程在容器里造网况，宿主网络一个字节不动）" >&2
  exit 2
}

if [ "$FORCE" -eq 0 ] && docker image inspect "$IMG" >/dev/null 2>&1; then
  echo "[build-image] 镜像已在：$IMG（要重建加 --force）"
  exit 0
fi

ARGS=()
if [ -n "$PROXY_HOST" ] && timeout 2 bash -c ": >/dev/tcp/$PROXY_HOST/$PROXY_PORT" 2>/dev/null; then
  P="http://$PROXY_HOST:$PROXY_PORT"
  echo "[build-image] 代理 $P 在听 ⇒ 走代理装包"
  ARGS+=(--build-arg "http_proxy=$P" --build-arg "https_proxy=$P")
  ARGS+=(--build-arg "HTTP_PROXY=$P" --build-arg "HTTPS_PROXY=$P")
else
  echo "[build-image] 代理 $PROXY_HOST:$PROXY_PORT 不通 ⇒ 改直连装包（CI runner 走的是这条）"
fi

# 🔴 下面这一行是本仓唯一一处宿主网络，理由见头注；它属于 build，不属于 run。
echo "[build-image] docker build（装包这一步用宿主 netns —— 见本文件头注）→ $IMG"
docker build --network host "${ARGS[@]}" -t "$IMG" -f "$HERE/Dockerfile" "$HERE"
rc=$?
if [ "$rc" -ne 0 ]; then
  echo "::error::建镜像失败（退出码 $rc）。这台机器上不了外网时唯一可行路是宿主上那个代理在听。" >&2
  exit "$rc"
fi
echo "[build-image] 建好了：$IMG"
