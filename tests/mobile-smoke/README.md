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
| 模拟器 | 无头，AVD 在台架目录旁（磁盘上），adb 私有端口 5137；单独一个 `systemd-run --user --scope`（MemoryHigh 6G / MemoryMax 8G） |

## 几样零件

- `rig.sh up|down|env|ssh|tmux`：台架本身。`ssh '<命令>'` 以台架环境跑一条（和手机进来同一个登录壳），每条进来的命令记在 `logs/commands.log`。
- `emu.sh up|down|env`：模拟器。
- `frames.py <命令> '<JSON 入参>' [期限毫秒]`：经台架开一条常驻流问一句（和手机同一条路）；`frames.py watch <秒>` 只看流上的帧。
- `ui.py dump|tap|has|wait|text|key|shot`：按字找控件点、截屏。
- `fake-agent.py`：替身。句里带 `long` ⇒ 一大段回答；带 `ask` ⇒ 停在批准框上（需手动），终端里再敲一行才往下走；敲进来 `/model <值>` ⇒ 只落那一家换模型的读回（第 9 步换模型用）。`CCM_SMOKE_STEP`（每条记录间隔）· `CCM_SMOKE_BOOT`（起到能收第一句之前停多久）。
- `smoke.py`：十步（加机器连上 · 对话列表 · 新建会话 · 聊天屏 · 完成通知 · 断网再连 · 换网 · 杀进程再开 · 终端屏（含命令面板换模型）· Codex），每步只看屏上和台架记录里看得到的。

收：`bash tests/mobile-smoke/emu.sh down; bash tests/mobile-smoke/rig.sh down`（台架目录留着，删掉 `.build/mobile-smoke/` 即全清）。
