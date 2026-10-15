#!/usr/bin/env bash
# 冒烟台架的模拟器：无头、KVM，AVD 放台架目录（磁盘上，不放 /tmp 内存盘），adb 用私有端口（不碰本机正在跑的 adb 与真手机）。
# 用法：bash tests/mobile-smoke/emu.sh up|down|env
#   up    没有 AVD 就建（要 SDK 里已装 emulator 与 system-images;android-36;google_apis;x86_64），起模拟器、等开机完。
#         模拟器单独起一个封了内存的 scope（systemd-run --user --scope，MemoryHigh 6G / MemoryMax 8G），不占 heavy 的槽位。
#   down  关模拟器、停私有 adb。
#   env   印出要 source 的那几格（ANDROID_* · ANDROID_ADB_SERVER_PORT · ANDROID_SERIAL · PATH）。
# 端口不替人挑：CCM_SMOKE_ADB_PORT（私有 adb server）· CCM_SMOKE_EMU_PORT（模拟器）两个都要给，缺一个就拒、退 2
#   （落到一个默认口上会撞上别的台架、顺手起一个 adb server）。各入口（smoke.sh · device.sh · 门禁那一格）自己定各自的口。
# 工具链照 src/mobile/scripts/env.sh（默认 $HOME/android-dev）。可改：CCM_SMOKE_AVD_HOME。
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
box=${CCM_SMOKE_BOX:-$root/.build/mobile-smoke/box}
avd_home=${CCM_SMOKE_AVD_HOME:-$box/../avd}
if [ -z "${CCM_SMOKE_ADB_PORT:-}" ] || [ -z "${CCM_SMOKE_EMU_PORT:-}" ]; then
  echo "emu: 要先给端口：CCM_SMOKE_ADB_PORT（adb server）与 CCM_SMOKE_EMU_PORT（模拟器）两个都要设（不落默认口）" >&2
  exit 2
fi
adb_port=$CCM_SMOKE_ADB_PORT
emu_port=$CCM_SMOKE_EMU_PORT
image="system-images;android-36;google_apis;x86_64"

activate() {
  # shellcheck disable=SC1091
  . "$root/src/mobile/scripts/env.sh" >/dev/null
  export ANDROID_AVD_HOME="$avd_home" ANDROID_ADB_SERVER_PORT="$adb_port" ANDROID_SERIAL="emulator-$emu_port"
  export PATH="$ANDROID_HOME/emulator:$PATH"
}

up() {
  activate
  [ -x "$ANDROID_HOME/emulator/emulator" ] || { echo "emu: SDK 里没有 emulator 包（sdkmanager \"emulator\" \"$image\"）" >&2; exit 2; }
  mkdir -p "$avd_home" "$box/logs"
  if [ ! -d "$avd_home/smoke.avd" ]; then
    echo no | avdmanager create avd -n smoke -k "$image" -d pixel_6 --force >/dev/null
  fi
  if ! timeout 5 adb -s "$ANDROID_SERIAL" get-state >/dev/null 2>&1; then
    setsid systemd-run --user --scope --quiet -p MemoryHigh=6G -p MemoryMax=8G -- \
      emulator -avd smoke -port "$emu_port" -no-window -no-audio -no-boot-anim -no-snapshot \
      -gpu swiftshader_indirect -accel on -memory 4096 > "$box/logs/emulator.log" 2>&1 &
  fi
  local waited=0
  until timeout 5 adb shell getprop sys.boot_completed 2>/dev/null | grep -q 1; do
    sleep 3; waited=$((waited + 3))
    [ "$waited" -lt 300 ] || { echo "emu: 五分钟没开机（看 $box/logs/emulator.log）" >&2; exit 1; }
  done
  echo "emu: 开机了（$ANDROID_SERIAL，adb 私有端口 $adb_port）" >&2
}

down() {
  activate
  timeout 10 adb emu kill >/dev/null 2>&1 || true
  adb kill-server >/dev/null 2>&1 || true
}

case "${1:-}" in
  up) up ;;
  down) down ;;
  env) activate; printf 'export ANDROID_HOME=%q ANDROID_AVD_HOME=%q ANDROID_ADB_SERVER_PORT=%q ANDROID_SERIAL=%q PATH=%q\n' \
    "$ANDROID_HOME" "$ANDROID_AVD_HOME" "$ANDROID_ADB_SERVER_PORT" "$ANDROID_SERIAL" "$PATH" ;;
  *) echo "用法：emu.sh up|down|env" >&2; exit 2 ;;
esac
