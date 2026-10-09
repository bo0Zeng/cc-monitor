// 性能台架的方法计时（只在开发服务器下有用：模块按源码路径 import 到的就是界面正在用的那一份）。
// 由 `webkit.py --dev --profile tests/shots/perf/profile.js` 在开页安静后跑一次：给切 tab 路上那几个类的每个原型方法
// 挂上计时（含子调用的总时长 ＋ 次数），再给几样会逼浏览器当场排版的读（getBoundingClientRect · scrollHeight …）记时。
// `window.__prof.dump()` 交回按总时长排的表；`window.__prof.reset()` 清零。
const mods = {
  TabManager: ["/src/frontend/ui/tabs.ts", "TabManager"],
  TabStreamView: ["/src/frontend/ui/tab-stream-view.ts", "TabStreamView"],
  TabBarView: ["/src/frontend/ui/tab-bar-view.ts", "TabBarView"],
  MessageStream: ["/src/frontend/ui/stream.ts", "MessageStream"],
  TurnFold: ["/src/frontend/ui/turn-fold.ts", "TurnFold"],
  TurnRail: ["/src/frontend/ui/turn-rail.ts", "TurnRail"],
  SessionFindPanel: ["/src/frontend/ui/views/session-find.ts", "SessionFindPanel"],
  TasksPanel: ["/src/frontend/ui/tasks-panel.ts", "TasksPanel"],
  AgentsPanel: ["/src/frontend/ui/agents-panel.ts", "AgentsPanel"],
  SessionHead: ["/src/frontend/ui/session-head.ts", "SessionHead"],
  UsageHud: ["/src/frontend/ui/usage-hud.ts", "UsageHud"],
  SkeletonView: ["/src/frontend/ui/skeleton-view.ts", "SkeletonView"],
  BranchFolder: ["/src/frontend/ui/branch-fold.ts", "BranchFolder"],
  RecordTimeline: ["/src/frontend/ui/record-timeline.ts", "RecordTimeline"],
};
const stats = new Map();
const add = (k, dt) => {
  const s = stats.get(k) ?? { ms: 0, n: 0 };
  s.ms += dt;
  s.n += 1;
  stats.set(k, s);
};
for (const [label, [url, name]] of Object.entries(mods)) {
  let cls;
  try {
    cls = (await import(url))[name];
  } catch {
    continue;
  }
  if (!cls) continue;
  for (const k of Object.getOwnPropertyNames(cls.prototype)) {
    if (k === "constructor") continue;
    const d = Object.getOwnPropertyDescriptor(cls.prototype, k);
    if (!d || typeof d.value !== "function") continue;
    const f = d.value;
    cls.prototype[k] = function (...a) {
      const t = performance.now();
      try {
        return f.apply(this, a);
      } finally {
        add(`${label}.${k}`, performance.now() - t);
      }
    };
  }
}
// 会逼浏览器当场排版 / 算样式的读
const forced = [
  [Element.prototype, "getBoundingClientRect"],
  [Element.prototype, "getClientRects"],
];
for (const [proto, k] of forced) {
  const f = proto[k];
  proto[k] = function (...a) {
    const t = performance.now();
    try {
      return f.apply(this, a);
    } finally {
      add(`〔排版读〕${k}`, performance.now() - t);
    }
  };
}
for (const [proto, k] of [
  [Element.prototype, "scrollHeight"],
  [Element.prototype, "clientHeight"],
  [Element.prototype, "scrollTop"],
  [HTMLElement.prototype, "offsetHeight"],
  [HTMLElement.prototype, "offsetTop"],
]) {
  const d = Object.getOwnPropertyDescriptor(proto, k);
  if (!d?.get) continue;
  Object.defineProperty(proto, k, {
    ...d,
    get() {
      const t = performance.now();
      try {
        return d.get.call(this);
      } finally {
        add(`〔排版读〕${k}`, performance.now() - t);
      }
    },
  });
}
// 往收起的流里建卡的是谁（停放中的 MessageStream 上 insertNode / batchInsert）：记调用栈前几帧
const hidden = new Map();
try {
  const { MessageStream } = await import("/src/frontend/ui/stream.ts");
  for (const k of ["insertNode", "batchInsert"]) {
    const f = MessageStream.prototype[k];
    MessageStream.prototype[k] = function (...a) {
      if (this.parked) {
        const st = (new Error().stack ?? "").split("\n").slice(2, 9).map((l) => l.trim().replace(/\(?https?:\/\/[^/]+/, "").replace(/\?[^:]*:/, ":")).join(" ← ");
        hidden.set(st, (hidden.get(st) ?? 0) + 1);
      }
      return f.apply(this, a);
    };
  }
} catch {
  // 没有这个模块（构建版）
}
const gcs = window.getComputedStyle;
window.getComputedStyle = function (...a) {
  const t = performance.now();
  try {
    return gcs.apply(this, a);
  } finally {
    add("〔排版读〕getComputedStyle", performance.now() - t);
  }
};
window.__prof = {
  hidden: () => [...hidden.entries()].sort((a, b) => b[1] - a[1]).slice(0, 12),
  reset: () => {
    stats.clear();
    hidden.clear();
  },
  dump: () =>
    [...stats.entries()]
      .sort((a, b) => b[1].ms - a[1].ms)
      .slice(0, 60)
      .map(([k, s]) => ({ k, ms: Math.round(s.ms * 10) / 10, n: s.n })),
};
return true;
