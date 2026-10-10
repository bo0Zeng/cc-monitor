# cc-monitor 手机端（Android）

Claude Code 的手机前端：长得像 Claude app 的聊天，背后是用户自己服务器上的 Claude Code。不做云同步。

它是 cc-monitor 的一个**输出适配层**（出口）：判定与写字都在后端核心，手机端只挑格、排版、装运（见 [`../doc/ARCHITECTURE.md`](../doc/ARCHITECTURE.md)「核心与适配层」一节）。它经 SSH 连服务器上那份常驻后端（`~/.cc-monitor/bin/ccm`）：先 `--backend-probe` 核同一个 `BUILD_ID`、`--resident-ensure` 起常驻，再开一条长连接 `--resident-attach`（双向帧协议，和桌面连远端同一条路），一问一答与会话帧都走它；与桌面端共用同一份后端，一台服务器装一次。

当前版本 0.6.1（versionCode 11）。包名与 `applicationId` 是 `com.ccmonitor.mobile`（与桌面端的 `com.ccmonitor.app` 同一个根）。

## 模块

| 模块 | 管什么 |
|---|---|
| `app` | 屏与导航、注入、前台保活、拨号网关与 tmux 网关、起路 A |
| `core-claude` | `link/`：常驻流的帧客户端（门槛 · 起常驻 · attach · 一问一答 · 格目录 · 断了自己接回去）与核心成品的解码（会话表 · 需手动 · `history-list` · `quota-read` · `turn_end` 折成一轮一条）；其余是还没删的路 A 与自读原文那一族 |
| `core-remote` | 中立的远端抽象：一次性 exec · 流式 exec · 双向长 exec（`RemoteDuplex`）· shell 引号 |
| `core-ssh` | SSH（sshj）：连接池、多地址竞速、跳板、SFTP、`~/.ssh/config`、known_hosts、密钥 |
| `core-data` | Room 库 |
| `core-terminal` | 终端引擎（termlib） |
| `core-ui` | 共用主题与反馈 · 取文口 `copyText`（构建时从 `src/shared/copy/table.json` 抄手机代码引到的那几条） |
| `bridge/` | 路 A 上下行的线协议（`PROTOCOL.md`）与金样（`vectors/`） |

## 技术栈

Jetpack Compose ＋ Material3 · Koin · Room（KSP）· termlib（libvterm，JNI）· sshj · Markwon ＋ Prism4j · Tink ＋ Android Keystore。Kotlin 2.3 · AGP 8.13 · Gradle 8.14 · JDK 21 · compileSdk 36 · minSdk 26。

termlib `0.1.1` 已放进仓内 maven 仓 `libs/m2/`，构建不需要 NDK。那份 aar 是上游打了补丁编的，补丁与重编步骤在 [`libs/termlib-patches/`](libs/termlib-patches/README.md)；改 termlib 原生代码要照那里另外构建，再更新 `libs/m2/`。

## 构建

要 JDK 21 与 Android SDK（`platforms;android-36`、`build-tools;36.0.0`）；不要 NDK。`scripts/env.sh` 在当前 shell 里激活一套隔离的工具链（默认 `$HOME/android-dev`，Gradle 缓存也放那里；Windows 用 `scripts/env.ps1`，装一套用 `scripts/bootstrap-android-env.ps1`，见 [`scripts/README.md`](scripts/README.md)）。

```sh
cd src/mobile
. ./scripts/env.sh
./gradlew :app:assembleDebug      # debug 包
./gradlew :app:assembleRelease    # release（R8）；签名读 gitignored 的 keystore.properties，没有就产出未签名的包
```

构建输出不落在 `src/mobile` 里：各模块的 `build/` 在仓根 `.build/mobile/<模块>/`，Gradle 的项目缓存在 `.build/mobile/.gradle/`（仓里另有几族判据走磁盘扫 `src/`，产物落在源码树里会把它们的人群淹掉）。

## 测试

```sh
./gradlew ktlintCheck detektDebug :app:lintDebug testDebugUnitTest :core-claude:test :core-claude:detektMain :core-remote:test :core-remote:detektMain
python3 -m pytest bridge/tests        # bridge 的 Python 金样测试
bash scripts/gate-count.sh            # 刚跑完上面那条之后看条数（读 .build/mobile/*/test-results）
```

`:core-claude`、`:core-remote` 是纯 JVM 模块，`testDebugUnitTest` 不覆盖，要显式带上。

- **门禁**：仓根 `tests/scripts/gate.sh` 的 `mobile` 格跑的就是上面这几条 ＋ `:app:assembleRelease`（CI 的 `mobile` job 调同一格，Linux 上跑，不起模拟器）。
- **要设备的 instrumentation 测（androidTest）不进门禁、也不进 CI**：接好模拟器或真机后跑 `bash scripts/android-test.sh`。其中 `SshConnectionTest` 要一台真 SSH 服务端，入口脚本不跑它，手测时用 `-Pandroid.testInstrumentationRunnerArguments.sshHost=<地址>` 给地址。
- **Android Lint**（`:app:lintDebug`）在上面那条里：error 一条就红，warning 不拦。

## License

MIT，与仓根 `LICENSE` 相同。
