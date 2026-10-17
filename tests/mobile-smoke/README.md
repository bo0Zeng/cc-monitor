# 手机冒烟台架

手机端在 Android 模拟器上接沙箱后端，把主路从头走一遍。**不进门禁默认格**，要看手机端整条路通不通时按需跑：

```sh
bash tests/mobile-smoke/smoke.sh
```

跑两趟（`CCM_SMOKE_VARIANTS` 可挑）：`debug`（调试包）· `minified`（混淆过的包：R8 照正式包那一套、调试签名，不碰正式 keystore）。门禁只编正式包不跑它，靠运行时按名字认的地方在混淆后坏了，只有第二趟露得出来。每趟末尾印一张表（每步过没过 · 用时 · 截图），截图与 `results.json` 在 `.build/mobile-smoke/shots/<趟>/`（`CCM_SMOKE_OUT` 可改）。

## 要什么

- Android 工具链照 `src/mobile/scripts/env.sh`（默认 `$HOME/android-dev`），另装 `emulator` 与 `system-images;android-36;google_apis;x86_64`（`sdkmanager`）。
- KVM（当前用户在 `kvm` 组）· `/usr/sbin/sshd` · `tmux` · `python3`（截屏缩图要 Pillow，没有也能跑）。

## 隔离

| 东西 | 怎么隔 |
|---|---|
| sshd | 只听 `127.0.0.1:22422`（`CCM_SMOKE_PORT`），私有 host key 与登录钥匙，只认台架那把钥匙；模拟器经它的网关（即宿主机）够到它 |
| 后端 | ssh 进来的每条命令由台架的登录壳（`rig.sh up` 写进台架目录）换上台架的 `HOME` · `TMUX_TMPDIR` · `PATH` · `CCM_SANDBOX=1` · 私有中转口；后端文件放在台架家里的 `~/.cc-monitor/bin/ccm`（本树编的那份） |
| tmux | 台架自己的 server（`TMUX_TMPDIR` 在台架目录下）；从桌面那一侧打字用 `rig.sh tmux …`（显式 `-S`） |
| agent | 不起真 claude / codex：`fake-agent.py` 装成这两个名字放在台架 `PATH` 最前，按那一家的原始记录格式写盘（pidfile · jsonl · Codex rollout），带初始提示词起、读终端里敲进来的行 |
| 模拟器 | 无头，AVD 在台架目录旁（磁盘上），adb 私有端口 5137 · 模拟器口 5590（`smoke.sh` 定的；`emu.sh` 自己不落默认口，没给 `CCM_SMOKE_ADB_PORT` · `CCM_SMOKE_EMU_PORT` 就拒）；单独一个 `systemd-run --user --scope`（MemoryHigh 6G / MemoryMax 8G） |

## 几样零件

- `rig.sh up|down|env|ssh|tmux|land`：台架本身（`land none|<文件>`：停台架里的常驻、落点删掉 / 换成那份 —— 装后端那两步用）。`ssh '<命令>'` 以台架环境跑一条（和手机进来同一个登录壳），每条进来的命令记在 `logs/commands.log`。
- `emu.sh up|down|env`：模拟器（要先给 `CCM_SMOKE_ADB_PORT` 与 `CCM_SMOKE_EMU_PORT`）。
- `frames.py <命令> '<JSON 入参>' [期限毫秒]`：经台架开一条常驻流问一句（和手机同一条路）；`frames.py watch <秒>` 只看流上的帧。
- `ui.py dump|tap|has|wait|text|key|shot`：按字找控件点、截屏。
- `fake-agent.py`：替身。句里带 `long` ⇒ 一大段回答；带 `ask` ⇒ 停在批准框上（需手动）、带 `quiz` ⇒ 停在提问框、带 `plan` ⇒ 停在批计划：起会话时垫了答题 hook ⇒ 同真的那样把请求交给它、照它给的决定往下走（决定原样落进那一步的结果），终端里先敲一行也算答（谁先算谁的）；敲进来 `/model <值>` ⇒ 只落那一家换模型的读回（第 9 步换模型用）。`CCM_SMOKE_STEP`（每条记录间隔）· `CCM_SMOKE_BOOT`（起到能收第一句之前停多久）。
- `smoke.py`：十二步（加机器连上（那台先没有后端；后跟三小步：1b 装后端（问题条［安装］⇒ 手机把包里那一格放上去、换常驻、接上）· 1c 升级后端（落点换成一份身份早一格的旧版 ⇒ 说要更新 ⇒［更新］⇒ 换常驻、不再说要更新）· 1d 主机设置里核对版本（同版不换常驻））· 对话列表（台架只有一台：一行两层，停在批准框上那条第二层是需手动；几台并一列没在模拟器上跑过）· 新建会话（目录 chip → 浏览… → 选这个目录）· 聊天屏（后跟两小步：输入框下的模型 chip · ［+］面板的命令 / 清空先确认 / 压缩；再在聊天里答四样：钉条［答］升抽屉答批准 · 答提问（一题一屏）· 批计划 · 抽屉开着时终端先答了 ⇒ 别处先答了）· 断网再连 · 换网 · 杀进程再开 · 断着网重开（列表与那一条的最后一屏照本地缓存画，网回来跟上新的） · 终端屏（「用哪个启动」那一排点台架那台配的别名起会话 · 接进去两侧打字 · 命令面板换模型）· Codex（新建会话的 agent chip 选它）· 文件页（列目录 · 进一层 · 预览 · 新建 · 改名 · 删除 · 下载 · 上传，结果在台架盘上核）· 轮换屏（台架里种三个号与两份策略、起一条带轮换的会话；聊天屏「⋯」进 → 换策略 → 长按号名拖到最上那条成临时条目 → 改一格 → 存，再从设置 → 机器与账号 → 轮换进一次）· 长按改标题（列表那一行换字、台架那一行 customTitle 是新字，再改回空）），每步只看屏上和台架记录里看得到的。 `CCM_SMOKE_ONLY=files` 只走加机器 ＋ 文件页；`CCM_SMOKE_ONLY=deploy` 只走加机器 ＋ 装后端那几步（印放字节那一段用了几秒）。
- 装后端要包里带着字节：开发树先 `bash tests/scripts/re-embed.sh`（铺 `src/frontend/shell/embedded-backends/`，桌面同一个目录），没铺 `smoke.sh` 当场停、说一声。

收：`CCM_SMOKE_ADB_PORT=5137 CCM_SMOKE_EMU_PORT=5590 bash tests/mobile-smoke/emu.sh down; bash tests/mobile-smoke/rig.sh down`（台架目录留着，删掉 `.build/mobile-smoke/` 即全清）。

## 真机代测（`device.sh`）

真机上才看得出的几样，能在模拟器上代的都挪到这里，和冒烟那十步分开跑（冒烟走主路，这里只核真机那几样）：

```sh
bash tests/mobile-smoke/device.sh                         # 四项全走
CCM_DEVICE_ITEMS="2 3" bash tests/mobile-smoke/device.sh  # 挑几项
CCM_DEVICE_BREAK=reconnect bash tests/mobile-smoke/device.sh  # 看台架会红：回前台 / 网回来前把台架 sshd 停掉，2 · 3 必红
```

| 项 | 做什么 | 判 |
|---|---|---|
| 1 正式包经 ARM 转译 | 编 `assembleRelease`（arm64-v8a、R8 混淆）、用台架目录里现造的测试钥匙签（正式 keystore 不在树里），装进 x86_64 镜像；导钥匙 · 加机器 · 新建会话 | 系统报包的 ABI 是 arm64-v8a · 进程里有 `libndk_translation` · arm64 的 `libmobcore.so` 映进来了 · 新会话答回。镜像 ABI 表里没有 arm64-v8a ⇒ 「没核」，2–3 改装混淆过的 x86_64 包 |
| 2 Doze / Standby / 杀后台 | 退后台 → `deviceidle force-idle`（先 `battery unplug`、灭屏）· `set-standby-bucket rare` · `am kill` · `am force-stop` → 桌面那侧隔 4 秒来三句、再等 20 秒 → 回前台 | 回前台后 60 秒（`CCM_DEVICE_WITHIN`）内三句的回答都在。量「回前台 → 重连」（`commands.log` 里第一条新进来的命令）与「回前台 → 收齐」 |
| 3 网 | 先成对量模拟器控制台 `network delay` · `network speed` · `gsm data off` 管不管用；再在模拟器里（只动模拟器自己，不碰宿主网络；弱网台架 `tests/e2e/weak-net/` 是两个容器之间的，模拟器放不进去，这里借它「改前 / 改后成对读数」的规矩，造不出就红）：`tc netem` 造慢网（400±100ms · 64kbit，只管出方向），`iptables` 两个方向丢掉与宿主之间的包造静默断网（网卡还在、连接收不到 RST）15 秒 · 90 秒 | 慢网下回答上屏；断网期间来的那句在网回来 60 秒内上屏。量「网回来 → 重连」「网回来 → 看到」 |

每项末尾那一屏亮暗各一张（`<项>-亮.png` · `<项>-暗.png`）、量的数与每项结果在 `.build/mobile-device/shots/device-results.json`（`CCM_SMOKE_OUT` 可改）。

端口与冒烟、别的几路错开：sshd `22452` · 中转 `28818` · adb `5167` · 模拟器 `5670`（`CCM_SMOKE_PORT` · `CCM_SMOKE_RELAY_PORT` · `CCM_SMOKE_ADB_PORT` · `CCM_SMOKE_EMU_PORT` 可改），台架目录 `.build/mobile-device/`（AVD 也在它下面，和冒烟那台互不相干）。收：先 `eval` 同样那几格再 `emu.sh down; rig.sh down`。

只有用户那台手机核得了的（这里不代）：厂商 ROM 自己的杀后台 · 真 Wi-Fi ↔ 蜂窝切换 · 用户的输入法 · 用户那把签名覆盖装。
