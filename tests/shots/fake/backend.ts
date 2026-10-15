/**
 * 页里的假适配层：站在壳的位置答产品代码发出的 Tauri 命令（`load_config` …）、帧命令（`chan_call` 里的 op）与订阅（`chan_subscribe`）。
 * `REAL_OPS` 那几条与 `SHELL_KINDS` 那几种订阅原样交给壳的真代码（`../real/pool.mjs` 起的无头壳，后面每台机器一个真后端，读 `world.disk` 那几份原始文件）；
 * 其余的还是场景合成的。
 * 答不上的一律记进 `unhandled`（工具最后列出来），并按真后端「不认」那一形拒绝 —— 不悄悄给空。
 */
import { Refuse, type World } from "./types";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { diskOf } from "./world";
import type { Convo } from "./records";
import { WALL_CEILING_MS } from "../../test-support/wall-ceiling";

/**
 * 交给真后端答的帧命令（`real/pool.mjs` 每台机器起的那一个，读 `world.disk` 那几份原始文件）。场景不许再替它们写答（`world.ops` 里有就报）。
 * 其余的帧命令还是场景合成的；`tests/shots/fake/` 里不许写核心成品的字与判定（判据 `tests/shots/fake-no-core.vitest.ts`）。
 */
export const REAL_OPS = new Set([
  "accounts-list",
  "profiles-read",
  "profiles-resolve",
  "profiles-impact",
  "profiles-bases",
  "profiles-write",
  "quota-read",
  "rotation-view",
  "rotation-strategies",
  // 轮换那几条写（会话的草稿 · 存 · 换策略 · 拖出来的顺序 · 24 小时条算分段）：真后端写台架盘上那几份文件，场景照它回的重画。
  "rotation-settings",
  "rotation-save",
  "rotation-revert",
  "rotation-use",
  "rotation-for-new",
  "rotation-temp",
  "rotation-day-paint",
  "history-page",
  "history-lines",
  "history-index",
  "history-user-inputs",
  "history-turns",
  "history-facts",
  "history-branch",
  "history-run",
  "history-list",
  "history-search",
  "history-search-merge",
  "history-find",
  "history-last-accounts",
  "exit-policy-read",
  "needs-answer",
  // 那台的偏好（`prefs.json`：启动命令 · 子会话 / 分身会话进不进标签栏 …）：真后端读写台架盘上那一份，缺省只在后端。
  "prefs-read",
  "prefs-set",
  // 改注解（星标 · 改名 · 隐藏）：真后端写台架盘上那一份，它的观测那一侧照新注解重判隐藏、推 `changed/session_hidden`。
  "history-annotate",
]);

/**
 * 交给无头壳的订阅（壳的 `event_replay` 交格，那几台后端推）：会话流（`session-lines` 与独立窗口的 `session-lines/<sid>`：
 * 后端读盘上的原始记录 ＋ pidfile ＋ 私有 tmux 出宣告 · 行 · 红绿灯 · 容器 · 子运行 · 已结束 / 可重连，壳的会话账攒批、重放）· 那四种 `changed/<topic>`（含待答请求 `needs`）。
 * 其余几种还是这里合成：`session-live-card`（活卡帧，台架没有真流量，场景自己推）· 其余几种 `changed/<topic>`（场景演换号那一下自己推，`pushChanged`；
 * 换号那几条帧命令还是合成的）· 终端画面。看不见 / 被关掉的那几台（`unseenMachines` · `closedMachines`）是连接那一跳的事，照旧这里演。
 */
export const SHELL_KINDS = new Set(["session-lines", "changed/accounts", "changed/profiles", "changed/tasks", "changed/needs", "changed/session_hidden"]);
const shellKind = (kind: string): boolean => SHELL_KINDS.has(kind) || kind.startsWith("session-lines/");

/**
 * 真后端够不着的那一支：只有 Linux 编出来的后端，Windows 那台的 `cfg(windows)` 分支演不出来。那几问照样交真后端答，
 * 只把 Windows 才有的那一格改过来（账号清单的 `meta.unsupported`：`platform::acct_view::unsupported_said`）。
 */
const WINDOWS_FACE: Record<string, Record<string, { meta: Record<string, unknown> }>> = {
  "win-laptop": { "accounts-list": { meta: { unsupported: copyText("beAcctWire.platform.unsupported", { os: "windows" }) } } },
};

type Emit = (event: string, payload: unknown) => unknown;

const enc = new TextEncoder();
const dec = new TextDecoder();

interface Sub {
  id: number;
  origin: string;
  kind: string;
}

export class FakeBackend {
  readonly unhandled: string[] = [];
  /** 每条帧命令问过几次（性能台架数取回的问数用；真后端答的那几条也算）。 */
  readonly asked: Record<string, number> = {};
  private emit: Emit = () => undefined;
  private readonly subs = new Map<number, Sub>();
  /** 交给无头壳的那几条订阅：编号 → 哪台、哪一种。 */
  private readonly shellSubs = new Map<number, Sub>();
  /** 开页之后台架还在那几台上演的事（已结束的收场 · 断了）：截图等它们都落定（[`settled`]）。 */
  private readonly pending: Promise<unknown>[] = [];
  /** 收到过清单的那几台（每台只演一次）。 */
  private readonly listedOrigins = new Set<string>();
  /** 有一台报完清单了（[`settled`] 按它醒，不按钟点问）。 */
  private onListed: () => void = () => undefined;
  private screenSeq = 0;
  /** 无头壳已经交出 `live` 那一格的会话（按 sid 去重）：等 tab 等不到时的读数（`window.__shots.sessions`）。 */
  readonly liveSent = new Set<string>();

  /** 真后端那一屋子起好了（`/__ccm/world` 答了）。 */
  private readonly ready: Promise<void>;
  private readonly key = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;

  /**
   * `mainWindow`：这一页是不是主窗口。主窗口等那台的会话流报完清单再演「收场」（开页时那几个还活着，看得见它们变成已结束）；
   * 别的页（独立窗口 · 设置）不订那一条，开页就让那几个收场 —— 不然它们在那几页里一直活着（已结束的画成「在跑」、规则页把它算进在用的会话）。
   */
  constructor(
    readonly world: World,
    opts: { mainWindow: boolean } = { mainWindow: true },
  ) {
    for (const op of REAL_OPS) {
      if (op in world.ops) this.miss(`场景替真后端答了 ${op}（改写 world.disk）`);
    }
    const machines = diskOf(world);
    this.ready = fetch("/__ccm/world", { method: "POST", body: JSON.stringify({ key: this.key, machines, now: Date.now(), scene: new URLSearchParams(location.search).get("scene") ?? "" }) }).then(async (r) => {
      if (!r.ok) throw new Error(`真后端起不来：${await r.text()}`);
      // 无头壳交的格（同壳交给页的 `chan-items` 事件体），原样当事件发。
      // 事件流**连上了才算起好**：别的问（帧命令 · 订阅）都排在它后面。浏览器对一个源同时只开 6 条连接，
      // 机器忙时那几问答得慢，后开的事件流排不上队 ⇒ 一格都收不到、一个 tab 都起不来（负载高时偶发的「现有 0 个 tab」就是它）。
      const es = new EventSource(`/__ccm/items?key=${encodeURIComponent(this.key)}`);
      es.onmessage = (m) => {
        const d = JSON.parse(m.data as string) as { sub: number; items: { t: string; body?: string }[] };
        this.watch(d.sub, d.items);
        for (const it of d.items) {
          if (it.t !== "frame" || !it.body?.startsWith('{"live"')) continue;
          const sid = (JSON.parse(it.body) as { live?: { session_id?: string } }).live?.session_id;
          if (sid) this.liveSent.add(sid);
        }
        void this.emit("chan-items", d);
      };
      await new Promise<void>((resolve) => (es.onopen = () => resolve()));
    });
    if (!opts.mainWindow) {
      for (const origin of new Set(world.sessions.map((x) => x.origin))) {
        this.listedOrigins.add(origin);
        this.pending.push(this.tell("end", { origin }));
      }
    }
  }

  /** 交给壳 `chan_call` 那一跳：成 ⇒ 原样字节；不成 ⇒ 壳交给页的那一形（`{err, body, detail}`，原样抛）。 */
  private async real(origin: string, op: string, req: Record<string, unknown>, view: unknown): Promise<ArrayBuffer> {
    await this.ready;
    const r = await fetch("/__ccm/call", { method: "POST", body: JSON.stringify({ key: this.key, origin, op, args: req, view: view ?? null }) });
    const f = (await r.json()) as { ok: boolean; body?: string; fail?: unknown };
    if (f.ok) return enc.encode(f.body ?? "null").buffer;
    throw f.fail;
  }

  /** 那台真后端此刻在等人的会话（`sessions-needs`）；那台看不见 ⇒ 空。 */
  private async realNeeds(origin: string): Promise<{ sid: string; needs: { ask: unknown; manualText: unknown } }[]> {
    if (this.world.unseenMachines.includes(origin)) return [];
    try {
      const body = await this.real(origin, "sessions-needs", {}, null);
      return ((JSON.parse(dec.decode(body)) as { waiting?: unknown[] }).waiting ?? []) as { sid: string; needs: { ask: unknown; manualText: unknown } }[];
    } catch {
      return [];
    }
  }

  /**
   * 场景演「那一家的答题 hook 挂上一条请求」：经真后端 `needs-ask`（与 hook 客户端 `--answer-hook` 同一条命令）挂上、不等它答完。
   * `hook` 是那一家 hook 交来的那一份（PermissionRequest 的形）。之后真后端推 `changed {needs}`，页里照常重读。
   */
  hookAsk(origin: string, hook: Record<string, unknown>): void {
    // 答完 / 撤了才回；台架只记一笔（没挂上就看得见为什么）。
    void this.real(origin, "needs-ask", { agent: "claude", hook }, null).then(
      (b) => console.info("[shots] needs-ask 收场：", dec.decode(b)),
      (e: unknown) => console.warn("[shots] needs-ask 没成：", JSON.stringify(e)),
    );
  }

  /** 那台真后端此刻给这个会话挂着的请求 id（没有 ⇒ `null`）。 */
  async askIdOf(origin: string, sid: string): Promise<string | null> {
    const ask = (await this.realNeeds(origin)).find((r) => r.sid === sid)?.needs.ask as { id?: string } | null | undefined;
    return ask?.id ?? null;
  }

  /** 场景演「别处（终端那边）先答了」：经真后端答掉那一条。 */
  async answerElsewhere(origin: string, sid: string, choice: string): Promise<void> {
    const w = await this.realNeeds(origin);
    const ask = (w.find((r) => r.sid === sid)?.needs.ask as { id?: string } | null)?.id;
    if (!ask) throw new Error(`${sid} 没挂着请求`);
    await this.real(origin, "needs-answer", { sid, ask, choice }, null);
  }

  /** 场景演那一家改了自己 pidfile 里的状态（原词：busy · waiting · idle · shell）。 */
  async setStatus(origin: string, sid: string, status: string, waitingFor: string | null = null): Promise<void> {
    await this.ready;
    const r = await fetch("/__ccm/status", { method: "POST", body: JSON.stringify({ key: this.key, origin, sid, status, waitingFor }) });
    if (!r.ok) throw new Error(`改不了 ${origin} 上 ${sid} 的状态：${await r.text()}`);
  }

  /** 场景在开页之后改那台盘上的一份文件（相对家目录；例：存之前配置文件被别处改过）。 */
  async writeFile(origin: string, rel: string, text: string, append = false): Promise<void> {
    await this.ready;
    const r = await fetch("/__ccm/write", { method: "POST", body: JSON.stringify({ key: this.key, origin, rel, text, append }) });
    if (!r.ok) throw new Error(`改不了 ${origin} 上的 ${rel}：${await r.text()}`);
  }

  /** 场景在开页之后往一个会话的记录里再写几行（`from` 起的那几条；真后端的 watcher 读到、照真的那一套交格）。 */
  appendLines(origin: string, c: Convo, from: number): Promise<void> {
    return this.writeFile(origin, c.file(), c.textFrom(from), true);
  }

  /** 订阅那几样交给无头壳。 */
  private tell(what: "sub" | "want" | "stop" | "ready" | "kill" | "end" | "watch", body: Record<string, unknown>): Promise<void> {
    return this.ready.then(async () => {
      const r = await fetch(`/__ccm/${what}`, { method: "POST", body: JSON.stringify({ key: this.key, ...body }) });
      if (!r.ok) throw new Error(`/__ccm/${what}：${await r.text()}`);
    });
  }

  /** 主界面发 `frontend-ready`（就绪点：壳照它重放会话流，带上次所在的 tab）。 */
  frontendReady(payload: { prioritySid?: string | null } | null): void {
    void this.tell("ready", { priority_sid: payload?.prioritySid ?? null });
  }

  /**
   * 那台的会话流报完清单了（主界面那一条）：开页之后才演的那两样这时演 —— 已结束的那几个收场（[`SessionSpec.ended`]）·
   * 断了的那几台过一会儿后端被杀（[`World.droppedMachines`]：壳那一侧照真的那一套作废那台的成品、通道说断了）。
   */
  private watch(id: number, items: { t: string; body?: string }[]): void {
    const sub = this.shellSubs.get(id);
    if (!sub || sub.kind !== "session-lines" || this.listedOrigins.has(sub.origin)) return;
    if (!items.some((it) => it.t === "frame" && it.body?.startsWith('{"listed"'))) return;
    this.listedOrigins.add(sub.origin);
    this.onListed();
    this.pending.push(this.tell("end", { origin: sub.origin }));
    if (this.world.droppedMachines?.includes(sub.origin)) this.pending.push(new Promise((r) => setTimeout(r, 1500)).then(() => this.tell("kill", { origin: sub.origin })));
  }

  /** 开页之后台架在演的那几样都落定了（截图在这之后量、截）。 */
  async settled(): Promise<void> {
    await this.ready;
    // 主界面订了会话流的那几台都报完清单（收场 · 断了那几样是收到清单才排上的）：按「又一台报完了」醒，不按钟点问；
    // 到兜底上限还有没报完的 ⇒ 抛、说是哪几台（从前至多等 15 秒就往下走：机器忙时清单晚到，截到的是收场之前那一刻）。
    const unlisted = (): string[] => [...this.shellSubs.values()].filter((s) => s.kind === "session-lines" && !this.listedOrigins.has(s.origin)).map((s) => s.origin);
    const t0 = Date.now();
    while (unlisted().length > 0) {
      const left = WALL_CEILING_MS - (Date.now() - t0);
      if (left <= 0) throw new Error(`等到上限，这几台的会话流还没报完清单：${unlisted().join("、")}`);
      await new Promise<void>((r) => {
        const t = setTimeout(r, left);
        this.onListed = () => {
          clearTimeout(t);
          r();
        };
      });
    }
    for (let n = 0; n < this.pending.length; n = this.pending.length) await Promise.all(this.pending.slice(n));
  }

  attachEmitter(emit: Emit): void {
    this.emit = emit;
  }

  private miss(what: string): void {
    if (!this.unhandled.includes(what)) this.unhandled.push(what);
    console.warn(`[shots] 假后端答不上：${what}`);
  }

  invoke(cmd: string, args: Record<string, unknown>): unknown {
    switch (cmd) {
      case "chan_call":
        return this.chanCall(args);
      case "chan_offer":
        return { ops: Object.keys(this.world.ops), unavailable: [], stoppable: [] };
      case "watch_sessions":
        // 主窗口报在看哪几个（整份换）：转给无头壳，它照产品那一条合每台的名单、经 `stream-watch` 报给那台后端（没在看的会话，行不上流）。
        // 交给无头壳的这几条一律等它收下了才回（同产品：Tauri 命令登记完才返回）—— 主界面 `await` 完订阅才发 `frontend-ready`，
        // 当场回的话就绪点可能抢在订阅前面到无头壳，那条订阅就永远过不了就绪点（判据 `tests/shots/fake-order.vitest.ts`）。
        return this.tell("watch", { sessions: args.sessions ?? [] }).then(() => null);
      case "chan_subscribe":
        return this.subscribe(args);
      case "chan_want":
        if (this.shellSubs.has(Number(args.id))) return this.tell("want", { id: Number(args.id), more: Number(args.more) }).then(() => null);
        return null;
      case "chan_stop":
        if (this.shellSubs.delete(Number(args.id))) return this.tell("stop", { id: Number(args.id) }).then(() => null);
        return null;
      case "chan_cancel":
        return null;
    }
    const own = this.world.commands[cmd];
    if (own) return own(args, this.world);
    if (cmd.startsWith("plugin:")) return pluginDefault(cmd, args);
    this.miss(`命令 ${cmd}`);
    return Promise.reject(`shots: 没有这条命令 ${cmd}`);
  }

  private chanCall(args: Record<string, unknown>): unknown {
    const origin = String(args.origin);
    const op = String(args.op);
    this.asked[op] = (this.asked[op] ?? 0) + 1;
    const raw = args.payload as number[];
    const req = raw.length > 0 ? (JSON.parse(dec.decode(Uint8Array.from(raw))) as Record<string, unknown>) : {};
    // 不带声明的那一问（展开那一下按记录 id 取回全文）另有一档延时，让「读取中」那一形截得到。声明本身原样交给真后端，这里不读它。
    const delay = (args.view == null ? this.world.fullFetchDelayMs?.[op] : undefined) ?? this.world.opDelayMs?.[op] ?? 0;
    // 那台后端比界面老、不认这一条（照真壳交的「不认」那一形）· 到那台那一跳断开（照壳的跳失败那一形）。
    if (this.world.older?.[origin]?.includes(op)) return Promise.reject({ err: "Unsupported", body: [] });
    if (this.world.hopDown?.[origin]?.includes(op)) return Promise.reject({ err: { Hop: { idx: 0, tag: "open", reach: "NotSent", why: "Unreachable" } }, body: [] });
    if (REAL_OPS.has(op)) {
      const face = WINDOWS_FACE[origin]?.[op];
      return new Promise((r) => setTimeout(r, delay))
        .then(() => this.real(origin, op, req, args.view))
        .then((b) => {
          if (!face) return b;
          const v = JSON.parse(dec.decode(b)) as { meta?: object };
          return enc.encode(JSON.stringify({ ...v, meta: { ...v.meta, ...face.meta } })).buffer;
        });
    }
    const handler = this.world.ops[op];
    if (!handler) {
      if (!this.world.quiet?.includes(op)) this.miss(`帧命令 ${op}`);
      return Promise.reject({ err: "Unsupported", body: [] });
    }
    try {
      const v = handler(origin, req, this.world);
      const later = delay > 0 ? new Promise((r) => setTimeout(() => r(v), delay)) : Promise.resolve(v);
      return later.then(
        // 照真壳交原始字节（`tauri::ipc::Response` ⇒ 页里拿到 ArrayBuffer），不交数字数组：
        // 长会话整份读那一问有几 MB，数字数组那一形光假后端自己造就占掉页里几百 ms（性能台架量的是产品）
        (value) => enc.encode(JSON.stringify(value)).buffer,
        (e: unknown) => Promise.reject(refusal(e, op)),
      );
    } catch (e) {
      return Promise.reject(refusal(e, op));
    }
  }

  private subscribe(args: Record<string, unknown>): Promise<null> | null {
    const sub: Sub = { id: Number(args.id), origin: String(args.origin), kind: String(args.kind) };
    if (shellKind(sub.kind) && !this.world.unseenMachines.includes(sub.origin) && !this.world.closedMachines.includes(sub.origin)) {
      this.shellSubs.set(sub.id, sub);
      return this.tell("sub", { id: sub.id, origin: sub.origin, kind: sub.kind, want: Number(args.want ?? 0) }).then(() => null);
    }
    this.subs.set(sub.id, sub);
    // 订阅登记好之后才交格（与真句柄一样：返回之后格才可能到）。
    setTimeout(() => this.deliver(sub), 0);
    return null;
  }

  private deliver(sub: Sub): void {
    if (this.world.unseenMachines.includes(sub.origin)) {
      this.send(sub, [{ t: "unseen", idx: 1, tag: "open", why: "Unreachable" }]);
      return;
    }
    if (this.world.closedMachines.includes(sub.origin) && sub.kind.startsWith("session-lines")) {
      this.send(sub, [{ t: "closed_by_peer", body: JSON.stringify({ code: "busy", message: "这台机器上的会话太多，先关掉几个再连" }) }]);
      return;
    }
    if (sub.kind.startsWith("terminal-screen/")) {
      // 终端实时画面：订上之后推一屏（就是那一刻的 `terminal-preview` 成品）；按场景随后停（画面中断 · 那台断开）。
      // 那一屏抓不到（场景让 `terminal-preview` 拒）⇒ 不推：真后端那边是 `terminal-follow` 那一问被拒，流上什么都不来。
      const mode = this.world.terminalLive ?? "live";
      let view: unknown;
      try {
        view = this.world.ops["terminal-preview"]?.(sub.origin, {}, this.world);
      } catch (e) {
        if (e instanceof Refuse) return;
        throw e;
      }
      setTimeout(() => this.send(sub, [{ t: "frame", seq: 0, body: JSON.stringify({ seq: 1, view }) }]), 200);
      if (mode === "lost") setTimeout(() => this.send(sub, [{ t: "frame", seq: 1, body: JSON.stringify({ end: "lost" }) }]), 500);
      if (mode === "offline") setTimeout(() => this.send(sub, [{ t: "unseen", idx: 1, tag: "read", why: "Dropped" }]), 500);
    } else if (sub.kind === "session-live-card" || sub.kind.startsWith("changed/")) {
      this.send(sub, [{ t: "seen", from: null }]);
    } else {
      this.miss(`订阅 ${sub.kind}`);
    }
  }

  private send(sub: Sub, items: unknown[]): void {
    void this.emit("chan-items", { sub: sub.id, items });
  }

  /** 场景在开页之后推一格 `changed/<topic>`（格体 `{key?, rev?, body?}`，同壳交的那一份）。 */
  pushChanged(origin: string, topic: string, cell: unknown): void {
    for (const sub of this.subs.values()) {
      if (sub.origin === origin && sub.kind === `changed/${topic}`) this.send(sub, [{ t: "frame", seq: 0, body: JSON.stringify(cell) }]);
    }
  }

  /** 性能台架：往那台所有终端实时画面的订阅上推一屏（`view` 同 `terminal-preview` 成品）。 */
  pushScreen(origin: string, view: unknown): void {
    this.screenSeq += 1;
    for (const sub of this.subs.values()) {
      if (sub.origin === origin && sub.kind.startsWith("terminal-screen/")) this.send(sub, [{ t: "frame", seq: this.screenSeq, body: JSON.stringify({ seq: this.screenSeq + 1, view }) }]);
    }
  }

  pushLiveCard(origin: string, payloads: unknown[]): void {
    for (const sub of this.subs.values()) {
      if (sub.origin === origin && sub.kind === "session-live-card") {
        this.send(
          sub,
          payloads.map((p, seq) => ({ t: "frame", seq, body: JSON.stringify(p) })),
        );
      }
    }
  }
}

/** 替身那一份复制详情（那台后端写、monitor 转交的那几行）：钟停在夹具那一刻，机器与版本是合成的。 */
function detailOf(op: string, e: Refuse): string {
  const l = (k: Parameters<typeof copyText>[0]): string => copyText(k);
  return [
    `${l("detail.label.at")}：2026-10-08 14:35:50 +08:00`,
    `${l("detail.label.machine")}：Linux x86_64 · ${copyText("detail.value.backend", { build: "p9k-shots" })}`,
    `${l("detail.label.command")}：${op}`,
    `${l("detail.label.code")}：${e.code}`,
    ...(e.raw !== undefined ? [`${l("detail.label.raw")}：${e.raw}`] : []),
  ].join("\n");
}

function refusal(e: unknown, op = ""): { err: unknown; body: number[]; detail?: string } {
  // 场景要演通道那一跳的失败（够不着 · 期限到）：直接抛线上形状 `{err, body}`，原样交回。
  if (e !== null && typeof e === "object" && "err" in e && "body" in e) return e as { err: unknown; body: number[] };
  if (e instanceof Refuse) {
    const body = e.data === undefined ? { code: e.code, message: e.message } : { code: e.code, message: e.message, data: e.data };
    return { err: "Refused", body: Array.from(enc.encode(JSON.stringify(body))), detail: detailOf(op, e) };
  }
  return { err: "Refused", body: Array.from(enc.encode(JSON.stringify({ code: "shots", message: String(e) }))) };
}

/** 窗口 / 路径 / 对话框这几个插件：截图里不真开窗、不真选文件，给一个不出错的答。 */
function pluginDefault(cmd: string, args: Record<string, unknown>): unknown {
  if (cmd === "plugin:path|resolve_directory") return "/home/user";
  if (cmd === "plugin:path|join") return (args.paths as string[]).join("/");
  if (cmd === "plugin:window|scale_factor") return 1;
  if (cmd === "plugin:window|inner_size" || cmd === "plugin:window|outer_size") {
    return { width: window.innerWidth, height: window.innerHeight };
  }
  if (cmd.startsWith("plugin:window|is_")) return false;
  if (cmd === "plugin:dialog|open" || cmd === "plugin:dialog|save") return null;
  return null;
}
