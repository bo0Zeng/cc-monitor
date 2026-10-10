<#
  env.ps1 — 激活隔离的 Android 工具链(只影响当前 shell,不写任何系统/用户环境变量)。

  用法(每个新 PowerShell 会话):
      . .\scripts\env.ps1
  换工具链位置:
      $env:ATERM_ANDROID_DEV = 'D:\android-dev'; . .\scripts\env.ps1

  全局 JAVA_HOME 不受影响 —— 退出这个 shell 一切复原。
#>
$root = if ($env:ATERM_ANDROID_DEV) { $env:ATERM_ANDROID_DEV } else { 'E:\android-dev' }

$env:JAVA_HOME        = Join-Path $root 'jdk'
$env:ANDROID_HOME     = Join-Path $root 'sdk'
$env:ANDROID_SDK_ROOT = $env:ANDROID_HOME
# 让 AVD / adb key / Studio 的机器级状态也落在隔离目录,不写 ~/.android:
$env:ANDROID_USER_HOME = Join-Path $root 'android-user'
# 让 avdmanager 与 emulator 对同一 AVD 目录达成一致（否则 emulator 找不到 avdmanager 建的 AVD）
$env:ANDROID_AVD_HOME = Join-Path $env:ANDROID_USER_HOME 'avd'
# 想连 Gradle 依赖缓存也隔离(不写 ~/.gradle),取消下一行注释:
# $env:GRADLE_USER_HOME = Join-Path $root 'gradle-home'

$env:PATH = @(
  (Join-Path $env:JAVA_HOME 'bin'),
  (Join-Path $env:ANDROID_HOME 'platform-tools'),
  (Join-Path $env:ANDROID_HOME 'cmdline-tools\latest\bin'),
  $env:PATH
) -join ';'

Write-Host "Android 工具链已激活(仅本 shell):"
Write-Host "  JAVA_HOME        = $env:JAVA_HOME"
Write-Host "  ANDROID_HOME     = $env:ANDROID_HOME"
Write-Host "  ANDROID_USER_HOME= $env:ANDROID_USER_HOME"
