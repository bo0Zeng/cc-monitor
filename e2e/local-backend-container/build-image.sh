#!/usr/bin/env bash
# 建干净 Linux 容器台架的镜像。**本目录唯一允许出现宿主网络的一步**（同 `e2e/weak-net/build-image.sh`）。
#
# ## 为什么 build 这一步非用宿主网络不可
#
# 这台开发机的出网靠宿主上的一个本地代理（`127.0.0.1:7890`）。容器**默认 bridge 网络里**
# 那个地址是容器自己的回环，走 `172.17.0.1:7890`（宿主在 docker0 上的地址）实测**超时不通**
# ⇒ 装包唯一可行路是让 build 容器直接用宿主的 netns，再指 `127.0.0.1:7890`。
# 这一段与 `e2e/weak-net/build-image.sh` 是同一条现实，不是抄来的措辞。
#
# ## 为什么这在红线之内
#
# 红线禁的是**跑的时候**用宿主网络。`docker build` 只是装包；而 `rig.sh` 跑的时候一律
# **自建 `--internal` docker 网络**（内部网，无出网）+ **不给任何 `--cap-add`、不给 `--privileged`**。
# 两件事在同一份代码里的边界由 `guard-run-netns.sh` 那条文本判据看着，
# 而本文件被排除在运行面之外（排除的理由就是上面这一段）。
#
# ## 代理是写死的，但不通的时候会大声说，不会静默
#
# 探法是**裸 TCP 连一下那个端口**（不发 HTTP，也不依赖 curl 通不通）：
#   · 通  ⇒ 走代理（这台开发机）
#   · 不通 ⇒ 印一行「代理不通，改直连」再继续（CI runner 走的是这条）
# 刻意不做成「不通就退」，也刻意不做成静默回落：静默回落时「代理挂了」与
# 「本来就不需要代理」长得一模一样。
#
# 用法：
#   bash e2e/local-backend-container/build-image.sh            # 镜像已在就跳过
#   bash e2e/local-backend-container/build-image.sh --force    # 强制重建
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
IMG="${LBC_IMAGE:-ccmon-lbc:latest}"

# 🔴 写死的代理。给了 `LBC_BUILD_PROXY_HOST=` 空串就是「别用代理」。
PROXY_HOST="${LBC_BUILD_PROXY_HOST-127.0.0.1}"
PROXY_PORT="${LBC_BUILD_PROXY_PORT-7890}"

FORCE=0
[ "${1:-}" = "--force" ] && FORCE=1

command -v docker >/dev/null 2>&1 || {
  echo "需要 docker（本套件全程在容器里跑，宿主上一个测试都不跑）" >&2
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

# 🔴 下面这一行是本目录唯一一处宿主网络，理由见头注；它属于 build，不属于 run。
echo "[build-image] docker build（装包这一步用宿主 netns —— 见本文件头注）→ $IMG"
docker build --network host "${ARGS[@]}" -t "$IMG" -f "$HERE/Dockerfile" "$HERE"
rc=$?
if [ "$rc" -ne 0 ]; then
  echo "::error::建镜像失败（退出码 $rc）。这台机器上不了外网时唯一可行路是宿主上那个代理在听。" >&2
  exit "$rc"
fi
echo "[build-image] 建好了：$IMG"
