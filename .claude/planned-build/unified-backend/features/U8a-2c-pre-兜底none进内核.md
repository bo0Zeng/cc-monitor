# U8a-2c-pre — 摸底 U8a-2c（结论：可达，只是没接）+ 兜底 `container:"none"` 切到 Rust

- 工作区：unified-backend · 第六梯队 · 任务 #96
- 本件性质：**一次判定订正 + 一处生产切换**。

## 一、U8a-2c 摸底：它**不是不可达，是还没接**（并订正我自己中途的一次误判）

| 断言 | 实测 |
|---|---|
| `ssh_source.rs` 断言 `!accepts("launch")` ⇒ daemon 不支持 launch？ | **误读。** 那是一条**桩了 hello 只声明 `["ping"]`** 的测试，验的是「客户端的 accepts 如实反映 daemon 说了什么」。真 daemon 的 `COMMANDS` 是 `["cancel","launch","ping","resolve"]` |
| `stream_loop`（长连接）接不接入方向客户端？ | **接。** `ssh_source.rs:2833` 切分、`:2965` attach 并登记进注册表 |
| `client_for()` 有生产消费方吗？ | **没有** —— 全是测试。注册表被填了，但**还没有人去取** |

⚠ **我中途报过一次假阻塞**：`grep … | head -8` 把输出截断，于是我看到「`attach_inbound_client`
只在测试里出现」，差点把「入方向在生产里不可达」写进计划。**是我自己复测时发现的**（守卫的
计数自检 `require(2, "stream_loop 的长连接 + probe_daemon 的一次性探测")` 与我的结论矛盾，
那句矛盾把我拉了回来）。⇒ **`head` 截断也会造假证据**，量东西时别带它。

⇒ **U8a-2c 可做**，形态是两步：① daemon `launch`（建会话 + 送键，幂等闸已在 U8a-2b 建好）
② monitor 开窗跑 `ssh -t <origin> tmux attach -t '=name:'`（平面 ③，daemon 永远做不了）。
`createRunAttach` 今天那条串的**幂等性在拆成两步后仍然成立** —— 两边的幂等闸是同一个语义。
**但它切的是活的远端主路，而本机没有真远端可验** ⇒ 本轮不切，登记形态供下轮用。

## 二、改做：兜底那支的 `container:"none"` 切到 Rust

`renderFallback` 两格：`none` 是 `env → cd → argv`（= `launch_core::render_payload`，
U8c-1 起就有跨语言夹具）；`tmux` 还要外层容器命令（`session-backend.ts`）⇒ 那半归 U8c-3。
**只切 none 那一格**，它不需要外层容器、也不需要 daemon。

新命令 `render_launch_payload(req) -> Result<String, String>`。
**后端拒了就报错，绝不静默用 TS 版糊过去** —— 后端拒的正是「非法 configDir / 会裂的 arg」
那一类，静默回退等于把一次 fail-closed 变成 fail-open。

## 三、代码审计（D）：切换初版**没有任何判据**，是变异检查逼出来的

提交前跑了两个变异，**都存活**：

| 变异 | 加判据前 | 加判据后 |
|---|---|---|
| 把兜底 none 那格切回 TS（本轮唯一实质改动的回退） | **存活** | **红 2 条** |
| 后端拒渲染时静默回退到 TS（fail-closed → fail-open） | **存活** | **红 1 条** |

⇒ 补了两条：① 断言生产真的调 `render_launch_payload`、且送的是**结构化请求**不是渲染好的串；
② 断言后端拒绝时走「无法构造 resume 命令」且**根本不发起拉起**。

⚠ **这是 U8c-2a 咬过我的同一个形状**（那次是「接线没判据」）。区别是这次**在提交前自己抓到了** ——
「先问它能不能红」这条已经开始起作用。

## 四、工程审计（E）

- **账本 S28**：TS `renderFallback` 的 **none 那一格退役**（tmux 两格还在）⇒ 产出点从五份变成
  「四份半」。**不四舍五入成四份** —— 半份就是半份，`renderFallback` 还活着。
- **`parity_ledger` 又咬了五处**（对账表 / 命令总数 / 能力总数 / 不对称理由 / 天然不对称条数），
  逐条登记为 `NaturallyAsymmetric`（本地按 §36 + R07 不经 IR 产出命令）。
- **测试桩里加了一份最小镜像**（`remote-launch-run.vitest.ts` 的 `render_launch_payload` 路由）。
  ⚠ 它**不是第三份实现**：渲染正确性由 `payload-golden.json` 的跨语言对拍钉住，
  桩只是让 IPC 吐出形状对的串。这句写在桩的注释里。
- **U8a-2c 本体的形态已登记**（上面第一节），下轮可直接照做；它的风险是「切活的远端主路而本机
  无真远端可验」，不是「做不到」。

## 签收

- [x] 过代码审计（D）—— 两个变异**加判据前都存活**，补完后逐条转红
- [x] 过工程审计（E）—— S28 记成「四份半」；U8a-2c 形态登记；假阻塞的成因（`head` 截断）留档
- [x] 主计划已更新（F）
