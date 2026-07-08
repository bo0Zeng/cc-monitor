# E2E 套件(Batch13-F40 起)

无 devtools/eval 通道(生产与 `CCM_NO_DEVTOOLS=1` 下 webview 不可注入)——断言数据
全部走 **DEV 探针 → 后端日志**:

- `src/e2e-probe.ts`(仅 dev 构建,`import.meta.env.DEV` 门控):
  - 启动重放抖动探针:batch 窗口内逐 rAF 采样定点卡片 `getBoundingClientRect().top`
    的方向反转(INVARIANTS §21:scrollTop 单调,只测它发现不了抖动),批末落盘
    `[e2e] jitter frames=… reversals=… retargets=…`。
  - 状态快照:`Ctrl+Alt+F9` **或中键点状态栏**(headless 用——xdotool 的 XTEST
    合成键盘进不了 WebKitGTK webview,鼠标事件畅通)→ `[e2e] snapshot
    {sid,scrollTop,distBottom,pending,midBuffer,timeline,foldWraps,sentinel,err}`。
- 日志:`~/.claude/claudecode-frontend/logs/monitor.<日期>.log`,grep `fe_perf`。
- 抖动指标 = **密度绊线**(反转/帧):守卫 snap 的整数 scrollTop 对分数行高布局有
  ±亚像素合法舍入摆动,幅度与 §21 病态同级、密度差一个量级——健康 ≈0.12-0.16,
  病态 ≈1.0,断言 ≤0.4(标定 2026-07-08,详 src/e2e-probe.ts 头注释)。

## 跑法

```bash
# 前置:Xvfb + dev 实例(探针随 debug 构建自动就绪)
Xvfb :80 -screen 0 1920x1080x24 &
DISPLAY=:80 CCM_NO_DEVTOOLS=1 npx tauri dev &   # 等编译完、窗口出现

./e2e/f40-suite.sh          # 环境变量:E2E_DISPLAY / E2E_LOG / E2E_DRAIN_MAX_MS
```

**单实例串行**:fixture 目录/cwd 固定名(`-tmp-e2e-fork`)且 `touch src/main.ts` 会触发
全窗口 reload——并发跑两个套件会互删 fixture、互触发重放,结果不可信。

套件场景:①启动门控(rendered≪deferred)+ drain 阈值 + 抖动密度绊线;②贴底快照;
③上翻补批(active + 厚账 tab 两处,pending 下降断言);④逐 tab 点击切换贴底;
⑤合成 fork 会话折叠段断言——**fixture 必须伴生活进程 pidfile 且 pidfile 先落**
(watcher 只 emit 活跃会话,Batch5-F20;jsonl 先落会被 process_file 抢跑跳过,实测);
⑥trap 清理(pidfile/宿主进程/项目目录)。
无 WM 注意:主窗必须先 `xdotool windowraise`(tear-off 浮窗会按 z 序吃掉指针事件)。

## 人工场景(未脚本化,原因与流程)

**chunked 大增量批(R-1 缓冲)**:触发面 = 远端 SSH 重连 chunked 重放(末块先发)+
离线期 >600 行增量——本地 watcher 追加是升序到达,按构造不产生中部插入,无法本地
合成;脚本化需可控地断开/重连 daemon 且不污染真实会话镜像。已由单测钉住路由与
批末排序挂载(`tabs.vitest.ts`「R-1」用例);人工验证流程:
1. 远端机器上对某会话 tmux 挂起 monitor 连接(断网/杀 daemon 进程);
2. 该会话继续产出 >600 行;
3. 恢复连接 → 观察该 tab:内容一次性补齐、贴底不逐帧抖、无 NotFoundError。

**WebView2(生产)复核**:WebKitGTK 无 overflow-anchor,补批补偿路径两端语义不同;
发版前在 Windows 真机把 ①③④ 手动过一遍。
