#!/usr/bin/env bash
# 真机代测台架：模拟器上能代真机核的那几样，一条命令走完（不进门禁默认格，按需跑；说明见同目录 README.md「真机代测」）。
# 和冒烟（smoke.sh）是两件事：冒烟走主路十步，这里只核真机那几样——
#   ① 正式包（arm64-v8a，混淆过）经 ARM 转译在 x86_64 镜像上装 · 开 · 连上 · 进一条会话；
#   ② Doze / App Standby / 杀后台之后回前台，重连、断开期间来的话收齐；
#   ③ 模拟器控制台断网 · 限速 · 加延迟，量回来到看到新话的时间；
#   ④ 后台时来一条需手动：出通知、点通知进那一条会话。
# 用法：bash tests/mobile-smoke/device.sh
#   CCM_DEVICE_BREAK=reconnect  看台架会红：回前台前把台架 sshd（监听的与已连上的）停掉，重连必失败 ⇒ ②③ 必红。
#   CCM_DEVICE_ITEMS="1 2 3 4"  挑哪几项（默认全走；1 不走时 2–4 照样先导钥匙 · 加机器）。
#   CCM_DEVICE_APK=<路径>        用现成的正式包（不编）；默认编 assembleRelease。
# 端口与冒烟、别的几路错开（都能用 CCM_SMOKE_* 改）：sshd 22452 · 中转 28818 · adb 5167 · 模拟器 5670；台架目录 .build/mobile-device/。
# 正式包没正式签名（keystore 不在树里）⇒ 用台架目录里现造的一把测试钥匙签（apksigner），包里的字节与正式包一样，只是签名不同；
#   「用户那把签名覆盖装」只能用户手机核，见报告。
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
export CCM_SMOKE_BOX=${CCM_SMOKE_BOX:-$root/.build/mobile-device/box}
export CCM_SMOKE_PORT=${CCM_SMOKE_PORT:-22452} CCM_SMOKE_RELAY_PORT=${CCM_SMOKE_RELAY_PORT:-28818}
export CCM_SMOKE_ADB_PORT=${CCM_SMOKE_ADB_PORT:-5167} CCM_SMOKE_EMU_PORT=${CCM_SMOKE_EMU_PORT:-5670}
export CCM_SMOKE_AVD_HOME=${CCM_SMOKE_AVD_HOME:-$CCM_SMOKE_BOX/../avd}
heavy=()
[ -f "$root/tests/scripts/heavy.sh" ] && heavy=(bash "$root/tests/scripts/heavy.sh")
(cd "$root/src/backend" && "${heavy[@]}" cargo build -q)
bash "$here/rig.sh" up
bash "$here/emu.sh" up
eval "$(bash "$here/emu.sh" env)"
pkg=com.ccmonitor.mobile
apks=$root/.build/mobile/app/outputs/apk

# 镜像有没有 ARM 转译：照模拟器自己报的 ABI 表（没有 arm64-v8a ⇒ 第 1 项「没核」，2–4 改装混淆过的 x86_64 包照样走）。
abis=$(adb shell getprop ro.product.cpu.abilist | tr -d '\r')
if [[ ",$abis," == *",arm64-v8a,"* ]]; then
  arm=1
  apk=${CCM_DEVICE_APK:-}
  if [ -z "$apk" ]; then
    # shellcheck disable=SC1091
    (cd "$root/src/mobile" && . ./scripts/env.sh >/dev/null && "${heavy[@]}" ./gradlew --no-daemon -q :app:assembleRelease)
    for apk in "$apks"/release/app-release*.apk; do break; done
  fi
  # 测试钥匙：台架目录里现造，只签这一份拷贝。
  # shellcheck disable=SC1091
  . "$root/src/mobile/scripts/env.sh" >/dev/null
  ks=$CCM_SMOKE_BOX/device/test-sign.jks
  mkdir -p "$(dirname "$ks")"
  [ -f "$ks" ] || keytool -genkeypair -keystore "$ks" -storepass devtest -keypass devtest -alias dev -keyalg RSA -keysize 2048 \
    -validity 365 -dname "CN=device-rig" >/dev/null 2>&1
  signed=$CCM_SMOKE_BOX/device/app-release-testsigned.apk
  bt=$(printf '%s\n' "$ANDROID_HOME"/build-tools/* | sort -V | tail -1)
  "$bt/zipalign" -f -p 4 "$apk" "$signed.aligned"
  "$bt/apksigner" sign --ks "$ks" --ks-pass pass:devtest --ks-key-alias dev --out "$signed" "$signed.aligned"
  rm -f "$signed.aligned" "$signed.idsig"
  apk=$signed
else
  arm=0
  echo "device: 镜像报的 ABI 是「$abis」，没有 arm64-v8a（没有 ARM 转译）⇒ 第 1 项没核，2–4 用混淆过的 x86_64 包" >&2
  # shellcheck disable=SC1091
  (cd "$root/src/mobile" && . ./scripts/env.sh >/dev/null && "${heavy[@]}" ./gradlew --no-daemon -q :app:assembleMinified)
  apk=$apks/minified/app-minified.apk
fi
# 签名与冒烟那几趟不同 ⇒ 先卸再装（数据在 device.py 开头也清一遍）。
adb uninstall "$pkg" >/dev/null 2>&1 || true
adb install "$apk" >/dev/null
CCM_DEVICE_ARM=$arm CCM_DEVICE_APK="$apk" CCM_SMOKE_OUT=${CCM_SMOKE_OUT:-$CCM_SMOKE_BOX/../shots} python3 "$here/device.py"
