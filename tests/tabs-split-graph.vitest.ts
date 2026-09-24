// @vitest-environment node
/**
 * 〔U2 · 第三波 · `设计/01 §1.5`「一个 store，一个 router」〕**拆 `tabs.ts` 的判据：拆出来的每一份职责单一。**
 *
 * 拆之前 `tabs.ts` 是一个类干五件事（会话状态账 · 路由 · 实时流视图 · tab 栏视图 · 会话动作，
 * 逐件现打写在 `tabs.ts` 的头注里）。拆完是 13 份。「看起来拆开了」有两种假法，本文件各钉一条：
 *
 * ① **搬了家、依赖却没断** —— 比如 tab 栏视图照样 import 会话动作、路由里又直呼 `invoke`。
 *    ⇒ 每一份的**直接运行期依赖**与下面的登记表**两向相等**（多一条、少一条都红）。
 *    登记表每一行都写着「它为什么需要这个」—— 写不出理由的依赖就是职责漏了。
 * ② **组装根又长回去** —— 有人图省事把新逻辑直接写进 `tabs.ts`。
 *    ⇒ `tabs.ts` 的顶层声明 == `{TabManager}`，导出面 == 登记（它对外的 import 面，拆前拆后逐字相同）。
 *
 * 外加三条分工的零命中（**每条带同一谓词的正控**，免得零命中是因为谓词拼错）：
 * - 直呼 `invoke`（`@tauri-apps/api/core`）只在 `tab-session-actions.ts`；
 * - 渲染栈（流 / 时间线 / 折叠层 / 卡片 / 逐条渲染）只在 `tab-stream-view.ts`（组装根除外）；
 * - 纯模块（形状 / 落点算术 / store / 路由 / 事实抽取）一处 `document.` / `window.` 都不碰。
 *
 * # 口径（照 `entry-graphs.vitest.ts` 的形状，但量的是源码 import 而不是构建产物）
 *
 * - 「运行期依赖」= 剥注释后的 `import … from` / `export … from`，**`import type` / `export type` 不算**
 *   （TS 编译期擦除，运行期不存在；与 `import-cycle-guard.vitest.ts` 同一口径）。相对路径解析到仓相对文件，
 *   裸包名记成 `npm:<包>`。
 * - 只量**直接**依赖。传递闭包在这里判不了职责：`accounts.ts` 自己就直呼 `invoke`，
 *   tab 栏视图为了画账号徽章要它 —— 那是账号面的事，不是 tab 栏视图长了 IPC。
 *
 * # 买到 / 买不到
 *
 * - ✅ 每一份的依赖面被钉死；分工的三条边界有零命中 ＋ 正控；组装根只剩一个类。
 * - ❌ 行为不变 —— 那由 `tabs.vitest.ts` 等既有判据买（拆分全程断言一条没改，逐子步全绿）。
 * - ❌ 「职责切得对不对」本身 —— 登记表是人写的切法，本文件只保证代码与切法一致。
 *
 * # 死值验（逐刀：下在哪 · 红在哪；刀后逐字节恢复，恢复后全绿）
 *
 * 读数在交付报告里（本文件不抄数，免得第二份副本漂）。
 */
import { readFileSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { REPO_ROOT } from "./test-support/repo-root.ts";
import { stripComments } from "./test-support/strip-comments.ts";

const read = (rel: string): string => readFileSync(resolve(REPO_ROOT, rel), "utf8");

/** 一份文件的直接运行期依赖（仓相对路径 / `npm:<包>`），排序去重。 */
export function runtimeImports(rel: string, src: string = read(rel)): string[] {
  const code = stripComments(src, "ts");
  const abs = resolve(REPO_ROOT, rel);
  const out = new Set<string>();
  const re = /(?:^|\n)\s*(?:import|export)\s+([^;]*?)\s*from\s*["']([^"']+)["']/g;
  for (const m of code.matchAll(re)) {
    const clause = m[1].trim();
    if (/^type\b/.test(clause)) continue; // `import type {…}` / `export type {…}`
    const braced = /^\{([\s\S]*)\}$/.exec(clause);
    if (braced) {
      const items = braced[1]
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
      if (items.length > 0 && items.every((s) => /^type\s/.test(s))) continue; // `{ type A, type B }`
    }
    const spec = m[2];
    if (!spec.startsWith(".")) {
      out.add(`npm:${spec}`);
      continue;
    }
    const base = resolve(dirname(abs), spec.replace(/\.ts$/, ""));
    let hit: string | null = null;
    for (const cand of [`${base}.ts`, join(base, "index.ts")]) {
      try {
        if (statSync(cand).isFile()) {
          hit = cand;
          break;
        }
      } catch {
        /* 下一个候选 */
      }
    }
    // 解析不出来的相对 import 不许静默丢掉 —— 丢了就是「少一条边还照样绿」。
    out.add(hit ? relative(REPO_ROOT, hit).replace(/\\/g, "/") : `UNRESOLVED:${spec}`);
  }
  return [...out].sort();
}

/**
 * 拆出来的 13 份 × 它们各自的直接运行期依赖。**这张表就是切法**：每一行的理由写在旁边。
 * 改一份文件的依赖 ⇒ 回来改这里，并写清为什么这一份需要它。
 */
const DEPS: Record<string, readonly string[]> = {
  // 组装根：把下面每一份接起来 ＋ 原样 re-export 旧的 import 面（tab-drop / tab-model / tmux-sessions）。
  "src/tabs.ts": [
    "src/accounts.ts", // debugSessionsSnapshot 的「账号不一致」派生（detectAccountMismatch）
    "src/cards/index.ts", // onLine：这一行是不是 compact 摘要（换号重启的等待者）
    "src/error-toast.ts", // bringActiveTerminalToFront：非 Windows 说一句实话
    "src/fork-flow.ts", // startForkedSession（E78：fork-flow.vitest 钉「tabs.ts 调 runForkFlow」）
    "src/live-window.ts", // ensureTab：新 tab 的尾部窗口
    "src/tab-bar-drag.ts",
    "src/tab-bar-prefs.ts",
    "src/tab-bar-view.ts",
    "src/tab-drop.ts", // re-export
    "src/tab-menu.ts",
    "src/tab-model.ts", // computeTitleFor
    "src/tab-router.ts",
    "src/tab-session-actions.ts",
    "src/tab-session-facts.ts",
    "src/tab-store.ts",
    "src/tab-stream-view.ts",
    "src/tasks-panel.ts", // ensureTab：初始 task 快照
    "src/terminal-front.ts", // bringActiveTerminalToFront 的 OS 门
    "src/tmux-sessions.ts", // re-export
    "src/turn-notify.ts", // onLine：轮次结束通知
    "src/views/context-limit.ts", // snapshotSessions 的 context%
  ],
  // ① 形状：只有类型。
  "src/tab-model.ts": [],
  // ④ 落点算术：纯函数，只认集合表的增删。
  "src/tab-drop.ts": ["src/tab-collections.ts"],
  // ① store：零依赖（只存东西、只做顺序运算、只有一份订阅）。
  "src/tab-store.ts": [],
  // ② 路由：只写「上次的 tab」那一格 localStorage。
  "src/tab-router.ts": ["src/local-storage.ts"],
  // ① 事实抽取：agent 工具名判定 · 写类工具的文件路径。
  "src/tab-session-facts.ts": ["src/cards/subagent.ts", "src/panorama/session-files.ts"],
  // ③ 实时流视图：渲染栈 ＋ 骨架 ＋ 大纲 ＋ 分叉按钮，经 ipc/commands 包装层要骨架索引与正文。
  //   〔SE2〕大纲的界面从直接建 `UserInputPanel` 换成建查找面板（它里面挂着大纲）⇒ `user-input-panel` 只剩类型依赖。
  "src/tab-stream-view.ts": [
    "src/backend-policy.ts",
    "src/branch-button.ts",
    "src/branch-fold.ts",
    "src/cards/index.ts",
    "src/height-estimate.ts",
    "src/ipc/commands.ts",
    "src/record-timeline.ts",
    "src/render-stream-record.ts",
    "src/skeleton-view.ts",
    "src/stream.ts",
    "src/views/outline-source.ts",
    "src/views/session-find.ts", // 〔SE2〕查找面板（搜索 ／ 大纲两个模式）
    "src/views/session-viewer.ts", // 只为 revealCard（方向别扭的那条，理由在 import 处）
  ],
  // ④ tab 栏视图：画按钮（账号徽章 · 状态灯 · 分组 · ↗ 的 OS 门），手势全交宿主。
  "src/tab-bar-view.ts": [
    "src/account-color.ts",
    "src/accounts.ts",
    "src/session-status.ts",
    "src/tab-collections.ts",
    "src/terminal-front.ts",
  ],
  // ④ 拖拽：落点算术 ＋ 建组时铸一个集合 id。
  "src/tab-bar-drag.ts": ["src/tab-collections.ts", "src/tab-drop.ts"],
  // ④ 落盘偏好：集合 / 固定 / 顺序的盘上那一层。
  "src/tab-bar-prefs.ts": ["src/tab-bar-state.ts", "src/tab-collections.ts"],
  // ⑤ 菜单放哪几项：账号 flyout · tmux 判据 · attach / 预览 · 菜单控件 · 会话动作。
  "src/tab-menu.ts": [
    "src/agent-profile.ts",
    "src/backend-policy.ts",
    "src/behavior.ts",
    "src/error-toast.ts",
    "src/launch-menu.ts",
    "src/remote-launch-run.ts",
    "src/tab-collections.ts",
    "src/tab-context-menu.ts",
    "src/tab-session-actions.ts",
    "src/tmux-sessions.ts",
    "src/views/pane-preview.ts",
  ],
  // ⑤ 菜单控件：零依赖（纯 DOM）。
  "src/tab-context-menu.ts": [],
  // ⑤ 会话动作：tab 层唯一直呼 invoke 的一份。
  "src/tab-session-actions.ts": [
    "npm:@tauri-apps/api/core",
    "npm:@tauri-apps/plugin-opener",
    "src/account-restart.ts",
    "src/accounts.ts",
    "src/backend-policy.ts",
    "src/behavior.ts",
    "src/error-toast.ts",
    "src/file-window.ts", // F78：远端会话「打开工作目录」（〔F7b〕老 SFTP 面板删了，改开文件窗口）
    "src/ipc/commands.ts",
    "src/launch-requests.ts",
    "src/remote-config.ts",
    "src/remote-launch-run.ts",
    "src/remote-launch.ts",
    "src/tmux-sessions.ts",
  ],
};

const SPLIT = Object.keys(DEPS).filter((f) => f !== "src/tabs.ts");

/** 渲染栈（逐条渲染 · 卡片 · 流 · 时间线 · 折叠层）。 */
const RENDER_STACK = [
  "src/render-stream-record.ts",
  "src/cards/index.ts",
  "src/stream.ts",
  "src/record-timeline.ts",
  "src/branch-fold.ts",
];
const INVOKE = "npm:@tauri-apps/api/core";
/** 纯模块：零 DOM 全局。 */
const PURE = [
  "src/tab-model.ts",
  "src/tab-drop.ts",
  "src/tab-store.ts",
  "src/tab-router.ts",
  "src/tab-session-facts.ts",
];

describe("〔U2〕拆 tabs.ts：每一份的直接运行期依赖 == 登记（两向）", () => {
  it("★ 抽取器自检：一份已知有 import 的文件抽得出来，一份 type-only 的抽成空（否则下面整张表在空转）", () => {
    expect(runtimeImports("src/tab-stream-view.ts")).toContain("src/render-stream-record.ts");
    expect(runtimeImports("src/tab-model.ts")).toEqual([]);
    // type-only 两种写法都要排掉；值 import 与 re-export 都要算。
    const probe = [
      'import type { A } from "./tab-store";',
      'import { type B } from "./tab-drop";',
      'import { C, type D } from "./tab-router";',
      'export { E } from "./tab-menu";',
      'export type { F } from "./tab-bar-view";',
      '// import { G } from "./tab-bar-drag";',
    ].join("\n");
    expect(runtimeImports("src/tabs.ts", probe)).toEqual(["src/tab-menu.ts", "src/tab-router.ts"]);
  });

  it("★ 登记的份数 == 盘上 src/tab-*.ts ＋ tabs.ts 里「U2 拆出来的」那几份（拆出去的每一份都得进表）", () => {
    // `tab-collections.ts` / `tab-bar-state.ts` 是拆之前就在的两份（盘上那一层），不在这张表里。
    const src = read("src/tabs.ts");
    const header = src.slice(0, src.indexOf("*/")); // 只认文件头那一块注释（拆分地图）
    const named = new Set([...header.matchAll(/`(tab-[a-z-]+\.ts)`/g)].map((m) => `src/${m[1]}`));
    expect([...named].sort(), "tabs.ts 头注拆分地图里点名的份 == 登记表里拆出来的份").toEqual(
      [...SPLIT].sort(),
    );
  });

  for (const [file, want] of Object.entries(DEPS)) {
    it(`${file}`, () => {
      expect(runtimeImports(file), `${file} 的直接运行期依赖变了 —— 回登记表写清为什么这一份需要它`).toEqual(
        [...want].sort(),
      );
    });
  }
});

describe("〔U2〕分工的三条边界（零命中 ＋ 同一谓词的正控）", () => {
  it("★ 直呼 invoke 只在 tab-session-actions.ts", () => {
    const hits = SPLIT.filter((f) => runtimeImports(f).includes(INVOKE));
    expect(hits, "正控：会话动作那一份必须命中（否则谓词拼错了）").toContain("src/tab-session-actions.ts");
    expect(hits).toEqual(["src/tab-session-actions.ts"]);
    expect(runtimeImports("src/tabs.ts"), "组装根也不许直呼").not.toContain(INVOKE);
  });

  it("★ 渲染栈只在 tab-stream-view.ts（组装根除外）", () => {
    const hits = SPLIT.filter((f) => runtimeImports(f).some((d) => RENDER_STACK.includes(d)));
    expect(hits).toEqual(["src/tab-stream-view.ts"]);
    // 正控：流视图五样全都命中。
    expect(RENDER_STACK.every((d) => runtimeImports("src/tab-stream-view.ts").includes(d))).toBe(true);
  });

  it("★ 纯模块一处 document. / window. 都不碰", () => {
    const dom = (rel: string): number =>
      (stripComments(read(rel), "ts").match(/\b(?:document|window)\./g) ?? []).length;
    expect(dom("src/tab-bar-view.ts"), "正控：tab 栏视图必然碰 DOM（否则谓词拼错了）").toBeGreaterThan(0);
    expect(Object.fromEntries(PURE.map((f) => [f, dom(f)]))).toEqual(
      Object.fromEntries(PURE.map((f) => [f, 0])),
    );
  });
});

describe("〔U2〕tabs.ts 只剩组装根", () => {
  const code = stripComments(read("src/tabs.ts"), "ts");

  it("★ 顶层声明 == { TabManager }", () => {
    const decls = [
      ...code.matchAll(
        /^(?:export\s+)?(?:default\s+)?(?:abstract\s+)?(?:class|function|const|let|var|interface|type|enum)\s+([A-Za-z_$][\w$]*)/gm,
      ),
    ].map((m) => m[1]);
    expect(decls).toEqual(["TabManager"]);
  });

  it("★ 导出面 == 拆之前的 import 面（逐名两向相等）", () => {
    const names = new Set<string>();
    for (const m of code.matchAll(/^export\s+(?:type\s+)?\{([^}]*)\}/gm)) {
      for (const n of m[1].split(",").map((s) => s.trim().replace(/^type\s+/, "")).filter(Boolean)) {
        names.add(n);
      }
    }
    for (const m of code.matchAll(/^export\s+class\s+([A-Za-z_$][\w$]*)/gm)) names.add(m[1]);
    expect([...names].sort()).toEqual(
      [
        "TabManager",
        // tab-model.ts
        "Tab",
        "TabStatus",
        "TabsSummary",
        // tab-drop.ts
        "moveTabBlock",
        "pickDropTarget",
        "tabUnderY",
        "commonDirName",
        "defaultGroupName",
        "applyDropToCollections",
        "collectionsEqual",
        "DWELL_MS",
        "DWELL_MOVE_PX",
        "DropTarget",
        "TabRect",
        // tmux-sessions.ts（G6 起就是 re-export）
        "findClaudeTmuxMatches",
        "findClaudeTmux",
        "findIdleTmux",
        "isCwdFallbackMatch",
        "claudeExited",
        "TmuxSession",
      ].sort(),
    );
  });
});
