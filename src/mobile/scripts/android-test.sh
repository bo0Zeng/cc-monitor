#!/usr/bin/env bash
# android-test.sh — 要设备的 instrumentation 测（androidTest）的唯一入口。
#
# 用法：先接好一台模拟器或真机（adb devices 看得到；多台时设 ANDROID_SERIAL），再
#   . ./scripts/env.sh && bash scripts/android-test.sh
#
# 进全量门禁（10-11 起）：`tests/scripts/gate.sh` 的 `android-device` 那一格经 `tests/mobile-smoke/emu.sh`
#   起一台封了内存的模拟器再调本脚本。CI 不跑（runner 上没有 KVM 模拟器）；门禁 `mobile` 格只跑 JVM 单测。
# 跑的是：:core-data 的 Room 迁移 / DB / Keystore、:app 的 UI 冒烟与重放、:core-ssh 的硬件 ECDSA 签名。
# 跑之前先装上 app、`pm clear` 清它的数据（退出码 0 且回 `Success` 才往下跑，清不掉就停）：
#   上一趟留下的数据会让冒烟落到「进过服务器」那一支，「从没连过 ⇒ 落在服务器列表」那一支就走不到了。
# gradle 一律 --no-daemon：守护进程会留在 heavy 的 scope 里、一直占着槽位。
# core-data 与 app 合进一次 gradlew 加 --continue：一个挂了另一个照跑，两份报告都在。
# core-ssh 单开一条：RustTransportTest 要台架参数（不给就整份跳过），手连台架时照它头注另跑。
# 这份脚本的形状由 core-data 的 InstrumentedTestInventoryTest 钉着（JVM 单测，门禁里跑）；
# 清数据与 --no-daemon 由后端那棵的 `flow_scripts_guard` 拿假 gradlew / adb 真跑一遍钉着。
set -euo pipefail
cd "$(dirname "$0")/.."
pkg=com.ccmonitor.mobile
./gradlew --no-daemon --console=plain :app:installDebug --stacktrace
said="$(adb shell pm clear "$pkg" 2>&1)" && clear_rc=0 || clear_rc=$?
said="${said//$'\r'/}"
if [ "$clear_rc" -ne 0 ] || [ "$said" != Success ]; then
  echo "android-test: pm clear $pkg 没成（退出码 $clear_rc，回话「$said」）—— 带着上一趟的数据跑判不了「从没连过」那一支，停" >&2
  exit 1
fi
echo "android-test: pm clear $pkg ⇒ Success"
rc=0
./gradlew --no-daemon --console=plain :core-data:connectedDebugAndroidTest :app:connectedDebugAndroidTest :core-rs:connectedDebugAndroidTest --continue --stacktrace || rc=$?
# core-ssh：导入时派生公钥 · RustTransport（后者不带台架参数时整份跳过；要连台架的跑法见 core-ssh 的 RustTransportTest 头注）。
./gradlew --no-daemon --console=plain :core-ssh:connectedDebugAndroidTest --stacktrace || rc=$?
exit "$rc"
