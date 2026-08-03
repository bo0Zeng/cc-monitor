# U8a-2c — daemon `launch` 的第一条生产调用（`send-into` 那一格）

- 工作区：unified-backend · 第六梯队 · 任务 #96
- 前置：U8a-2b（daemon 执行面）· U6b（入方向通道）· P4a/P4b（monitor 侧 `backend/control/`）
- 本件性质：**生产切换**。切的是活的远端主路，本机没有真远端 —— 风险处置见 §三。

## 一、摸底：旧阻塞已解，新刀口在 `send-into`

此前登记的阻塞是「tauri 命令收到的是**渲染好的 shell 串**，拆不回结构化计划」。
**P4a/P4b 之后这条不成立了** —— `backend/control/` 里现在就收结构化请求。

但直接切「远端起会话主路」仍然不行，摸底发现一个更硬的前提问题：
装了 ccm 的机器上那条串是 `ccm resume … --tmux=<name>`，**ccm 自己还做**预信任等待 /
标题 rbind / `CC_BUS_ID` / 身份轮询（S10 的契约）。让 daemon 建会话并键入裸载荷，
那批行为**谁做哪一半没有定论** ⇒ 那一刀今天不该切。

⇒ 找到唯一一处**天然两半分开**的地方：`session-backend.ts` 的 `send-into` 那一格，
今天产的串逐字是

```text
tmux send-keys -t '=name:' '<载荷>' Enter; tmux attach -t '=name:'
```

| 半边 | 归谁 | 理由 |
|---|---|---|
| `send-keys` | **daemon `launch{mode:"send-into"}`** | 逐字对应，daemon 侧已有真进程 + 真 tmux 的 e2e |
| `attach` | **用户自己的终端** | §1.3：pid 要等于 pidfile 名、tty/Ctrl-C 要落在 agent 上、`tmux attach` 要占住调用方终端 |

而且这一格**完全不经 ccm** —— CLI 渲染器对它恒返回 `Refusal::SendIntoHasNoCliForm`
（#76 防线）⇒ **没有 ccm 契约冲突**。

⚠ **`create-or-attach` 那一格刻意不做**：它靠 shell 的
「`new-session -d 2>/dev/null &&` 建失败被吞 ⇒ 短路跳过 send-keys」实现幂等，
而 daemon 的 `created`/`typed` 两个布尔与那个技巧**不逐字等价**。要切先对拍两种幂等语义 ⇒ 另立。

### ★ 仓里的守卫把我原定的分刀否掉了

我原打算按本仓「先建判据后切生产」的先例分成两轮（先通道、后前端）。
加完 Rust 命令一跑，`commands.vitest.ts` 那条
「**Rust 声明的命令必须在 TS 里静态可见**」当场红，报
`expected [ 'daemon_send_into' ] to deeply equal []`。

⇒ 它说得对：**没有调用方的 IPC 命令就是死接口面**。把它加进白名单等于「为实现让路去改判据」
（审计点名过的形状）。于是本轮把前端一起切。**这是守卫改了我的计划，不是我改了守卫。**

## 二、做了什么

- **`backend/control/daemon_launch.rs`**（新）：`daemon_send_into` tauri 命令。
  `mode` 写死 `send-into`；拿不到控制通道 / daemon 回报未键入 ⇒ **tagged 返回**
  （`typed:false` + `reason`），不是 `Err` —— 那是诚实降级，调用方要拿着理由回落。
- **`inbound_client::launch_args` 的 `#[allow(dead_code)]` 摘掉了** —— 它终于有生产调用方。
  那个属性本身就是复盘点名的「方向偏移」的物证：编码器早写好、零调用方。
- **`remote-launch-run.ts::runRemoteResumeIntoExistingTmux`**：先试 daemon，
  成功 ⇒ 终端那条命令改成 `planAttach(name)` 的产物（**只 attach**）；
  任何一步不顺 ⇒ 原样回落到今天那条整串。
- 账本 `parity_ledger`：新增 `("daemon_send_into", "launch.send-into", Side::Remote)`，
  不对称性质记 **`Undecided`**（不是 `NaturallyAsymmetric`）——
  本机今天没有对应能力，但那不是「本机不需要」，而是**本机该不该也有一个后端进程还没裁定**
  （§1.2 的 v1 三宿主被否决、U12 的 daemonless 未定）。写成天然不对称就是替产品做主。

## 三、核心风险的处置：本机没有真远端

不靠「小心点」，靠**两层判据**：

### ① 真进程 + 真 tmux 的跨轨对拍（e2e，27 → 28 条）

`inbound-daemon-frames.sh` 新增一条场景：真建一个 tmux 会话，把
**monitor 编码器产的那一行**（`INBOUND_SEND_INTO_LINE`，值全写死不插值）喂给真 daemon，
断言 `ok:true` + `typed:true`。monitor 侧 `the_e2e_send_into_line_is_exactly_what_the_encoder_produces`
逐字节钉住那条字面量。

**为什么非要这条**：脚本里原有的 `e2e-launch-3` 是手写+插值的 ——
它只证明「daemon 认得我写的形状」，证明不了「**monitor 真正会发的形状**」。
`launch_args` 的键名一改，e2e 会继续全绿而生产里一条命令都发不出去。
**变异复验**：把 `payload` 键名改成 `body` ⇒ 那条钉当场红。

### ② 前端两个分支各有判据（`send-into-daemon.vitest.ts`，4 条）

daemon 说键入了 ⇒ 终端串**只 attach**（还带 send-keys 就是把载荷键两遍）·
说没键入/IPC 抛了 ⇒ **逐字回落**到今天那条整串 ·
发出去的是 `render_launch_payload` 的产物 + **裸会话名**（`=name:` 由 daemon 侧加，
两侧各加一次会变成 `==name::`）· 载荷必须先渲染再发。

### ③ 回落即今天 ⇒ 没有 daemon 的远端上是零影响

拿不到控制通道时走的就是今天那条整串。**这是本件敢切活路的根本原因。**

## 四、变异复验（七条，全红）

| 变异 | 结果 |
|---|---|
| M1 daemon 说没键入也当成键入了（乐观解读） | **红 2 条** |
| M2 daemon 成功后终端仍走整串（载荷键两遍） | **红** |
| M3 发出去的会话名多包一层 `=name:` | **红** |
| M4 载荷不发 `render_launch_payload` 的产物、自己拼一个 | **红** |
| M5 Rust 侧 `mode` 改成 `create-or-attach`（会新建会话 = #76） | **红** |
| M6 Rust 侧 `typed` 缺字段当 `false`（把协议漂移报成「会话不存在」） | **红** |
| M7 Rust 侧没有通道也报 `typed:true` | **红** |
| （跨轨）`launch_args` 的 `payload` 键改名 | **红** |

## 五、诚实边界

- **一处 fail-open，登记在案**：载荷渲染被 Rust 拒（非法 configDir / 会裂的 arg）时也回落，
  而兜底渲染器（TS）对同样输入**未必拒**。那不是本件引入的 —— 这一格今天**根本不经 Rust 渲染**
  （`renderLaunchCommand` 对 send-into 恒走 `renderFallback`），所以「回落 = 今天」才是
  零行为变化的那个选择。要收成 fail-closed 得连兜底渲染器一起收 ⇒ U8c-3。
- **真远端仍未验**：本机验的是「monitor 编码器 ↔ 真 daemon ↔ 真 tmux」这一段，
  **SSH 那一跳没有验**（长连接握手后 `client_for(origin)` 才有值）。
  真机路径要等用户在真远端上跑一次 —— 登记，不假装做完了。
- **`create-or-attach` 与远端起会话主路都没切**（见 §一）。

## 六、门禁

monitor **783** · shell-quote-core 1 · daemon 237（跨 target Windows check 0 error）·
vitest **85 文件 1183 例** · tsc 0 · fmt clean · e2e 17 套全绿（**inbound 地板 27 → 28**，
CI 里两处一起提）· clippy 去行号 46 == 46 零新增。

## 签收

- [x] 过代码审计（D）—— 七条变异 + 一条跨轨变异全红；e2e 那条场景第一次跑就抓到我自己
      建会话那行写错（`=name:` 是 target 形态、`-s` 收裸名）
- [x] 过工程审计（E）—— 摸底把刀口从「远端起会话主路」改到 `send-into`（ccm 契约冲突的前提问题
      落档）；账本记 `Undecided` 而不是替产品裁定；fail-open 与「真远端未验」如实登记
- [x] 主计划已更新（F）
