/**
 * 「起会话的真成功正信号：只有看见那台后端报出这个会话才说起来了；预算内没见到 ⇒ 说命令发出去了，
 * 但没看到会话起来，并给出启动器那一行的退出原话，不报成功」。
 *
 * 起会话的各条路（`remote-launch-run.ts` 的执行器 · `local-resume.ts` · 历史页起新会话 · cc-bus 派生）把命令发出去之后
 * 不再自己说「起来了」，而是交一件「等它」（[`expectArrival`]）。等的那一方只有一个：**主窗口**（它订着每台机器的会话流，
 * 设置窗 / 查看窗订不全）⇒ 预期经窗口间事件交过去（同 `settings/events.ts` 那两条跨窗事件的做法），主窗口在
 * [`bindLaunchArrivals`] 里收下、在 [`noteLive`] 里对那台报上来的活会话（`live` 格）。预算到了还没见到 ⇒ 说没见到：
 * 起在 tmux 里的顺手抓那一屏当原话（`terminal-reads.ts::previewByTmuxName`），直接开窗的说原话在那个窗口里。
 *
 * 认「是不是它」只看那台报上来的事实：resume 按 sid；新开的会话（本机远端 · cc-bus 派生）按「预期之后第一次出现、
 * 工作目录相同的新 sid」。
 */
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { copyText } from "./copy-table";
import { machineName } from "./control-said";
import { showActionFailureToast } from "./error-toast";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { previewByTmuxName } from "./terminal-reads";

/** 预算：从命令发出去到那台报出会话。慢机器上 ssh 握手 ＋ claude 冷启动在这之内；过了只说「没看到」，不说失败。 */
export const ARRIVAL_BUDGET_MS = 45_000;
/** 原话取那一屏最后几行（非空行）。 */
const WORDS_LINES = 6;
/** 发起方窗口 → 主窗口：交一件「等它」（载荷 [`ArrivalSpec`]）。 */
export const LAUNCH_EXPECT_EVENT = "launch-arrival-expect";
/** 主窗口 → 发起方：带票的那一件等到了没有（载荷 `{ ticket, arrived }`）。 */
export const LAUNCH_DONE_EVENT = "launch-arrival-done";
/** 主窗口 → 发起方所在的那扇窗：等到了 / 没等到那一句也在那里说一遍（发起方常在设置窗里，主窗口被它挡着）。 */
export const LAUNCH_SAID_EVENT = "launch-arrival-said";
/** 主窗口的标签（与 Rust `MAIN_WINDOW_LABEL` 同值）。 */
const MAIN_WINDOW = "main";
/** 发起方自己的上界：主窗口没回话（不存在的形态）也不许一直挂着 —— 比预算多留 15 秒给抓屏与回话。 */
const AWAIT_CAP_MS = ARRIVAL_BUDGET_MS + 15_000;

/** 执行器交回的「等到了没有」：`unsent` = 命令没真发出去（复制回退 / 拉不起窗口，那一路自己说过了）。 */
export type LaunchWait = "arrived" | "missed" | "unsent";

/** 认它用的那一格。 */
export type ArrivalMatch = { sid: string } | { cwd: string };

export interface ArrivalSpec {
  origin: Origin;
  match: ArrivalMatch;
  /** 起在哪个 tmux 会话里（有 ⇒ 没见到时抓那一屏当原话）；`null` = 直接开的窗口。 */
  tmuxName: string | null;
  /** 见到了说的那一句；`null` = 调用方等到了自己说（换号重启 · 分叉），这里不说。 */
  arrived: { title: string; body: string } | null;
  /** 带票 ⇒ 主窗口等到 / 等不到时回一声（[`awaitArrival`]）。 */
  ticket?: string;
  /** 发起方所在的窗口（不是主窗口 ⇒ 那一句在那里也说一遍）。由 [`expectArrival`] 填。 */
  from?: string;
}

/** 那一句：主窗口里说，发起方在别的窗口 ⇒ 那里也说一遍。 */
interface Said {
  to: string;
  title: string;
  body: string;
  level: "info" | "error";
  durationMs: number;
}

function say(p: ArrivalSpec, title: string, body: string, level: "info" | "error", durationMs: number): void {
  showActionFailureToast(title, body, { level, durationMs });
  if (p.from === undefined || p.from === MAIN_WINDOW) return;
  const said: Said = { to: p.from, title, body, level, durationMs };
  emit(LAUNCH_SAID_EVENT, said).catch((e: unknown) => console.warn("[launch-arrival] 交不给发起方那扇窗：", e));
}

/** 这扇窗的标签；没有 Tauri 宿主 ⇒ `undefined`。 */
function thisWindow(): string | undefined {
  try {
    return getCurrentWindow().label;
  } catch {
    return undefined;
  }
}

/** 那台报上来的一条活会话里认它要用的那一样。 */
export interface LiveSeen {
  cwd: string | null;
}

interface Pending extends ArrivalSpec {
  /** `{cwd}` 那一格：预期那一刻这台已经报过的 sid（它们不算「新起的」）。 */
  before: Set<string>;
  timer: ReturnType<typeof setTimeout>;
}

const pending = new Set<Pending>();
/** 每台报过的活会话 sid（`{cwd}` 那一格要分新旧）。 */
const seenLive = new Map<string, Set<string>>();

const key = (o: Origin): string => (isLocalOrigin(o) ? LOCAL_ORIGIN : o);
const trimSlash = (p: string): string => (p.length > 1 ? p.replace(/\/+$/, "") : p);

/** 纯函数：这条活会话是不是那件预期要等的。 */
export function arrivalMatches(match: ArrivalMatch, sid: string, seen: LiveSeen, before: ReadonlySet<string>): boolean {
  if ("sid" in match) return match.sid === sid;
  return !before.has(sid) && seen.cwd !== null && trimSlash(seen.cwd) === trimSlash(match.cwd);
}

/** 纯函数：那一屏 ⇒ 原话（最后几行非空行）。 */
export function lastWords(screen: string): string {
  return screen
    .split("\n")
    .map((l) => l.trimEnd())
    .filter((l) => l.trim() !== "")
    .slice(-WORDS_LINES)
    .join("\n");
}

// 「{machine}上…」那一族的中西文空格（「lx 上」·「本机上」）由取文口按值补（`rules.json` C-L5，`copy-table.ts::joinSeams`），
//   这里不再另判一次。

/** 见到了那一句的正文（标题由各条路自己给）。 */
export function arrivedBody(o: Origin): string {
  return copyText("launchArrival.arrived.body", { machine: machineName(o) });
}

async function sayMissed(p: ArrivalSpec): Promise<void> {
  const secs = String(Math.round(ARRIVAL_BUDGET_MS / 1000));
  const machine = machineName(p.origin);
  let body: string;
  if (p.tmuxName === null) {
    body = copyText("launchArrival.missed.inWindow", { secs, machine });
  } else {
    try {
      const words = lastWords(await previewByTmuxName(p.origin, p.tmuxName));
      body =
        words === ""
          ? copyText("launchArrival.missed.emptyScreen", { secs, machine, name: p.tmuxName })
          : copyText("launchArrival.missed.words", { secs, machine, name: p.tmuxName, words });
    } catch (e) {
      body = copyText("launchArrival.missed.noScreen", { secs, machine, name: p.tmuxName, why: String(e) });
    }
  }
  say(p, copyText("launchArrival.missed.title"), body, "error", 15000);
}

/** 发起方（任何窗口）：命令已经发出去了 ⇒ 交主窗口等那台报出这条会话再说起来了。 */
export function expectArrival(spec: ArrivalSpec): void {
  // 交不过去（没有 Tauri 宿主）⇒ 这一趟没人等：记一笔，不替它说起没起来。
  emit(LAUNCH_EXPECT_EVENT, { ...spec, from: spec.from ?? thisWindow() }).catch((e: unknown) => console.warn("[launch-arrival] 交不给主窗口：", e));
}

/** 主窗口：收下一件「等它」。 */
export function watchArrival(spec: ArrivalSpec): void {
  const p: Pending = {
    ...spec,
    before: new Set(seenLive.get(key(spec.origin)) ?? []),
    // 一次性的预算（`polling_registry` 登记）：到点只说一次「没看到」，不重试、不轮询。
    timer: setTimeout(() => {
      if (!pending.delete(p)) return;
      void sayMissed(p);
      answer(p, false);
    }, ARRIVAL_BUDGET_MS),
  };
  pending.add(p);
}

function answer(p: ArrivalSpec, arrived: boolean): void {
  if (p.ticket === undefined) return;
  emit(LAUNCH_DONE_EVENT, { ticket: p.ticket, arrived }).catch((e: unknown) => console.warn("[launch-arrival] 回不了发起方：", e));
}

/**
 * 发起方（任何窗口）：交一件「等它」并**等主窗口回话** ⇒ 见到了 `true`、预算内没见到 `false`
 * （没见到那一句由主窗口照常说）。换号重启与分叉据它才说「已用新账号重启 / 已分叉」、才记账。
 */
export async function awaitArrival(spec: ArrivalSpec): Promise<boolean> {
  const ticket = crypto.randomUUID();
  let done: (v: boolean) => void = () => {};
  const got = new Promise<boolean>((res) => (done = res));
  let un: () => void;
  try {
    un = await listen<{ ticket: string; arrived: boolean }>(LAUNCH_DONE_EVENT, (e) => {
      if (e.payload.ticket === ticket) done(e.payload.arrived);
    });
  } catch (e) {
    // 听不了回话（没有 Tauri 宿主）⇒ 等不到：照「没看到」算，不说成起来了。
    console.warn("[launch-arrival] 听不了主窗口的回话：", e);
    return false;
  }
  const cap = setTimeout(() => done(false), AWAIT_CAP_MS);
  try {
    expectArrival({ ...spec, ticket });
    return await got;
  } finally {
    clearTimeout(cap);
    un();
  }
}

/** 主窗口：那台的会话流报出了一条活会话（`main.ts` 的本机 / 远端两个入口都交这里）。 */
export function noteLive(origin: Origin, sid: string, seen: LiveSeen): void {
  const k = key(origin);
  for (const p of pending) {
    if (key(p.origin) !== k || !arrivalMatches(p.match, sid, seen, p.before)) continue;
    clearTimeout(p.timer);
    pending.delete(p);
    if (p.arrived !== null) say(p, p.arrived.title, p.arrived.body, "info", 6000);
    answer(p, true);
  }
  let s = seenLive.get(k);
  if (!s) seenLive.set(k, (s = new Set()));
  s.add(sid);
}

/** 主窗口：接上发起方交过来的预期（`main.ts` 装一次）。 */
export function bindLaunchArrivals(): void {
  void listen<ArrivalSpec>(LAUNCH_EXPECT_EVENT, (e) => watchArrival(e.payload));
}

/** 主窗口之外的窗口（设置窗 · 会话窗）：主窗口交回来的那一句，是给这扇窗的就在这里说。 */
export function bindLaunchEcho(): void {
  const me = thisWindow();
  if (me === undefined || me === MAIN_WINDOW) return;
  void listen<Said>(LAUNCH_SAID_EVENT, (e) => {
    if (e.payload.to === me) {
      showActionFailureToast(e.payload.title, e.payload.body, { level: e.payload.level, durationMs: e.payload.durationMs });
    }
  });
}

/** 只给判据用。 */
export function __resetArrivalsForTests(): void {
  for (const p of pending) clearTimeout(p.timer);
  pending.clear();
  seenLive.clear();
}
