# scripts

## env.sh / env.ps1 — 在当前 shell 里激活隔离的 Android 工具链

```bash
. ./scripts/env.sh                             # Linux / macOS / Git Bash
```
```powershell
. .\scripts\env.ps1                            # PowerShell
```

设当前 shell 的 `JAVA_HOME` / `ANDROID_HOME` / `ANDROID_SDK_ROOT` / `PATH`，并把 `ANDROID_USER_HOME` 指进工具链目录（不写 `~/.android`）。`env.sh` 还把 `GRADLE_USER_HOME` 指进工具链目录。退出 shell 一切复原，全局 `JAVA_HOME` 不动。

工具链根目录：设了 `ATERM_ANDROID_DEV` 就只认它；没设时 `env.sh` 按序探测 `$HOME/android-dev`、`/e/android-dev`（Git Bash 下的 `E:\`），取第一个 `jdk` 与 `sdk` 都在的；`env.ps1` 默认 `E:\android-dev`。找不到可用的根就报错返回非零，一个变量都不改。

`env.sh` 必须 `source`；直接 `bash scripts/env.sh` 会提示用法并退出。

## bootstrap-android-env.ps1 — 装一套隔离的 Android 工具链（Windows）

```powershell
.\scripts\bootstrap-android-env.ps1            # 默认装到 E:\android-dev
.\scripts\bootstrap-android-env.ps1 -Root D:\android-dev
```

装 Temurin JDK 21 与 Android SDK（cmdline-tools、platform-tools、`platforms;android-36`、`build-tools;36.0.0`）。不装 NDK / CMake：termlib 用仓内 `libs/m2/` 里的包，只有改 termlib 原生代码时才要。幂等，已存在的组件跳过。

## gate-count.sh — 门禁条数

```bash
. ./scripts/env.sh && ./gradlew ktlintCheck testDebugUnitTest :core-claude:test :core-remote:test
bash scripts/gate-count.sh
```

只读仓根 `.build/mobile/<模块>/test-results/` 里已有的结果（构建输出不落在 `src/mobile` 里）：逐目录打印条数、失败数、最后写入时间，再求和；有失败退出码非零。它只在刚跑完全量门禁时准，用 `--tests` 过滤跑过之后数会偏小。
