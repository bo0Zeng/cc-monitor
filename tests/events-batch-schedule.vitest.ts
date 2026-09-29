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
vi.mock("../src/ipc/commands", () => ({
  commands: new Proxy({}, { get: () => vi.fn().mockResolvedValue(undefined) }),
}));
// 〔CF2 · 第四波 4B〕会话内容从通道 `subscribe` 来：换成桩，按句柄的形状灌（`test-support/chan-stream-fake.ts`）。
vi.mock("../src/comms/inward/chan", async () => (await import("./test-support/chan-stream-fake.ts")).chanStreamModule);

import { bindEvents } from "../src/events";
import { streamFake } from "./test-support/chan-stream-fake.ts";

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
  /** 发成批那一段的一块（〔CF2〕`chunkIndex === 0` 才以 `batch:start` 开头；每块以 `batch:end` 收尾）。 */
  chunk: (chunkIndex: number, seqs: number[]) => void;
  /** 发一条逐行来的实时格。 */
  line: (seq: number) => void;
  /** 后端的 snapshot_inflight 格 计数。 */
  inflight: (count: number) => void;
}

async function bind(): Promise<Harness> {
  subs.clear();
  streamFake.reset();
  const onBatchStart = vi.fn();
  const onBatchEnd = vi.fn();
  const onLine = vi.fn();
  await bindEvents({
    onLine,
    onSessionEnded: vi.fn(),
    onBatchStart,
    onBatchEnd,
  } as never, { streams: [{ origin: "<local>", kind: "session-lines" }] });
  // 抽取器自检：会话流少订一条，下面全是零命中地绿。〔MIG-1〕snapshot-inflight 并进会话流（`{"snapshot_inflight": …}` 那一格）。
  expect(streamFake.subscriptions.length, "没订到会话流 —— 本文件会零命中地绿").toBe(1);
  return {
    onBatchStart,
    onBatchEnd,
    onLine,
    chunk: (chunkIndex, seqs) => streamFake.chunk(chunkIndex === 0, seqs.map(payload)),
    line: (seq) => streamFake.lines([payload(seq)]),
    inflight: (count) => streamFake.lifecycle([{ snapshot_inflight: { count } }]),
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

// ═══════════════════════════════════════════════════════════════════════
// 〔CF2 · 第四波 4B〕会话流那一段：credit 怎么还 · `gap` 怎么进队
//
// 要求住址：`设计/05 §3.3.4`「级 1 · 回推：订阅方不取 ⇒ 通信层不读」（这一侧的队列上界 = 给出去的 credit，
// 处理掉一格还一格）· 「`Gap` 必须在流里的原位」（进 queue 与行保序）。
// ═══════════════════════════════════════════════════════════════════════
describe("〔CF2〕会话流：还 credit 与丢格", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("★ 处理掉几格就还几格（按 drain 一片还一次，不是逐格一次 IPC）；批边界也算格；读不懂的一格也还", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    const h = await bind();
    const rec = streamFake.subscriptions[0];
    h.chunk(0, [1, 2, 3]); // start + 3 行 + end = 5 格
    rec.sink([{ t: "frame", seq: 99, body: "不是 JSON" }]); // 读不懂：跳过，但照还
    await vi.advanceTimersByTimeAsync(0);
    expect(h.onLine).toHaveBeenCalledTimes(3);
    expect(
      rec.wants.reduce((a, b) => a + b, 0),
      "处理掉 6 格（含两个批边界与一格读不懂的），还回去的 credit 不等于 6 —— 窗口会越用越小直到停摆",
    ).toBe(6);
    expect(rec.wants.length, "一片 drain 里逐格还 credit（每格一次 IPC）").toBeLessThanOrEqual(2);
  });

  it("★ `gap` 进 queue、排在它之前到的行之后：宿主收到 `onStreamGap(那台机器)` 时，前面的行都已交给它", async () => {
    subs.clear();
    streamFake.reset();
    const order: string[] = [];
    await bindEvents(
      {
        onLine: (p: { seq: number }) => order.push(`line ${p.seq}`),
        onSessionEnded: vi.fn(),
        onBatchStart: vi.fn(),
        onBatchEnd: vi.fn(),
        onStreamGap: (o: string) => order.push(`gap ${o}`),
      } as never,
      { streams: [{ origin: "box", kind: "session-lines" }] },
    );
    streamFake.lines([payload(1), payload(2)]);
    streamFake.gap(2, 5);
    streamFake.lines([payload(6)]);
    await vi.advanceTimersByTimeAsync(0);
    expect(order).toEqual(["line 1", "line 2", "gap box", "line 6"]);
  });
});

// ★ S6〔CF2 · 第四波 4B〕：会话内容的旧路（两个广播事件 ＋ 独立窗口的定向重放命令）在前端生产段**零命中**。
//
// 要求住址：`设计/05 §8` 步 6「流那半收口成 `subscribe`」—— 旧路留着一处订阅，会话内容就有了第二个入口
// （而且那个入口没有 credit、丢了也不报 `Gap`）。正控：同一识别器在合成代码上三针全中、在注释里不中。
describe("〔CF2〕会话内容的旧路退役", () => {
  const needles = [`"jsonl-${"line"}"`, `"jsonl-${"batch"}"`, `replay_session_${"to_window"}`];
  const strip = (t: string): string =>
    t
      .split("\n")
      .filter((l) => !/^\s*(\/\/|\*|\/\*)/.test(l))
      .join("\n");
  const hits = (t: string): string[] => needles.filter((n) => strip(t).includes(n));
  it("★ S6：前端生产段零命中（正控三针全中；注释不算）", () => {
    expect(hits(`listen("jsonl-${"line"}"); sub("jsonl-${"batch"}"); invoke("replay_session_${"to_window"}");`)).toHaveLength(3);
    expect(hits(`// 原来的 "jsonl-${"line"}" 退役了`)).toEqual([]);
    const files = productionTsFiles("src");
    expect(files.length, "前端生产语料一份都没扫到 —— 本条零命中地绿").toBeGreaterThan(100);
    const offenders = files.flatMap(({ file, text }) => hits(text).map((n) => `${file}: ${n}`));
    expect(offenders, "会话内容的旧路又长回来了（该经 `chan.subscribe`，`events.ts` 的 `streams`）").toEqual([]);
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
//（`tests/scripts/assert-coverage-floors.mjs` 的 `ZERO_TODAY` 第一行就是它）。
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
// 而它在 `main.ts` 里有 **3 处**，最早两处是**事件处理器体内**的（远端 `live` 格 那批，
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

  // 〔MIG-1 · ⑬〕本机骨架也挪进就绪点（会话流里的 `live` 成品）：「启动骨架批」这个锚没了，换成「就绪点」`frontend-ready`；
  //   抑制写回那一对随之删了（它罩的那批骨架不在这里建了）⇒ 那条判据换成「两处骨架入口都按 pending 补切」。
  it("读 last-active 记忆排在就绪点（骨架从那里才来）之前", () => {
    const read = at("safeGet(LS_KEYS.lastActiveSid)", 1);
    const ready = at('void emit("frontend-ready"', 1);
    expect(
      read < ready,
      "读记忆排到就绪点之后了 —— 骨架一到就可能经 switchTo 写回，记忆会被覆写成先到的那个 sid。",
    ).toBe(true);
  });

  // 〔MIG-1 续〕补切交给 `startup-active.ts`（它自己的真值表住 `tests/startup-active.vitest.ts`）：两处骨架入口各报一次「出现了」。
  it("本机与远端两处骨架入口都按 pending 补切上次所在 tab", () => {
    at("startup?.onAppeared(sessionId);", 2);
  });
});

