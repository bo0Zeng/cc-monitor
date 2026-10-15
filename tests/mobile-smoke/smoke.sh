#!/usr/bin/env bash
# 手机冒烟主路，一条命令走完（不进门禁默认格，按需跑；说明见同目录 README.md）。
# 用法：bash tests/mobile-smoke/smoke.sh        （CCM_SMOKE_BOX 改台架目录；CCM_SMOKE_OUT 改截图目录；CCM_SMOKE_VARIANTS 挑哪几趟，默认 `debug minified`）
# 做的事：编后端（每趟都编：增量，没改就秒过；只在缺二进制时才编会拿旧后端去接新手机包，版本门照「后端要更新」接、对不上的命令不发）→ rig.sh up（沙箱 sshd ＋ 后端 ＋ 替身）→ emu.sh up（模拟器）
#   → 每一趟：编那个包、装上、smoke.py 走十一步。两趟：debug（调试包）· minified（混淆过的包：R8 照正式包、调试签名，不碰正式 keystore）——
#   门禁只编正式包不跑它，靠运行时按名字认的地方被混淆弄坏了，只有这一趟露得出来。截图分趟放（`<截图目录>/<趟>/`）。
# 重活（cargo · gradle）经 tests/scripts/heavy.sh（有就用），gradle 带 --no-daemon；模拟器自己一个封了内存的 scope。
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
export CCM_SMOKE_BOX=${CCM_SMOKE_BOX:-$root/.build/mobile-smoke/box}
# 冒烟这一台的 adb · 模拟器口（与 device.sh · 门禁那一格错开；emu.sh 自己不落默认口）。
export CCM_SMOKE_ADB_PORT=${CCM_SMOKE_ADB_PORT:-5137} CCM_SMOKE_EMU_PORT=${CCM_SMOKE_EMU_PORT:-5590}
heavy=()
[ -f "$root/tests/scripts/heavy.sh" ] && heavy=(bash "$root/tests/scripts/heavy.sh")
(cd "$root/src/backend" && "${heavy[@]}" cargo build -q)
bash "$here/rig.sh" up
bash "$here/emu.sh" up
eval "$(bash "$here/emu.sh" env)"
out=${CCM_SMOKE_OUT:-$CCM_SMOKE_BOX/../shots}
red=()
for variant in ${CCM_SMOKE_VARIANTS:-debug minified}; do
  cap=${variant^}
  # shellcheck disable=SC1091
  (cd "$root/src/mobile" && . ./scripts/env.sh >/dev/null && "${heavy[@]}" ./gradlew --no-daemon -q ":app:assemble$cap")
  # 两趟同一个包名、同一把调试签名：覆盖装（清数据在 smoke.py 开头）。
  adb install -r "$root/.build/mobile/app/outputs/apk/$variant/app-$variant.apk" >/dev/null
  echo "== $variant =="
  CCM_SMOKE_OUT="$out/$variant" python3 "$here/smoke.py" || red+=("$variant")
done
[ ${#red[@]} -eq 0 ] || { echo "冒烟没过：${red[*]}" >&2; exit 1; }
