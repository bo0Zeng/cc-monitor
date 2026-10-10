#requires -Version 5.1
<#
.SYNOPSIS
  在隔离目录里装好一套自包含的 Android 构建工具链(不污染全局)。
.DESCRIPTION
  装到 -Root(默认 E:\android-dev):
    <Root>\jdk\   ← Temurin JDK 21(给 AGP / Gradle 用;全局 JAVA_HOME 维持不动)
    <Root>\sdk\   ← Android SDK(cmdline-tools + platform-tools + platform + build-tools)
  NDK / CMake 不装:termlib 用仓内 libs/m2/ 里的包,只有改 termlib 原生代码时才要。
  幂等:已存在的组件跳过。激活用 scripts\env.ps1(只改当前 shell)。
.PARAMETER Root
  工具链根目录。默认 E:\android-dev。
.NOTES
  cmdline-tools 的 build 号会变,下载地址在 -CmdlineToolsUrl 里换。
#>
[CmdletBinding()]
param(
  [string]$Root           = 'E:\android-dev',
  [string]$JdkUrl         = 'https://api.adoptium.net/v3/binary/latest/21/ga/windows/x64/jdk/hotspot/normal/eclipse?project=jdk',
  [string]$CmdlineToolsUrl= 'https://dl.google.com/android/repository/commandlinetools-win-14742923_latest.zip',
  [string]$Platform       = 'android-36',
  [string]$BuildTools     = '36.0.0'
)
$ErrorActionPreference = 'Stop'
$ProgressPreference     = 'SilentlyContinue'   # 大文件下载不渲染进度条,快很多

$Sdk = Join-Path $Root 'sdk'
$Jdk = Join-Path $Root 'jdk'

function Get-AndFlatten {
  # 下载 zip,解压,把"唯一顶层目录"的内容提到 $Dest(处理 jdk-21.x\ / cmdline-tools\ 这种嵌套)
  param([string]$Url, [string]$Dest, [string]$TmpName)
  $zip = Join-Path $env:TEMP "$TmpName.zip"
  $ext = Join-Path $env:TEMP "$TmpName.ext"
  Write-Host "  下载 $Url"
  Invoke-WebRequest -Uri $Url -OutFile $zip
  if (Test-Path $ext) { Remove-Item $ext -Recurse -Force }
  Write-Host "  解压…"
  Expand-Archive -Path $zip -DestinationPath $ext -Force
  $top = Get-ChildItem $ext -Directory
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Dest) | Out-Null
  if ($top.Count -eq 1) {
    Move-Item $top[0].FullName $Dest
  } else {
    New-Item -ItemType Directory -Force -Path $Dest | Out-Null
    Move-Item (Join-Path $ext '*') $Dest
  }
  Remove-Item $zip,$ext -Recurse -Force -ErrorAction SilentlyContinue
}

New-Item -ItemType Directory -Force -Path $Root | Out-Null
Write-Host "=== 工具链根目录: $Root ===`n"

# --- 1) JDK 21 ---
if (Test-Path (Join-Path $Jdk 'bin\java.exe')) {
  Write-Host "[skip] JDK 已存在: $Jdk"
} else {
  Write-Host "[1/3] 装 JDK 21 -> $Jdk"
  Get-AndFlatten -Url $JdkUrl -Dest $Jdk -TmpName 'aterm-jdk21'
}

# --- 2) cmdline-tools ---
$Clt = Join-Path $Sdk 'cmdline-tools\latest'
if (Test-Path (Join-Path $Clt 'bin\sdkmanager.bat')) {
  Write-Host "[skip] cmdline-tools 已存在: $Clt"
} else {
  Write-Host "[2/3] 装 cmdline-tools -> $Clt"
  Get-AndFlatten -Url $CmdlineToolsUrl -Dest $Clt -TmpName 'aterm-cmdtools'
}

# --- 3) SDK 包(用隔离 JDK 跑 sdkmanager)---
$env:JAVA_HOME = $Jdk
$env:PATH = "$Jdk\bin;$env:PATH"
$sdkmanager = Join-Path $Clt 'bin\sdkmanager.bat'

Write-Host "`n[3/3] 接受 license + 安装 SDK 包(platform-tools, platforms;$Platform, build-tools;$BuildTools)"
$yes = ((1..50 | ForEach-Object { 'y' }) -join "`n")
$yes | & $sdkmanager --sdk_root="$Sdk" --licenses | Out-Null
& $sdkmanager --sdk_root="$Sdk" "platform-tools" "platforms;$Platform" "build-tools;$BuildTools"
if ($LASTEXITCODE -ne 0) { throw "sdkmanager 安装失败(exit $LASTEXITCODE)" }

Write-Host "`n=== 完成。验证: ==="
& (Join-Path $Jdk 'bin\java.exe') -version
Write-Host "platform-tools: $(Test-Path (Join-Path $Sdk 'platform-tools\adb.exe'))"
Write-Host "platform:       $(Test-Path (Join-Path $Sdk "platforms\$Platform"))"
Write-Host "build-tools:    $(Test-Path (Join-Path $Sdk "build-tools\$BuildTools"))"
Write-Host "`n激活(每个新 shell):  . .\scripts\env.ps1"
