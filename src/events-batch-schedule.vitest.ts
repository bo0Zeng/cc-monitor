/**
 * audit-0805 F17 下半：`events.ts` 的**批量调度状态机**三条分支。
 *
 * # 为什么这三条排在最后
 *
 * F17 上半给 `events.ts` 补了第一条运行期判据（突发哨兵），把它从 0% 抬到 53.33%。
 * 剩下的三条 —— **批量调度 / 哨兵配对 / 300ms grace 续期** —— 功能件 §8 逐字记着
 * 「仍未做：它们要控时间（假计时器 + `snapshotInflight` 状态机），与本轮这条不是同一类活」。
 * 本文件就是那一类活。
 *
 * # 它们凭什么值得钉
 *
 * 这台状态机决定**整个重放期前端是 batch 还是 live**：
 * batch 期 `BranchFolder` 只 push 不算、代码块走 lazy hljs、渲染走 defer 路径。
 * 判错一次的后果不是「慢一点」，是**几千条历史逐条走 live 全量渲染**
 * （`events.ts:150-152` 逐字记着那条已确诊的用户后果）。
 *
 * ★ 而这三条分支里有一条是 **5 分钟防呆上限** —— 它只在「后端 inflight 计数卡死」
 * 时才走到，真机上大概从没执行过一次。**这种分支正是覆盖率空白里最危险的那类**：
 * 它写下来就是为了兜底，而兜底的路自己没被验过。
 *
 * # 判据钉什么
 *
 * 钉**时序与次数**：onBatchStart/onBatchEnd 各被调了几次、在第几毫秒。
 * 不钉「渲染快不快」（那要真机）。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { REPO_ROOT } from "./test-support/repo-root.ts";
import { productionTsFiles } from "./test-support/production-sources.ts";

type Cb = (e: { payload: unknown }) => void;
const subs = new Map<string, Cb>();

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, cb: Cb) => {
    subs.set(event, cb);
    return Promise.resolve(() => {});
  }),
}));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({
    listen: vi.fn((event: string, cb: Cb) => {
      subs.set(event, cb);
      return Promise.resolve(() => {});
    }),
  }),
}));
// ⚠ 必须返回 **Promise**：`emitPerfSummary` 里是
// `void commands.frontend_perf_log({…}).catch(() => {})`（`events.ts:516`），
// 而它只在 **onBatchEnd 真正 fire 之后**才被调到。
// ★ 同仓的 `events-burst.vitest.ts` 用的是 `get: () => vi.fn()`（返回 `undefined`）——
// 那个洞一直在，只是它从没跑到这条路。**一条 mock 在它没跑到的输入上是什么行为，
// 不能靠「它一直是绿的」推断**（F18 下半刚在 `digit_after` 上栽过同一跤）。
vi.mock("./ipc/commands", () => ({
  commands: new Proxy({}, { get: () => vi.fn().mockResolvedValue(undefined) }),
}));

import { bindEvents } from "./events";

const payload = (seq: number): unknown => ({
  session_id: "s",
  cwd: "/p",
  path: "/p/s.jsonl",
  seq,
  message: { type: "assistant", uuid: `u-${seq}` },
});

interface Harness {
  onBatchStart: ReturnType<typeof vi.fn>;
  onBatchEnd: ReturnType<typeof vi.fn>;
  onLine: ReturnType<typeof vi.fn>;
  /** 发一块 jsonl-batch（`chunkIndex === 0` 才带 batch-start 哨兵）。 */
  chunk: (chunkIndex: number, seqs: number[]) => void;
  /** 发一条裸 jsonl-line。 */
  line: (seq: number) => void;
  /** 后端的 snapshot-inflight 计数。 */
  inflight: (count: number) => void;
}

async function bind(): Promise<Harness> {
  subs.clear();
  const onBatchStart = vi.fn();
  const onBatchEnd = vi.fn();
  const onLine = vi.fn();
  await bindEvents({
    onLine,
    onSessionEnded: vi.fn(),
    onBatchStart,
    onBatchEnd,
  } as never);
  // 抽取器自检：三条通道少订一条，下面全是零命中地绿。
  for (const ch of ["jsonl-batch", "jsonl-line", "snapshot-inflight"]) {
    expect(subs.get(ch), `没订到 \`${ch}\` —— 本文件会零命中地绿（检查 listen 的 mock）`).toBeTruthy();
  }
  return {
    onBatchStart,
    onBatchEnd,
    onLine,
    chunk: (chunkIndex, seqs) =>
      subs.get("jsonl-batch")!({
        payload: { chunkIndex, chunkTotal: 2, payloads: seqs.map(payload) },
      }),
    line: (seq) => subs.get("jsonl-line")!({ payload: payload(seq) }),
    inflight: (count) => subs.get("snapshot-inflight")!({ payload: { count } }),
  };
}

describe("events.ts 批量调度状态机（audit-0805 F17 下半的三条分支）", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  // ── 分支 ①：批量调度 ────────────────────────────────────────────────
  it("★ 一块 batch → 立刻进 batch 模式，300ms 之后才退出", async () => {
    const h = await bind();
    h.chunk(0, [1, 2, 3]);
    await vi.advanceTimersByTimeAsync(0);
    expect(h.onBatchStart, "收到第一块就该进 batch 模式").toHaveBeenCalledTimes(1);
    expect(h.onLine, "三条 payload 该被 drain 出去").toHaveBeenCalledTimes(3);

    await vi.advanceTimersByTimeAsync(299);
    expect(
      h.onBatchEnd,
      "grace 还没满就退出 batch 模式了 —— 后续块会被当成 live 逐条渲染，" +
        "那正是 300ms grace 存在的理由（`events.ts:150-162`）",
    ).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(2);
    expect(h.onBatchEnd, "grace 满了却没退出 —— batch 模式会永远压着").toHaveBeenCalledTimes(1);
  });

  // ── 分支 ②：哨兵配对 ───────────────────────────────────────────────
  it("★ 已在 batch 模式再来一块 `chunkIndex===0` → 不许重复 onBatchStart", async () => {
    const h = await bind();
    h.chunk(0, [1]);
    await vi.advanceTimersByTimeAsync(0);
    h.chunk(0, [2]); // 第二块又带 batch-start 哨兵
    await vi.advanceTimersByTimeAsync(0);
    expect(
      h.onBatchStart,
      `onBatchStart 被调了 ${h.onBatchStart.mock.calls.length} 次。` +
        "`enterBatchMode` 的 `if (inBatchMode) return` 是幂等闸 —— 它一没，" +
        "TabManager 会在重放中途再走一遍 batch 进入路径（`BranchFolder.setBatchMode` 重入）。",
    ).toHaveBeenCalledTimes(1);
    // 反向：两块的 payload 都得到（幂等闸不许把第二块吃掉）
    expect(h.onLine, "第二块的 payload 丢了 —— 幂等闸挡的该是哨兵，不是数据").toHaveBeenCalledTimes(2);
  });

  // ── 分支 ③：300ms grace 续期 ───────────────────────────────────────
  it("★ grace 窗口内来 payload → 续期，不提前退出", async () => {
    const h = await bind();
    h.chunk(0, [1]);
    await vi.advanceTimersByTimeAsync(0);

    await vi.advanceTimersByTimeAsync(200);
    h.line(9); // 距上次排程 200ms，仍在窗口内
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(200); // 累计 400ms —— 没续期的话早退出了
    expect(
      h.onBatchEnd,
      "累计 400ms 就退出了 —— grace 没有被那条 payload 续期。" +
        "多块切片场景下每块间隔 >300ms 时会反复进出 batch 模式。",
    ).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(150);
    expect(h.onBatchEnd, "续期之后再等满 300ms 该退出了").toHaveBeenCalledTimes(1);
  });

  it("★ `snapshotInflight > 0` → 只续期不退出；归零后正常收尾", async () => {
    const h = await bind();
    h.inflight(2); // 后端说：还有 2 个快照在途
    h.chunk(0, [1]);
    await vi.advanceTimersByTimeAsync(0);

    await vi.advanceTimersByTimeAsync(1000);
    expect(
      h.onBatchEnd,
      "在途计数 >0 却退出了 batch 模式 —— 慢链路回填的 chunk 间隔 >300ms 时," +
        "旧历史会以 live 形态插进时间线中段（`events.ts:204-210` 记的那个乱序风险点）。",
    ).not.toHaveBeenCalled();

    h.inflight(0);
    await vi.advanceTimersByTimeAsync(400);
    expect(h.onBatchEnd, "归零之后下一次定时器该正常收尾").toHaveBeenCalledTimes(1);
  });

  // ── audit-0805 §5 2v：钉住一次**真实发生过**的事故形态 ──────────────────
  //
  // `events.ts:313-317` 那个 `catch` 的注释逐字记着：
  // 「v2.1.0 踩过：computeMainBranch stack overflow → drain 异常逃逸 → **后续上千条
  // record 永远不渲染**」。v2.1.1 加了这道 try/catch 兜住它。
  //
  // ★ 而它**今天没有任何东西验它** —— 那一行在覆盖率里是零执行。
  // 一道「防止整条队列冻死」的护栏自己没被验过，正是本区一直在治的形状。
  //
  // ⚠ 本组不追覆盖率数字：`events.ts` 还有十几行没覆盖（各类 session-* 分支），
  // 那些是**转发**，错了看得见；这一条不同 —— 它错了的表现是**后面什么都不来了**。
  // ⚠ 变异复验时的一个细节，写下来免得下次误读：拿掉那个 `catch` 之后，本条**不是**靠
  // 我写的断言红的，而是**异常直接逃到 vitest**（`Error: computeMainBranch 炸了`）。
  // 那也是一次干净的击杀 —— 但**诊断文案不是我的**。若将来它改成「吞掉但不继续」，
  // 才会走到下面那句「四条 payload 只走到第 N 条」。两种红都要认得。
  it("★ 单条 handler 抛异常 → 后面的行照常处理（drain 不冻死）", async () => {
    const h = await bind();
    let calls = 0;
    h.onLine.mockImplementation(() => {
      calls += 1;
      if (calls === 2) throw new Error("computeMainBranch 炸了（模拟 v2.1.0）");
    });
    const err = vi.spyOn(console, "error").mockImplementation(() => {});

    h.chunk(0, [1, 2, 3, 4]);
    await vi.advanceTimersByTimeAsync(0);

    expect(
      calls,
      `四条 payload 只走到第 ${calls} 条 —— **异常逃逸出 drain 了**。\n` +
        "★ 这正是 v2.1.0 那次事故：一条 record 出错，后续上千条永远不渲染。\n" +
        "v2.1.1 的 try/catch 就是为它加的（`events.ts:313-317`）。",
    ).toBe(4);
    expect(
      err.mock.calls.flat().join(" "),
      "吞了异常却没留痕迹 —— 那是**静默失败**（定框 E4）。丢一条 record 可以，" +
        "但要说得出丢了哪一条。",
    ).toContain("handler threw");
    await vi.advanceTimersByTimeAsync(400);
    expect(h.onBatchEnd, "出过错之后 batch 模式还要能正常收尾").toHaveBeenCalledTimes(1);
  });

  it("★ onBatchStart / onBatchEnd 自己抛异常 → 状态机不许卡住", async () => {
    const h = await bind();
    h.onBatchStart.mockImplementation(() => {
      throw new Error("TabManager 进 batch 时炸了");
    });
    h.onBatchEnd.mockImplementation(() => {
      throw new Error("TabManager 出 batch 时炸了");
    });
    const err = vi.spyOn(console, "error").mockImplementation(() => {});

    h.chunk(0, [1, 2]);
    await vi.advanceTimersByTimeAsync(0);
    expect(h.onLine, "onBatchStart 抛了之后 payload 就不 drain 了 —— 异常逃出了状态机").toHaveBeenCalledTimes(2);

    await vi.advanceTimersByTimeAsync(400);
    expect(h.onBatchEnd, "grace 满了却没试着收尾").toHaveBeenCalledTimes(1);

    // ★ 关键：onBatchEnd 抛了之后**还要能再进一次 batch** —— 否则 inBatchMode 卡在 true，
    // 后面每一块历史都会被当成 live 逐条渲染（F15 治的正是那个代价）。
    h.chunk(0, [3]);
    await vi.advanceTimersByTimeAsync(0);
    expect(
      h.onBatchStart,
      "第二块没能重新进 batch 模式 —— `inBatchMode` 多半卡在 true 了。" +
        "那之后每块历史都走 live 逐条渲染。",
    ).toHaveBeenCalledTimes(2);
    expect(err.mock.calls.flat().join(" ")).toContain("threw");
  });

  it("★★ 在途计数**卡死**时，5 分钟防呆上限必须真的踢开（这条分支真机上多半从没跑过）", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const h = await bind();
    h.inflight(3); // 卡在非零，永不归零
    h.chunk(0, [1]);
    await vi.advanceTimersByTimeAsync(0);

    await vi.advanceTimersByTimeAsync(60_000);
    expect(h.onBatchEnd, "才 1 分钟就放弃续期了 —— 上限不是 5 分钟").not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(5 * 60_000);
    expect(
      h.onBatchEnd,
      "超过 5 分钟上限仍在续期 —— 后端计数卡死会让前端**永久压在 batch 模式**：" +
        "新消息不出现在时间线上，而且没有任何报错。防呆上限就是为这个写的。",
    ).toHaveBeenCalledTimes(1);
    expect(
      warn.mock.calls.flat().join(" "),
      "强制清零时没有留下痕迹 —— 这是「静默失败要给身份」那条（定框 E4）：" +
        "被上限踢开说明后端计数出过问题，得有人看得见。",
    ).toContain("snapshot-inflight");
  });
});

// ★★ **`bindEvents` 的每一处调用都必须 `await`**〔audit-0805 08-08，Phase G 第 89 件〕。
//
// # 症状是实测过的，而没人钉着它
//
// `events.ts` 与 `main.ts` 两侧都逐字写着同一件事：
//
// > **必须 await**：listener 注册完成前调 replay 会丢事件（**实测白屏只剩状态栏**）。
// > 不 await 注册就会把 1599 条历史全丢。
//
// 而 TypeScript **不会**因为漏 `await` 报错（`bindEvents` 是 `async`，丢弃返回的 Promise
// 完全合法），eslint 那步又带 `|| true`（CI 步骤表里逐字登记着「结构上不会红」）。
// ⇒ 删掉一个 `await`：tsc 绿、vitest 绿、启动时历史全丢、用户看到一屏空白。
//
// ⚠ 08-08 的「顺序」透镜只扫了 Rust（31 处声称逐条查过），**前端那一侧一条都没扫**——
// 这条是补上的第一块。人群从源码派生（非测试的 `src/**.ts` 里每一处调用）。
describe("bindEvents 的接线", () => {
  const callSites = (() => {
    // ⚠ **不自己再写一份遍历**：`scanning-guard-registry` 的递减棘轮当场拦过（9→10），
    // 它要的答案是「你靠什么读不到自己」。共享 helper 的答案是**按构造**：
    // 只收生产文件（排掉 `.vitest.`/`.test.`），而判据都住在测试文件里。
    const out: Array<{ file: string; line: number; text: string }> = [];
    for (const { file, text } of productionTsFiles("src")) {
      text.split("\n").forEach((l, i) => {
        const s = l.trim();
        // 只认**调用**，不认定义行与文档注释。
        if (!s.includes("bindEvents(")) return;
        if (s.startsWith("*") || s.startsWith("//") || s.includes("function bindEvents")) return;
        out.push({ file, line: i + 1, text: s });
      });
    }
    return out;
  })();

  it("每一处调用都带 await（漏一个 = 启动时历史全丢，实测白屏只剩状态栏）", () => {
    expect(
      callSites.length,
      "一处 `bindEvents(` 调用都没扫到（08-08 实测 2 处，都在 main.ts）—— 遍历或过滤坏了，本条此刻无效",
    ).toBeGreaterThanOrEqual(2);
    const bare = callSites.filter((c) => !c.text.startsWith("await ") && !c.text.includes("= await "));
    expect(
      bare.map((c) => `${c.file}:${c.line}: ${c.text}`),
      "这些 `bindEvents` 调用没有 await：\n" +
        bare.map((c) => `  ${c.file}:${c.line}`).join("\n") +
        "\n★ 后果是实测过的：listener 注册完成前 replay 就发出去了 ⇒ 历史事件全丢，\n" +
        "  用户看到一屏空白（只剩状态栏）。`events.ts` 与 `main.ts` 两侧的注释都逐字写着这件事。\n" +
        "⚠ 编译器接不住：`bindEvents` 是 async，丢弃它返回的 Promise 完全合法；\n" +
        "  eslint 那步又带 `|| true`（CI 步骤表里登记着「结构上不会红」）。",
    ).toEqual([]);
  });

  // ⚠ **如实记这条腿的性质**〔08-08 变异实测〕：最小变异（去掉 `async`）**编不过** ——
  // 函数体里有 `await`，编译器先拦下了，vitest 连收集都做不到。
  // 也就是说本条挡的**不是**那一刀，而是「有人把注册真改成同步」那种**成套重构**：
  // 那时它会红，逼人回来重判上面那条 await 判据还成不成立。
  // 按铁律 13 不删（「难造变异」不是删除依据），但也不假称它被变异验过。
  it("前提：`bindEvents` 仍然是 async（不然 await 这件事本身就没意义）", () => {
    const src = readFileSync(resolve(REPO_ROOT, "src/events.ts"), "utf8");
    expect(
      src,
      "`bindEvents` 不再是 `async function` —— 上面那条在钉一件可能已经不成立的事。\n" +
        "若它改成同步（注册在返回前完成），把上面那条一起改掉，别留着一条空转的判据。",
    ).toContain("export async function bindEvents");
  });
});

// ★★ **启动接线：先读「上次活跃」的记忆，再建骨架**〔audit-0805 08-08，Phase G 第 90 件〕。
//
// `main.ts` 逐字写着这件事与它的后果：
//
// > **先读记忆再建骨架** —— 第一个骨架的自动切换会经 `switchTo` 写回 localStorage，
// > 读晚了就把用户记忆覆写成清单首个 sid（**F19 主路径在「本地有会话」的常见场景下
// > 整体失效**）。骨架期同时抑制写回双保险。
//
// 而没人钉它：`tabs.vitest.ts` 钉的是**单元**（`switchTo` 写回 + `persistLastActive` 开关），
// 而这件事是 `main.ts` 里的**接线顺序**；`main.ts` 又正好在 0% 覆盖那一族里
//（`scripts/assert-coverage-floors.mjs` 的 `ZERO_TODAY` 第一行就是它）。
// ⇒ 把 `safeGet` 挪到建骨架之后、或删掉那句抑制写回：**tsc 绿、vitest 绿，而「恢复上次
// 活跃 tab」在最常见的场景下静默失效**。
//
// # 为什么是源码层判据
//
// 行为层要把整个 `main()` 启动路径跑起来（Tauri invoke + DOM + localStorage 全套）。
// **如实说**：本条判的是「那三行的先后」，挡得住「有人把读记忆挪到骨架之后 / 删掉抑制」，
// **挡不住**「`switchTo` 自己改了写回时机」——那半由 `tabs.vitest.ts` 那条单元判据看着。
//
// ⚠⚠ **锚点第一版就踩了自己刚立的元判据**：我拿 `createSkeletonTab(` 当「建骨架」的锚，
// 而它在 `main.ts` 里有 **3 处**，最早两处是**事件处理器体内**的（`remote-session-added` 那批，
// 注册在前、**执行在后**）⇒ 判据当场红，说「读记忆排到建骨架之后了」——**红对了位置、讲错了事**。
// 这正是 08-08 那条元判据点名的两种坏法叠在一起：**比的是任意一处**（坏法②）+
// **文本顺序 ≠ 执行顺序**（坏法③）。
// ⇒ 改锚**启动那一批**本身：`commands.list_active_sessions()`（生产段唯一一处，
// 它下面那个 for 循环才是启动骨架）。三条纪律照旧：剥注释 · 不兜任意一处 · 每个锚点先核唯一性。
describe("启动接线：记忆与骨架的先后", () => {
  const code = (() => {
    const src = productionTsFiles("src").find((f) => f.file.endsWith("src/main.ts"));
    if (!src) throw new Error("扫不到 src/main.ts —— 遍历坏了，本条会零命中地绿");
    return src.text
      .split("\n")
      .filter((l) => {
        const s = l.trim();
        return !s.startsWith("//") && !s.startsWith("*") && !s.startsWith("/*");
      })
      .join("\n");
  })();

  const at = (needle: string, want: number): number => {
    const n = code.split(needle).length - 1;
    expect(n, `\`${needle}\` 在 main.ts 生产段出现 ${n} 次（08-08 实测 ${want}）—— 锚点漂了，先修锚点`).toBe(want);
    return code.indexOf(needle);
  };

  it("读 last-active 记忆排在启动骨架批之前", () => {
    const read = at("safeGet(LS_KEYS.lastActiveSid)", 1);
    const skeleton = at("commands.list_active_sessions()", 1);
    expect(
      read < skeleton,
      "读记忆排到建骨架之后了。\n" +
        "★ 后果 main.ts 自己写着：第一个骨架的自动切换会经 switchTo 写回 localStorage，\n" +
        "  读晚了就把用户记忆覆写成清单首个 sid ⇒ **F19「恢复上次活跃 tab」在最常见的\n" +
        "  场景（本地有会话）下整体失效**，而 tsc / vitest 全绿。",
    ).toBe(true);
  });

  it("骨架期抑制写回：`persistLastActive=false` 在前、恢复 `=true` 在后", () => {
    // ⚠ **别让计数自检抢在真故事前面**〔08-08 变异实测〕：第一版写成 `at(…, 2)`，
    // 于是「把骨架期那句抑制删掉」这一刀红在「锚点漂了，先修锚点」上 ——
    // **红对了，但会把人引去查抽取器**，而真事是「抑制没了」。本会话第 N 次同形。
    // ⇒ 计数放宽成范围自检（只挡「一处都找不到」这种抽取器塌掉），
    // 先后与存在性由下面两条各自讲自己的故事。
    const offAll = code.split("tabs.persistLastActive = false").length - 1;
    expect(
      offAll,
      `\`tabs.persistLastActive = false\` 一处都找不到（08-08 实测 2 处：骨架期 + viewer 窗口）——
` +
        "抽取器塌了，或者**抑制写回这件事整个不存在了**。先看 main.ts 再谈顺序。",
    ).toBeGreaterThanOrEqual(1);
    const off = code.indexOf("tabs.persistLastActive = false");
    const skeleton = at("commands.list_active_sessions()", 1);
    const on = at("tabs.persistLastActive = true", 1);
    expect(
      off < skeleton,
      "骨架期的抑制写回不在建骨架之前了（被挪走或删掉）。\n" +
        "★ 那是 main.ts 说的「双保险」那一半：骨架期的自动切换会经 switchTo 把记忆\n" +
        "  写成清单首个 sid —— 读记忆那半即使还在前面，也会被这一路覆写掉。",
    ).toBe(true);
    expect(
      skeleton < on,
      "恢复 `persistLastActive = true` 排到了建骨架之前 —— 抑制窗口空了，等于没抑制。",
    ).toBe(true);
  });
});
