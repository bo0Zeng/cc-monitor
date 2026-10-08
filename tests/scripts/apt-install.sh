#!/usr/bin/env bash
# CI 上装 apt 包的唯一写法（`.github/workflows/ci.yml` · `release.yml` 里每一处都调它）：卡住几分钟就失败、自己重来。
#
# 10-07 两回：runner 上 apt 挂住，整个 job 等满 35 分钟被取消。于是：
#   - 实测（主线 2e9f26d1f 那趟 CI）不是整个卡死，是从镜像下**单个小包**时停住：105 kB 的包前停了 521 秒、32 kB 的停了 625 秒。
#     ⇒ apt 自己带短的连接 / 读超时（`Acquire::http(s)::Timeout=20`，秒）＋ `Acquire::Retries=3`：单个包停住 20 秒就换连接重下，
#     不等十分钟；
#   - 每一趟 `apt-get update` / `install` 外面再套 `timeout`（上面那道没兜住的，这一趟几分钟内被杀）；
#     等 dpkg 锁也有上限（`DPkg::Lock::Timeout`），不无限等别的 apt 进程；
#   - 一趟没成 ⇒ 收拾被杀掉一半的 dpkg（`dpkg --configure -a`）、歇一下再来，最多 3 趟；
#   - 一趟最多 60 s（update）＋ 200 s（install）≈ 4 分多；3 趟连歇在内约 13 分半。
#     调它的那一步自己再带 `timeout-minutes: 14`（最外一道闸，比这里 3 趟的最坏情形略长）。
#   正常一趟 update 十几秒、install 一两分钟（09-10 实测 linux-app-build 整个 job 1–2m）。
#   没上 `actions/cache` 缓存 .deb：`/var/cache/apt/archives` 归 root、恢复要 sudo 搬，仓里也没有这个习惯；
#   上面两道超时已经让「单包停住」几十秒内换连接。
# 判据：`shared_crate_registry_tests.rs::every_apt_install_in_ci_sits_in_a_step_with_its_own_timeout`。
#
# 用法：bash tests/scripts/apt-install.sh <包> [<包> …]
set -uo pipefail

[ "$#" -gt 0 ] || { echo "apt-install: 没给包名" >&2; exit 2; }

tries=3
opts=(-o Acquire::Retries=3 -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20 -o DPkg::Lock::Timeout=60)
for ((n = 1; n <= tries; n++)); do
  if timeout -k 10 60 sudo apt-get "${opts[@]}" update &&
     timeout -k 10 200 sudo env DEBIAN_FRONTEND=noninteractive apt-get "${opts[@]}" install -y "$@"; then
    exit 0
  fi
  echo "apt-install: 第 $n 趟没装成（超时被杀或失败）" >&2
  if [ "$n" -lt "$tries" ]; then
    sudo dpkg --configure -a || true
    sleep 15
  fi
done
echo "apt-install: $tries 趟都没装成：$*" >&2
exit 1
