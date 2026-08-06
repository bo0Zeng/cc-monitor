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
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

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
