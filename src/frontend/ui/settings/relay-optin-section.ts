// 机器页「终端」栏：让直接敲的 claude 也走中转（可选、生成让你贴）。
//
// 那台后端读它自己那份用户级设置文件，判装没装、对不对，并给要合并进去的那一段（`relay-optin-reads.ts`）。
// 界面只照态画：一行状态 · 中转此刻没在听那一句 · 待贴块（贴到哪 · 合并别覆盖 · 新开会话才生效 · 复制）· 四条代价。
// **不写那份文件**：贴不贴、什么时候贴由用户自己定。
//
// 构造零 I/O：整块是个 `<details>`，第一次展开才问（要贴的那一段带着中转钥匙，不展开就不让它进界面）；
// 展开着切机器 ⇒ 问新的那一台，晚到的旧答复不许盖掉当前这台的。
import { getCurrentMachine, subscribeMachine } from "./machine-context";
import { buildPasteBlock } from "../paste-block";
import { copyText } from "../copy-table";
import { fetchRelayOptin, type RelayOptinReport, type RelayOptinState } from "../relay-optin-reads";

/** 一态 → 状态行那句话（读不了那一态用后端说的原因）＋ 语气。 */
export function describeOptin(r: RelayOptinReport): { text: string; tone: "ok" | "bad" | "unknown" } {
  const by: Record<RelayOptinState, () => { text: string; tone: "ok" | "bad" | "unknown" }> = {
    installed: () => ({ text: copyText("relayOptin.state.installed"), tone: "ok" }),
    stale: () => ({ text: copyText("relayOptin.state.stale"), tone: "bad" }),
    absent: () => ({ text: copyText("relayOptin.state.absent"), tone: "unknown" }),
    other: () => ({ text: copyText("relayOptin.state.other"), tone: "unknown" }),
    unreadable: () => ({ text: r.note, tone: "unknown" }),
  };
  return by[r.state]();
}

const TONE_CLASS = { ok: "relay-optin-state is-ok", bad: "relay-optin-state is-bad", unknown: "relay-optin-state is-unknown" } as const;

/** 一格：类名只用样式表里有的那几个；哪一格是什么由 `data-part` 说（判据按它找，不按类名）。 */
function div(cls: string, part: string, text = ""): HTMLDivElement {
  const d = document.createElement("div");
  d.className = cls;
  d.dataset.part = part;
  d.textContent = text;
  return d;
}

/** 四条代价（逐条写清；文案键逐字列出，不拼）。 */
const COSTS: readonly [string, () => string][] = [
  ["backend", () => copyText("relayOptin.cost.backend")],
  ["key", () => copyText("relayOptin.cost.key")],
  ["allAccounts", () => copyText("relayOptin.cost.allAccounts")],
  ["apiKey", () => copyText("relayOptin.cost.apiKey")],
];

export class RelayOptinSection {
  readonly element: HTMLDetailsElement;
  private body: HTMLElement;
  /** 第一次展开过没有（之后切机器才由订阅重问）。 */
  private opened = false;
  /** 切机器快过答复时，晚到的那一份不许盖掉当前这台的。 */
  private seq = 0;

  constructor() {
    this.element = document.createElement("details");
    this.element.className = "relay-optin";
    const summary = document.createElement("summary");
    summary.textContent = copyText("relayOptin.section.title");
    this.element.appendChild(summary);
    this.element.appendChild(div("settings-hint", "intro", copyText("relayOptin.section.intro")));
    this.body = div("", "body");
    this.element.appendChild(this.body);
    this.element.appendChild(this.buildCosts());
    this.element.addEventListener("toggle", () => {
      if (!this.element.open || this.opened) return;
      this.opened = true;
      void this.refresh();
    });
    subscribeMachine(() => {
      if (this.opened) void this.refresh();
    });
  }

  private buildCosts(): HTMLElement {
    const box = div("", "costs");
    box.appendChild(div("settings-label", "costs-title", copyText("relayOptin.costs.title")));
    const list = document.createElement("ol");
    list.className = "settings-hint";
    for (const [k, text] of COSTS) {
      const li = document.createElement("li");
      li.dataset.cost = k;
      li.textContent = text();
      list.appendChild(li);
    }
    box.appendChild(list);
    return box;
  }

  private async refresh(): Promise<void> {
    const origin = getCurrentMachine();
    const my = ++this.seq;
    this.body.replaceChildren(div("settings-hint", "loading", copyText("relayOptin.read.loading")));
    try {
      const r = await fetchRelayOptin(origin);
      if (my !== this.seq) return;
      this.render(r);
    } catch (e) {
      if (my !== this.seq) return;
      this.body.replaceChildren(div("relay-optin-state is-bad", "failed", copyText("relayOptin.read.failed", { said: e instanceof Error ? e.message : String(e) })));
    }
  }

  private render(r: RelayOptinReport): void {
    const d = describeOptin(r);
    const state = div(TONE_CLASS[d.tone], "state", d.text);
    state.dataset.state = r.state;
    const parts: HTMLElement[] = [state];
    if (!r.listening) parts.push(div("relay-optin-state is-bad", "not-listening", copyText("relayOptin.relay.notListening")));
    const snippet = r.snippet;
    if (snippet !== null) {
      const paste = buildPasteBlock({
        text: () => snippet,
        target: copyText("relayOptin.snippet.target", { source: r.source }),
        mergeNote: copyText("relayOptin.snippet.merge"),
        activation: copyText("relayOptin.snippet.activation"),
        multiline: true,
        rows: 5,
      });
      parts.push(paste.element);
    } else if (r.missing) {
      parts.push(div("relay-optin-state is-unknown", "missing", r.missing));
    }
    this.body.replaceChildren(...parts);
  }
}
