#!/usr/bin/env bash
# android-test.sh — 要设备的 instrumentation 测（androidTest）的唯一入口。
#
# 用法：先接好一台模拟器或真机（adb devices 看得到），再
#   . ./scripts/env.sh && bash scripts/android-test.sh
#
# 不进本仓门禁、也不进 CI：门禁与 CI 的 mobile 格只跑 JVM 单测（不起模拟器）。
# 跑的是：:core-data 的 Room 迁移 / DB / Keystore、:app 的 UI 冒烟与重放、:core-ssh 的硬件 ECDSA 签名。
# core-data 与 app 合进一次 gradlew 加 --continue：一个挂了另一个照跑，两份报告都在。
# core-ssh 单开一条，因为 -P 的 instrumentation 过滤是全局的；它只跑 KeystoreSigner，
# 要一台真 SSH 服务端的 SshConnectionTest 不在这里跑（留真机手测）。
# 这份脚本的形状由 core-data 的 InstrumentedTestInventoryTest 钉着（JVM 单测，门禁里跑）。
set -euo pipefail
cd "$(dirname "$0")/.."
rc=0
./gradlew :core-data:connectedDebugAndroidTest :app:connectedDebugAndroidTest --continue --stacktrace || rc=$?
./gradlew :core-ssh:connectedDebugAndroidTest -Pandroid.testInstrumentationRunnerArguments.class=com.ccmonitor.mobile.core.ssh.KeystoreSignerInstrumentedTest --stacktrace || rc=$?
exit "$rc"
