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
    /** 骨架占位插进流里的时刻（接骨架那一下；MutationObserver 回调里记）。 */
    gaps: [],
    frames: null,
  };
  window.__perf = P;
  // 进出后端的那几下：`invoke`（问 · 答回来的那一刻）与通道 / 事件回调（后端推来的那一包，记时刻与大小）。
  // 假适配层（`@tauri-apps/api/mocks`）往 `window.__TAURI_INTERNALS__` 上挂 `invoke` / `transformCallback`：先放一个空对象，挂上那一下就包一层。
  P.ipc = [];
  {
    const internals = {};
    let invoke;
    let transform;
    Object.defineProperty(internals, "invoke", {
      configurable: true,
      get: () => invoke,
      set: (f) => {
        invoke = (cmd, a, o) => {
          // 帧命令都走 `chan_call`：记上它问的是哪一条（`op`）
          const row = { k: "call", cmd: typeof a?.op === "string" ? `${cmd}:${a.op}` : String(cmd), t: performance.now() };
          P.ipc.push(row);
          const p = f(cmd, a, o);
          Promise.resolve(p).then(
            () => (row.done = performance.now()),
            () => (row.done = performance.now()),
          );
          return p;
        };
      },
    });
    Object.defineProperty(internals, "transformCallback", {
      configurable: true,
      get: () => transform,
      set: (f) => {
        transform = (cb, once) =>
          f((data) => {
            const t = performance.now();
            const out = cb?.(data);
            const body = data?.message ?? data;
            const bytes = typeof body === "string" ? body.length : Array.isArray(body?.items) ? body.items.reduce((n, it) => n + (typeof it?.body === "string" ? it.body.length : 0), 0) : 0;
            P.ipc.push({ k: "push", t, run: performance.now() - t, bytes });
            return out;
          }, once);
      },
    });
    window.__TAURI_INTERNALS__ = internals;
  }
  // 页里建过的每个 IntersectionObserver 都记下来（试验项「摘掉 IO 再量空闲」用）
  P.ios = [];
  if (typeof IntersectionObserver === "function") {
    const IO = IntersectionObserver;
    window.IntersectionObserver = function (cb, opts) {
      const io = new IO(cb, opts);
      io.__targets = new Set();
      const ob = io.observe.bind(io);
      const un = io.unobserve.bind(io);
      io.observe = (el) => (io.__targets.add(el), ob(el));
      io.unobserve = (el) => (io.__targets.delete(el), un(el));
      P.ios.push(io);
      return io;
    };
    window.IntersectionObserver.prototype = IO.prototype;
  }
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
        for (const a of r.addedNodes) if (a.nodeType === 1 && a.classList.contains("stream-skeleton-gap")) P.gaps.push(t);
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

  // 开窗那一段的帧（人一开窗就去点 —— 这一段一帧卡多久，就是点下去要等多久）：DOMContentLoaded 起一条 rAF 链，
  // 每帧记 [这一帧开始的时刻, 与上一帧的间隔]，记到安静为止（不固定记多久：世界起得慢时固定 15 s 会整个漏掉产品那一段）。
  // 另记两个时刻：流里第一张卡出现（主窗口）· `__shots` 离开 booting（场景演完）—— 早的那个是产品那一段的起点，之前是台架起世界、页里还没东西可画。
  // 安静 ＝ 起点之后 `BOOT_QUIET_MS` 里没有长任务、也没有新建节点（同 `quiet`）：链自己停，`bootStopped` 兑现；台架也可以先 `bootEnd()` 收掉。
  const BOOT_QUIET_MS = 1000;
  /** 一直不安静（页里有东西不停地建节点）也只记这么久，免得链一直跑、算进后面量的空闲 CPU。 */
  const BOOT_CAP_MS = 180_000;
  /** 安静那一刻（最后一个长任务结束 / 最后一次建节点）之后，接着画出来的那一帧最晚落在这么久之内。 */
  const BOOT_TAIL_MS = 100;
  const lastBusy = () => Math.max(P.lt.length ? P.lt[P.lt.length - 1].s + P.lt[P.lt.length - 1].d : 0, P.mut.length ? P.mut[P.mut.length - 1].t : 0);
  const B = (P.boot = { frames: [], firstCard: null, booted: null, quietAt: null, on: true });
  let stopped = () => undefined;
  /** 开窗那条链停了（安静了 / 到了上限 / 台架收了）。 */
  P.bootStopped = new Promise((r) => (stopped = r));
  const bootStart = () => (B.firstCard === null && B.booted === null ? null : Math.min(B.firstCard ?? Infinity, B.booted ?? Infinity));
  const bootStop = () => {
    if (!B.on) return;
    B.on = false;
    const start = bootStart();
    B.quietAt = start === null ? null : Math.max(lastBusy(), start);
    stopped();
  };
  const bootRec = () => {
    let last = performance.now();
    const step = (t) => {
      if (!B.on) return;
      B.frames.push([t, t - last]);
      last = t;
      if (B.firstCard === null && document.querySelector("#message-stream .card")) B.firstCard = t;
      if (B.booted === null && window.__shots && window.__shots.state !== "booting") B.booted = t;
      const start = bootStart();
      if ((start !== null && t - Math.max(lastBusy(), start) >= BOOT_QUIET_MS) || t > BOOT_CAP_MS) return bootStop();
      requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  };
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", bootRec);
  else bootRec();
  /**
   * 开窗那一段的读数（链还在跑就当场收掉）：起点（第一张卡与离开 booting 早的那个）到安静那一刻（最后一个长任务 / 新建节点）的帧间隔，
   * 另交世界握上手的那一刻（假适配层记的 `ccm-world-ready`）。时刻都是 `performance.now()`（从导航起算）。起点一个都没见到 ⇒ `start` 为 null、帧为空。
   */
  P.bootEnd = () => {
    bootStop();
    const start = bootStart();
    const quietAt = B.quietAt ?? lastBusy();
    const world = performance.getEntriesByName("ccm-world-ready")[0]?.startTime ?? null;
    // 起点那一刻收到的那一帧间隔里就有插第一张卡那一下 ⇒ 从它算起；到安静那一刻之后紧跟的那一帧为止（`BOOT_TAIL_MS` 之内画出的）。
    // 再往后的帧间隔是闲着没东西画：WebKit 闲时把 rAF 放慢到一秒一次，算进来就是一个假的 1000 ms 卡帧。
    const frames = start === null ? [] : B.frames.filter(([t]) => t >= start && t <= quietAt + BOOT_TAIL_MS).map(([, d]) => d);
    return { start, firstCard: B.firstCard, booted: B.booted, world, quietAt, frames };
  };

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
    const f = { last: performance.now(), d: [], at: [], on: true };
    P.frames = f;
    const step = (t) => {
      if (!f.on) return;
      f.d.push(t - f.last);
      f.at.push(t);
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
  /**
   * 「停住之后最长一帧」：`t0`（点下去）起 `fromMs` 之后结束的帧里最长那一帧（ms）。冷切进一个 tab、停住（宿主的停留判定 150 ms）之后
   * 那一帧接骨架、补可见区 —— 这一帧多长，就是人停下来看时第一下滚 / 点要等多久。`fromMs` 由台架给（停留判定与切换那一帧画完取晚的）。
   * 先 `frameStart`，`frameStop` 之前调。
   */
  /**
   * 「接骨架那一帧」：`t0` 之后第一次插骨架占位的那一刻落在哪一帧里 —— 那一帧的帧间隔（ms）；这一下没接骨架 ⇒ `null`。
   * 冷切停住之后接骨架、补可见区都在这一帧里（插占位、补可见区、DOM 变动回调排版是同一个任务）。
   */
  P.attachFrame = (t0) => {
    const f = P.frames;
    const g = P.gaps.find((t) => t >= t0);
    if (!f || g === undefined) return null;
    for (let k = 1; k < f.d.length; k++) if (f.at[k] > g) return f.d[k];
    return null;
  };
  /**
   * 点开一个 tab 之后的时间线：每一帧（rAF）看一眼当前那条流的视口 —— 已建条目几个、视口里有字的卡几张、
   * 视口被已建条目盖住多少像素、被骨架占位（没建的那几行）盖住多少；另记那一刻建过卡 / 收进账本的记录数，与进出后端的那几下（`ipc`）。
   * 读几何要排版：读在 rAF 里，排的是那一帧本来就要排的那一份（同一帧里之后再改 DOM 才多排一次）。
   * 记到安静（`quietMs` 里没有新建节点、没有长任务）或 `maxMs` 为止；交回 `{ samples, ipc, quietAt }`，时刻都从 `t0` 起算。
   */
  P.timeline = (t0, quietMs, maxMs) =>
    new Promise((resolve) => {
      const samples = [];
      const step = (now) => {
        const s = document.querySelector("#message-stream .stream.active");
        const perf = window.__ccmPerf ?? {};
        const row = { t: now - t0, items: 0, textCards: 0, coverPx: 0, gapPx: 0, viewPx: 0, rendered: perf.recordsRendered ?? 0, deferred: perf.recordsDeferred ?? 0 };
        if (s) {
          const v = s.getBoundingClientRect();
          row.viewPx = Math.round(v.height);
          row.scrollTop = Math.round(s.scrollTop);
          row.scrollHeight = s.scrollHeight;
          // 视口里自上而下扫一遍流里的顶层条目（`.stream-content` 的孩子：卡 · 折叠的包 · 骨架占位；各条不重叠，按先后取并集）
          let reached = v.top;
          let first = null;
          const content = s.querySelector(":scope > .stream-content");
          for (const el of content ? content.children : []) {
            row.items += 1;
            const r = el.getBoundingClientRect();
            const top = Math.max(r.top, reached);
            const bottom = Math.min(r.bottom, v.bottom);
            if (bottom <= top) continue;
            if (first === null) first = top;
            reached = bottom;
            if (el.classList.contains("stream-skeleton-gap")) row.gapPx += bottom - top;
            else {
              row.coverPx += bottom - top;
              if ((el.textContent ?? "").trim()) row.textCards += 1;
            }
          }
          // 视口与流内容框相交的那一段里，最上一条之上、最下一条之下空着的像素（条与条之间的外边距、流的内边距不算）：没东西可画
          const c = content ? content.getBoundingClientRect() : v;
          const lo = Math.max(c.top, v.top);
          const hi = Math.min(c.bottom, v.bottom);
          row.holePx = hi <= lo ? 0 : first === null ? Math.round(hi - lo) : Math.round(Math.max(0, first - lo) + Math.max(0, hi - reached));
          row.coverPx = Math.round(row.coverPx);
          row.gapPx = Math.round(row.gapPx);
        }
        samples.push(row);
        const busy = Math.max(lastBusy(), t0);
        if (now - busy >= quietMs || now - t0 > maxMs) {
          const ipc = P.ipc.filter((x) => x.t >= t0).map((x) => ({ ...x, t: x.t - t0, ...(x.done !== undefined ? { done: x.done - t0 } : {}) }));
          // 安静那一刻视口里的样子：视口框 · 流的内边距 · 与视口相交的每一条（类名 · 上下沿相对视口顶）
          let layout = null;
          if (s) {
            const v = s.getBoundingClientRect();
            const cs = getComputedStyle(s);
            const content = s.querySelector(":scope > .stream-content");
            const at = (el) => [String(el.className).slice(0, 40), Math.round(el.getBoundingClientRect().top - v.top), Math.round(el.getBoundingClientRect().bottom - v.top)];
            layout = {
              pad: [cs.paddingTop, cs.paddingBottom],
              rows: [...(content ? content.children : [])].map(at).filter(([, a, b]) => b > -50 && a < v.height + 50),
              other: [...s.children].filter((el) => el !== content).map(at),
            };
          }
          resolve({ samples, ipc, quietAt: busy - t0, layout });
        } else requestAnimationFrame(step);
      };
      requestAnimationFrame(step);
    });
  P.frameAfter = (t0, fromMs) => {
    const f = P.frames;
    if (!f) return 0;
    let m = 0;
    for (let k = 1; k < f.d.length; k++) if (f.at[k] - t0 >= fromMs) m = Math.max(m, f.d[k]);
    return m;
  };
})();
