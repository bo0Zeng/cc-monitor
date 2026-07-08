/**
 * Batch13-F40c:DEV-only E2E 探针。生产构建零包含——main.ts 以
 * `if (import.meta.env.DEV)` 门控动态 import,vite build 时整支消除。
 *
 * 背景:生产/CCM_NO_DEVTOOLS 下 webview 无 devtools、无 eval 通道,自动化断言的
 * 唯一出口是后端日志(frontend_perf_log → grep fe_perf)。本模块把两类不可见状态
 * 变成可 grep 的 `[e2e]` 行:
 * - 启动重放抖动:§21 明示「scrollTop 单调不震荡,只测 scrollTop 发现不了」——
 *   必须逐帧采样可见元素 getBoundingClientRect().top 的方向反转。
 * - 状态快照:贴底距离/账本余量/fold 数等(Ctrl+Alt+F9 触发,e2e 套件用 xdotool 按)。
 */
import { invoke } from "@tauri-apps/api/core";

function log(line: string): void {
  console.info(line);
  void invoke("frontend_perf_log", { lines: line }).catch(() => {});
}

/**
 * 方向反转计数:相邻差绝对值 ≥threshold 才算有效位移(滤 HiDPI 亚像素噪声),
 * 有效位移方向翻转一次计一次。导出仅为单测。
 */
export function countReversals(tops: number[], threshold = 0.3): number {
  let rev = 0;
  let lastDir = 0;
  for (let i = 1; i < tops.length; i++) {
    const d = tops[i] - tops[i - 1];
    if (Math.abs(d) < threshold) continue;
    const dir = d > 0 ? 1 : -1;
    if (lastDir !== 0 && dir !== lastDir) rev += 1;
    lastDir = dir;
  }
  return rev;
}

let rafId = 0;
let tops: number[] = [];
let target: Element | null = null;
let retargets = 0;

/**
 * 启动重放抖动探针:锁定 active stream 的**首卡**,逐 rAF 采样其 top。
 *
 * 为什么是首卡不是末卡:末卡上方的异步内容沉降(lazy hljs 高亮、字体后载、图片)
 * 会推挤其位置——那是内容加载不是抖动(首跑实测 +23 伪反转)。首卡文档位置在
 * F40a 语义下重放期恒定(零上方插入),top = 常数 − scrollTop,信号最干净。
 *
 * **指标语义 = 密度绊线,不是零断言**(2026-07-08 标定):守卫 snap 的整数
 * scrollTop 对分数行高布局天然有 ±亚像素合法舍入摆动,幅度与 §21 病态抖动同级
 * (±0.5px),**幅度/次数阈值无法区分,密度可以**——健康基线 ≈0.12-0.16 反转/帧,
 * deferMode 缺位的病态 ≈1.0 反转/帧(66 帧 66 反转)。套件断言 density ≤0.4,
 * 两边都有余量;回归到逐帧震荡必然绊线。
 * onBatchStart 起、onBatchEnd 停(批末 fold/物化的一次性位移不在测量窗内)。
 */
export function startReplayJitterProbe(): void {
  stopReplayJitterProbe(false);
  tops = [];
  target = null;
  retargets = 0;
  const sample = (): void => {
    // 防呆:batch-end 永不到达(后端 emit 后崩溃)时不无界采样(~60/s)
    if (tops.length > 100_000) {
      stopReplayJitterProbe(false);
      return;
    }
    const content = document.querySelector(".stream.active > .stream-content");
    if (content && content.firstElementChild) {
      // 批中切 tab:旧 target 仍 isConnected(hidden stream 保布局)但已不属于
      // active stream——必须按归属重锚,否则指标测的是不可见 tab(D 审计)
      if (!target || !target.isConnected || !content.contains(target)) {
        if (target) retargets += 1;
        target = content.firstElementChild;
        tops.push(Number.NaN); // 换锚哨兵:切断前后差值,不产生伪反转
      }
      tops.push(target.getBoundingClientRect().top);
    }
    rafId = requestAnimationFrame(sample);
  };
  rafId = requestAnimationFrame(sample);
}

export function stopReplayJitterProbe(report = true): void {
  if (rafId) cancelAnimationFrame(rafId);
  rafId = 0;
  // frames=0 也落盘:让套件能区分「探针没加载」与「active tab 整批无内容」
  // (last-active 指向已消亡会话时后者会发生)
  if (report) {
    // NaN 哨兵按段切开分别计数(NaN 参与的比较恒 false → 差值被 threshold 滤不掉,
    // 显式分段最稳)
    let rev = 0;
    let seg: number[] = [];
    for (const t of tops) {
      if (Number.isNaN(t)) {
        rev += countReversals(seg);
        seg = [];
      } else {
        seg.push(t);
      }
    }
    rev += countReversals(seg);
    const density = tops.length > 1 ? (rev / tops.length).toFixed(3) : "0";
    log(
      `[e2e] jitter frames=${tops.length} reversals=${rev} density=${density} retargets=${retargets}`,
    );
  }
  tops = [];
  target = null;
}

/**
 * 状态快照触发器:①Ctrl+Alt+F9(真桌面调试用);②**中键点状态栏**——xdotool 的
 * XTEST 合成键盘事件进不了 WebKitGTK webview(Xvfb 实测),鼠标事件畅通,headless
 * 套件走这条。getSnapshot 由 main.ts 提供(读 TabManager)。
 */
export function registerSnapshotHotkey(getSnapshot: () => string): void {
  window.addEventListener("keydown", (e) => {
    if (e.ctrlKey && e.altKey && e.key === "F9") {
      e.preventDefault();
      log(`[e2e] snapshot ${getSnapshot()}`);
    }
  });
  window.addEventListener("auxclick", (e) => {
    if (e.button === 1 && (e.target as Element | null)?.closest?.("#status-bar")) {
      e.preventDefault();
      log(`[e2e] snapshot ${getSnapshot()}`);
    }
  });
}
