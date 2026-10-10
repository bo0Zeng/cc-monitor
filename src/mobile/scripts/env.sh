# shellcheck shell=bash
# env.sh — 激活隔离的 Android 工具链(只影响当前 shell)。
# 用法:      . ./scripts/env.sh
# 换位置:    ATERM_ANDROID_DEV=/d/android-dev . ./scripts/env.sh
#
# 未指定 ATERM_ANDROID_DEV 时按序探测: $HOME/android-dev(Linux/macOS)、
# /e/android-dev(Git Bash 挂载的 E:\,= bootstrap-android-env.ps1 的默认位置)。
#
# 全局 JAVA_HOME 不受影响,退出 shell 即复原。
# 校验在任何 export 之前完成 —— 找不到工具链就报错返回,绝不把 shell 指向无效目录。

# 必须 source:直接执行的话变量随子进程消失,是无声空操作,比报错更坏。
# 只在 bash 下认得出来(BASH_SOURCE 是 bash 专有);dash/zsh 执行时抓不到,
# 但那个方向是安全的 —— 漏抓只是让人困惑,不会错杀调用者的 shell。
if [ -n "${BASH_SOURCE:-}" ] && [ "${BASH_SOURCE}" = "$0" ]; then
  echo "env.sh 要 source 才有用(直接执行的话变量随子进程消失):" >&2
  echo "    . ./scripts/env.sh" >&2
  exit 1
fi

# 可用的工具链根 = jdk 与 sdk 都在。只有 JDK 没 SDK 撑不起 Android 构建。
_aterm_root_ok() {
  [ -d "$1/sdk" ] || return 1
  [ -x "$1/jdk/bin/java" ] || [ -x "$1/jdk/bin/java.exe" ]  # java.exe: Git Bash
}

_root=""
if [ -n "${ATERM_ANDROID_DEV:-}" ]; then
  # 显式指定就只认它、不回退 —— 打错字必须响,静默换个地方会让人以为自己指定生效了。
  if _aterm_root_ok "$ATERM_ANDROID_DEV"; then
    _root="$ATERM_ANDROID_DEV"
  else
    echo "env.sh: ATERM_ANDROID_DEV=$ATERM_ANDROID_DEV 不是可用的工具链根。" >&2
    echo "        要求 <根>/jdk/bin/java 可执行,且 <根>/sdk 是目录。" >&2
    echo "        环境未改动。" >&2
  fi
else
  for _cand in "${HOME:-}/android-dev" /e/android-dev; do
    if _aterm_root_ok "$_cand"; then
      _root="$_cand"
      break
    fi
  done
  if [ -z "$_root" ]; then
    echo "env.sh: 没找到 Android 工具链(已试 ${HOME:-}/android-dev 和 /e/android-dev)。" >&2
    echo "        指定位置: ATERM_ANDROID_DEV=/path/to/android-dev . ./scripts/env.sh" >&2
    echo "        装一套:   scripts/bootstrap-android-env.ps1(PowerShell)" >&2
    echo "        环境未改动。" >&2
  fi
fi

if [ -z "$_root" ]; then
  unset -f _aterm_root_ok
  unset _root _cand
  return 1
fi

export JAVA_HOME="$_root/jdk"
export ANDROID_HOME="$_root/sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export ANDROID_USER_HOME="$_root/android-user"   # 不写 ~/.android
export ANDROID_AVD_HOME="$ANDROID_USER_HOME/avd"  # avdmanager 与 emulator 对齐 AVD 目录
# Gradle 缓存也隔离: 不写 ~/.gradle,整套工具链删目录即净。
export GRADLE_USER_HOME="$_root/gradle-home"

export PATH="$JAVA_HOME/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/cmdline-tools/latest/bin:$PATH"

echo "Android 工具链已激活(仅本 shell):"
echo "  JAVA_HOME    = $JAVA_HOME"
echo "  ANDROID_HOME = $ANDROID_HOME"

unset -f _aterm_root_ok
unset _root _cand
