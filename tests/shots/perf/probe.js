/*
 * 性能台架的页内探针：在页面任何脚本之前装好（`Page.addScriptToEvaluateOnNewDocument`），记
 *   - 长任务（PerformanceObserver `longtask`，> 50 ms）；
 *   - 输入事件的「按下到下一帧画出」（Event Timing，`durationThreshold` 16 ms；短于 16 的不报 ⇒ 记作 < 16）；
 *   - 点击的同步处理时长（捕获阶段起表、window 冒泡阶段停表 —— tab 栏的点击处理挂在栏上，冒泡到 window 时已跑完）
 *     与之后第一帧（rAF ＋ 一个宏任务 ≈ 那一帧交出去之后）；
 *   - 消息流里新建的节点（MutationObserver，按落在当前可见那条流 / 后台流里分开数）；
 *   - 帧间隔（`frames.start()` 起一条 rAF 链）。
 * 只给 `bench.mjs` 用；不进产品构建。
 */
(() => {
  const P = {
    lt: [],
    ev: [],
    clicks: [],
    mut: [],
    frames: null,
  };
  window.__perf = P;
  try {
    new PerformanceObserver((l) => {
      for (const e of l.getEntries()) P.lt.push({ s: e.startTime, d: e.duration });
    }).observe({ type: "longtask", buffered: true });
  } catch {
    P.noLongtask = true;
  }
  try {
    new PerformanceObserver((l) => {
      for (const e of l.getEntries()) {
        P.ev.push({ name: e.name, s: e.startTime, d: e.duration, ps: e.processingStart, pe: e.processingEnd });
      }
    }).observe({ type: "event", durationThreshold: 16, buffered: true });
  } catch {
    P.noEventTiming = true;
  }
  document.addEventListener(
    "click",
    (e) => {
      const c = { t0: performance.now(), ts: e.timeStamp, sync: null, frame: null, frame2: null };
      P.clicks.push(c);
      window.addEventListener(
        "click",
        () => {
          c.sync = performance.now() - c.t0;
          requestAnimationFrame(() => {
            const ch = new MessageChannel();
            ch.port1.onmessage = () => {
              c.frame = performance.now() - c.t0;
            };
            ch.port2.postMessage(0);
            // 第二个 rAF：上一帧（切换之后的第一帧）整帧画完、下一帧开始 —— 不认 Event Timing 的引擎（WebKit）用它当「画出来了」
            requestAnimationFrame(() => {
              c.frame2 = performance.now() - c.t0;
            });
          });
        },
        { once: true },
      );
    },
    true,
  );
  const watch = () => {
    // 主窗口数消息流（按可见 / 后台流分开）；别的窗（设置窗 · 查看窗）没有消息流 ⇒ 整个 body 都算「可见」。
    // 主窗口的消息流晚于 DOMContentLoaded 才建 ⇒ 只有主窗口才等它（别的窗不留一条 20 ms 的空转）。
    const main = !/^\/(settings|viewer)\.html$/.test(location.pathname);
    const root = main ? document.getElementById("message-stream") : document.body;
    if (!root) return void setTimeout(watch, 20);
    new MutationObserver((recs) => {
      const t = performance.now();
      let on = 0;
      let off = 0;
      for (const r of recs) {
        let n = 0;
        for (const a of r.addedNodes) n += a.nodeType === 1 ? 1 + a.getElementsByTagName("*").length : 0;
        if (n === 0) continue;
        const target = r.target.nodeType === 1 ? r.target : r.target.parentElement;
        const s = target?.closest?.(".stream");
        if (s && !s.classList.contains("active")) off += n;
        else on += n;
      }
      if (on + off > 0) P.mut.push({ t, on, off });
    }).observe(root, { childList: true, subtree: true });
  };
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", watch);
  else watch();

  // 开窗那一段的帧间隔：DOMContentLoaded 起记 15 s（人一开窗就去点 —— 这一段一帧卡多久，就是点下去要等多久）
  P.bootFrames = [];
  const bootRec = () => {
    const t0 = performance.now();
    let last = t0;
    const step = (t) => {
      P.bootFrames.push(t - last);
      last = t;
      if (t - t0 < 15000) requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  };
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", bootRec);
  else bootRec();

  /** 某一时刻之后的那一截读数。 */
  P.since = (t) => ({
    lt: P.lt.filter((x) => x.s + x.d >= t),
    ev: P.ev.filter((x) => x.s >= t),
    clicks: P.clicks.filter((x) => x.t0 >= t),
    mut: P.mut.filter((x) => x.t >= t),
  });
  /** 等主线程安静：最近 `quietMs` 里没有长任务、也没有新建节点；最多等 `maxMs`。返回等了多久。 */
  P.quiet = (quietMs, maxMs) =>
    new Promise((resolve) => {
      const t0 = performance.now();
      const tick = () => {
        const now = performance.now();
        const lastLt = P.lt.length ? P.lt[P.lt.length - 1].s + P.lt[P.lt.length - 1].d : 0;
        const lastMut = P.mut.length ? P.mut[P.mut.length - 1].t : 0;
        if ((now - Math.max(lastLt, lastMut) >= quietMs && now - t0 >= quietMs) || now - t0 > maxMs) resolve(now - t0);
        else setTimeout(tick, 50);
      };
      tick();
    });
  P.frameStart = () => {
    const f = { last: performance.now(), d: [], on: true };
    P.frames = f;
    const step = (t) => {
      if (!f.on) return;
      f.d.push(t - f.last);
      f.last = t;
      requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  };
  P.frameStop = () => {
    if (!P.frames) return [];
    P.frames.on = false;
    return P.frames.d.slice(1);
  };
})();
