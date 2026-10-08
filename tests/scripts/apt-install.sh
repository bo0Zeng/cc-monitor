#!/usr/bin/env bash
# CI 上装 apt 包的唯一写法（`.github/workflows/ci.yml` · `release.yml` 里每一处都调它）：停住就换连接、慢也装得完、下好的 .deb 进缓存。
#
# 两种失败，各一道闸：
#   - **停住**（10-07 两回，主线 2e9f26d1f）：从镜像下单个小包时不动了（105 kB 的包前停了 521 秒、32 kB 的停了 625 秒）。
#     ⇒ apt 自己带短的连接 / 读超时（`Acquire::http(s)::Timeout=20`，秒）＋ `Acquire::Retries=3`：停住 20 秒就换连接重下；
#     等 dpkg 锁也有上限（`DPkg::Lock::Timeout`），不无限等别的 apt 进程。
#   - **很慢但在动**（10-08，主线 83f27b4bc 那趟 linux-app-build）：azure 镜像约 125 KB/s
#     （22.1 MB 用了 174 秒；`libwebkit2gtk-4.1-0` 25.5 MB 在 199 秒里没下完），旧的每趟 install 固定 200 秒的闸把它连杀三趟。
#     ⇒ 不再给每趟一个固定的小闸，改成**一次调用的总时限** `budget`：update 每趟 60 秒，install 那一趟的闸是总时限里还剩的秒数。
#     总时限照最慢实测算：一个 job 一次最多装约 123 MB（rust-linux：构建依赖 61.9 MB ＋ Xvfb / CJK 字体 61.3 MB），
#     125 KB/s 下约 16 分钟；再加 update、解包、一回重来的歇口 ⇒ `budget=1200`（20 分钟）。
#     一趟没成 ⇒ 收拾被杀掉一半的 dpkg（`dpkg --configure -a`）、歇一下再来，最多 3 趟、都在同一个总时限里；
#     已经下完的 .deb 留在下载目录，下一趟不再下。
#     调它装包的那一步自己再带 `timeout-minutes: 21`（比总时限长一点，让脚本自己收尾报错），job 的时限再比它长。
#   - **缓存**：.deb 下到 `$RUNNER_TEMP/apt-archives`（`-o Dir::Cache::Archives=`；当前用户写得动 ——
#     `/var/cache/apt/archives` 归 root，官方 `actions/cache` 存取不了），前一步用 `actions/cache@v4` 存取这个目录。
#     这个目录 apt 的下载降权用户 `_apt` 进不去（家目录 750）⇒ `APT::Sandbox::User=root`，免得每个包一行「unsandboxed」的提示。
#     键 = 包清单（参数排序去重后）的摘要 ＋ 运行器镜像（`ImageOS` · `ImageVersion`）；镜像换版本时先按同一份包清单取最近的一份，
#     只补下变了的包，装完 `autoclean` 清掉镜像上已经没有的旧版本。命中时 install 不再下载。
#   正常（镜像快）一趟 update 几秒、install 十几秒到一分钟。
# 判据：`shared_crate_registry_tests.rs::every_apt_install_in_ci_sits_in_a_step_with_its_own_timeout`
#       · `every_job_that_installs_apt_packages_restores_the_deb_cache_first`。
#
# 用法：bash tests/scripts/apt-install.sh <包> [<包> …]
#       bash tests/scripts/apt-install.sh --cache-key <包> [<包> …] >> "$GITHUB_OUTPUT"   # 打出 dir= / key= / restore= 三行
set -uo pipefail

mode=install
if [ "${1:-}" = --cache-key ]; then
  mode=key
  shift
fi
[ "$#" -gt 0 ] || { echo "apt-install: 没给包名" >&2; exit 2; }
archives="${RUNNER_TEMP:?apt-install: 只在 CI 上用（要 RUNNER_TEMP）}/apt-archives"

if [ "$mode" = key ]; then
  pkgs=$(printf '%s\n' "$@" | sort -u | sha256sum | cut -c1-16)
  # shellcheck disable=SC1091  # 运行器上的 /etc/os-release，只在 ImageOS 缺席时兜底
  image="${ImageOS:-$(. /etc/os-release && echo "$ID$VERSION_ID")}"
  echo "dir=$archives"
  echo "key=apt-v1-$image-$pkgs-${ImageVersion:-unknown}"
  echo "restore=apt-v1-$image-$pkgs-"
  exit 0
fi

mkdir -p "$archives/partial"
tries=3
budget=1200
opts=(-o Acquire::Retries=3 -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20 -o DPkg::Lock::Timeout=60
      -o "Dir::Cache::Archives=$archives/" -o APT::Keep-Downloaded-Packages=true -o APT::Sandbox::User=root)
echo "apt-install: 下载目录里已有 $(find "$archives" -maxdepth 1 -name '*.deb' | wc -l) 个 .deb（缓存放回来的）"

# 装完把下载目录交还给当前用户（apt 以 root 写，锁文件只有 root 读得了），缓存那一步才存得下。
tidy() {
  timeout -k 10 60 sudo apt-get "${opts[@]}" autoclean >/dev/null || true
  sudo rm -rf "$archives/partial" "$archives/lock"
  sudo chown -R "$(id -u):$(id -g)" "$archives"
}

start=$SECONDS
for ((n = 1; n <= tries; n++)); do
  if timeout -k 10 60 sudo apt-get "${opts[@]}" update; then
    left=$((budget - (SECONDS - start)))
    if timeout -k 10 "$left" sudo env DEBIAN_FRONTEND=noninteractive apt-get "${opts[@]}" install -y "$@"; then
      tidy
      echo "apt-install: 装好了（第 $n 趟，用了 $((SECONDS - start)) 秒）"
      exit 0
    fi
  fi
  echo "apt-install: 第 $n 趟没装成（超时被杀或失败，已用 $((SECONDS - start)) / $budget 秒）" >&2
  [ "$n" -lt "$tries" ] || break
  if [ $((budget - (SECONDS - start))) -le 120 ]; then
    echo "apt-install: 总时限只剩不到两分钟，不再来一趟" >&2
    break
  fi
  sudo dpkg --configure -a || true
  sleep 15
done
echo "apt-install: 没装成（最多 $tries 趟、总时限 $budget 秒）：$*" >&2
exit 1
