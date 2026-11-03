#!/usr/bin/env bash
# baseline-profile.sh — 录 Baseline Profile · 量冷启的入口（`:baselineprofile`；要一台模拟器或真机，不进门禁）。
#
# 用法：CCM_SMOKE_ADB_PORT=… CCM_SMOKE_EMU_PORT=… bash scripts/baseline-profile.sh gen|bench
#   gen    两条路（冷启到主屏 · 打开长会话往上滑）各走几遍，规则写进 app/src/main/generated/baselineProfiles/baseline-prof.txt（进仓）。
#          界面大改之后重录；录完跑一遍 :app:testDebugUnitTest 的 BaselineProfileTest，看两条路的类还在不在。
#   bench  量：冷启（StartupTimingMetric）· 长会话往上滑（FrameTimingMetric），不预编译（None）与照包里的 profile（Partial(Require)）各 5 趟。
#          读数在 .build/mobile/baselineprofile/outputs/connected_android_test_additional_output/ 底下的 *-benchmarkData.json。
# 模拟器：经 tests/mobile-smoke/emu.sh 起（无头 · KVM · 私有 adb 口 · 自己一个封了内存的 scope；AVD 目录照它的 CCM_SMOKE_AVD_HOME），
#   收场（连中途红了）关掉、停那个私有 adb。两个口都要给（不落默认口），同 emu.sh。模拟器上的数只作相对比较。
# 接真机：设 CCM_BP_DEVICE=1 不起模拟器，用 adb 连着的那一台（多台时设 ANDROID_SERIAL）。
# 先把要的包编好再跑（编译照本仓的编译锁排队；本脚本跑在模拟器上，不持锁）。
set -euo pipefail
mobile=$(cd "$(dirname "$0")/.." && pwd)
root=$(cd "$mobile/../.." && pwd)
case "${1:-}" in
  # 插件的汇总任务把每个派生支都录一遍：`release` 那一支（arm64-v8a）摘掉，只录 `minified` 那一支、并进 src/main。
  gen) task=(:app:generateBaselineProfile -x :baselineprofile:collectNonMinifiedReleaseBaselineProfile) ;;
  bench) task=(:baselineprofile:connectedBenchmarkMinifiedAndroidTest) ;;
  *) echo "用法：baseline-profile.sh gen|bench" >&2; exit 2 ;;
esac
cd "$mobile"
# shellcheck disable=SC1091
. ./scripts/env.sh >/dev/null
args=()
if [ -z "${CCM_BP_DEVICE:-}" ]; then
  emu="$root/tests/mobile-smoke/emu.sh"
  trap 'bash "$emu" down' EXIT
  bash "$emu" up
  eval "$(bash "$emu" env)"
fi
# Macrobenchmark 在模拟器上默认拒跑（数不代表真机）：是模拟器就放行，读数只作相对比较。
[ "$(adb shell getprop ro.kernel.qemu 2>/dev/null | tr -d '\r')" = 1 ] &&
  args+=(-Pandroid.testInstrumentationRunnerArguments.androidx.benchmark.suppressErrors=EMULATOR)
./gradlew --no-daemon --console=plain "${task[@]}" "${args[@]}"
