#!/usr/bin/env bash
# 手机冒烟主路，一条命令走完（不进门禁默认格，按需跑；说明见同目录 README.md）。
# 用法：bash tests/mobile-smoke/smoke.sh        （CCM_SMOKE_BOX 改台架目录；CCM_SMOKE_OUT 改截图目录）
# 做的事：编后端（若没编）→ rig.sh up（沙箱 sshd ＋ 后端 ＋ 替身）→ emu.sh up（模拟器）→ 编 debug 包、装上 → smoke.py 走八步。
# 重活（cargo · gradle）经 tests/scripts/heavy.sh（有就用），gradle 带 --no-daemon；模拟器自己一个封了内存的 scope。
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
export CCM_SMOKE_BOX=${CCM_SMOKE_BOX:-$root/.build/mobile-smoke/box}
heavy=()
[ -f "$root/tests/scripts/heavy.sh" ] && heavy=(bash "$root/tests/scripts/heavy.sh")
[ -x "$root/.build/backend/debug/cc-monitor-backend" ] || (cd "$root/src/backend" && "${heavy[@]}" cargo build -q)
bash "$here/rig.sh" up
bash "$here/emu.sh" up
eval "$(bash "$here/emu.sh" env)"
# shellcheck disable=SC1091
(cd "$root/src/mobile" && . ./scripts/env.sh >/dev/null && "${heavy[@]}" ./gradlew --no-daemon -q :app:assembleDebug)
adb install -r "$root/.build/mobile/app/outputs/apk/debug/app-debug.apk" >/dev/null
python3 "$here/smoke.py"
