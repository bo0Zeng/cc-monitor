/**
 * 机器页「别名与配置文件」那一栏：一张清单，每项一行，点开就地做（`设计稿/机器配置-v2.md` §5 方案 A）。
 *
 * - 页首一句：这一页每一项都是你点了才改；要你自己动手的在「文件与数据 → 要你动手」。
 * - 「终端」组：别名（含接上 / 卸载 ccm；默认展开）· Windows 终端（只本机 Windows）—— 都由 `machine-aliases.ts` 建。
 * - 「账号与扩展」组：共用 MCP（含「停止同步」）· 装在这台的扩展（→ 扩展页）。
 *
 * 本机远端同一个组件，`origin` 是入参。界面只画：每行的现状、名字与数都是那台后端答的。
 *
 * **构造零 I/O**：这一栏第一次露出来（机器页切到这一栏 ⇒ [`CONFIG_SHOWN_EVENT`]）才问；再露出来重读一遍。
 */
import { copyText } from "../copy-table";
import { isLocalOrigin, type Origin } from "../ipc/origin";
import { button } from "../kit/button";
import { confirmDialog } from "../kit/dialog";
import { toast } from "../kit/toast";
import { detailOf } from "../kit/detail";
import { saidOfControl } from "../control-said";
import { accountsMcpPick, accountsMcpRead, accountsMcpRemove, accountsMcpSync, type AccountMcpView } from "../account-ops";
import { extList, type ExtList } from "../ext-reads";
import { CONFIG_SHOWN_EVENT, SETTINGS_GO_EVENT } from "./events";
import { buildAliasManager, type AliasManagerSpec } from "./machine-aliases";
import { cfgRow, type CfgRow } from "./cfg-row";
import { countTags } from "./chore-tags";
import { readDataReport } from "./data-reads";


function cap(text: string): HTMLElement {
  const c = document.createElement("div");
  c.className = "cfg-cap";
  c.textContent = text;
  return c;
}

function line(cls: string, text: string): HTMLElement {
  const e = document.createElement("div");
  e.className = cls;
  e.textContent = text;
  return e;
}

/** 行里的小链接（删 · 停止同步）。 */
function link(label: string, onClick: () => void, danger = false): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.className = danger ? "cfg-link cfg-link-danger" : "cfg-link";
  b.textContent = label;
  b.addEventListener("click", onClick);
  return b;
}

/** 「共用 MCP」那一行：各号共用的那几条 · 两边都改了的选一版 · 删一条（问一句）· 停止 / 开回同步。 */
function buildMcpRow(origin: () => Origin, machine: () => string): { row: CfgRow; load(): void } {
  const row = cfgRow(copyText("cfgPage.mcp.title"), false);
  row.element.dataset.anchor = "shared-mcp";
  row.element.dataset.role = "mcp-row";
  const body = row.body;

  const paint = (v: AccountMcpView): void => {
    body.replaceChildren();
    row.setAction(null);
    if (!v.enabled) {
      row.setStatus("off", copyText("cfgPage.mcp.noLibrary"));
      body.appendChild(line("cfg-hint", copyText("cfgPage.mcp.noLibraryBody")));
      return;
    }
    if (!v.sync) row.setStatus("off", copyText("cfgPage.mcp.paused", { n: String(v.servers.length) }));
    else if (v.conflicts.length > 0) {
      row.setStatus("warn", copyText("cfgPage.mcp.conflict", { name: v.conflicts[0].name }));
      row.setAction(button({ label: copyText("cfgPage.mcp.pickOpen"), size: "compact", onClick: () => row.setOpen(true) }));
    } else row.setStatus("ok", copyText("cfgPage.mcp.same", { n: String(v.servers.length), machine: machine() }));
    body.appendChild(line("cfg-sub", v.sync ? copyText("cfgPage.mcp.shared", { machine: machine() }) : copyText("cfgPage.mcp.pausedBody")));
    for (const c of v.conflicts) {
      const box = document.createElement("div");
      box.className = "cfg-conflict";
      box.dataset.role = "mcp-conflict";
      box.appendChild(line("cfg-conflict-title", copyText("cfgPage.mcp.conflictHead", { name: c.name })));
      const acts = document.createElement("div");
      acts.className = "cfg-acts";
      for (const ch of c.choices) {
        const label =
          ch.from !== null
            ? copyText("cfgPage.mcp.useFrom", { holders: ch.holders.join(copyText("accountsMcp.list.sep")) })
            : ch.gone
              ? copyText("cfgPage.mcp.useGone")
              : copyText("cfgPage.mcp.useShared");
        acts.appendChild(button({ label, size: "compact", onClick: () => void act(() => accountsMcpPick(origin(), c.name, ch.from), () => copyText("cfgPage.mcp.pickFailed")) }));
      }
      box.appendChild(acts);
      body.appendChild(box);
    }
    const list = document.createElement("div");
    list.className = "cfg-list";
    for (const name of v.servers) {
      const r = document.createElement("div");
      r.className = "cfg-list-row";
      r.dataset.role = "mcp-server";
      const n = document.createElement("code");
      n.className = "cfg-list-name";
      n.textContent = name;
      r.appendChild(n);
      if (v.sync) {
        r.appendChild(link(copyText("cfgPage.mcp.remove"), () => void remove(name), true));
      }
      list.appendChild(r);
    }
    if (v.servers.length === 0) list.appendChild(line("cfg-hint", copyText("cfgPage.mcp.empty")));
    body.appendChild(list);
    for (const n of v.notes) body.appendChild(line("cfg-hint", n));
    body.appendChild(line("cfg-hint", copyText("cfgPage.mcp.newSessions")));
    const foot = document.createElement("div");
    foot.className = "cfg-foot-row";
    foot.appendChild(
      link(v.sync ? copyText("cfgPage.mcp.stop") : copyText("cfgPage.mcp.resume"), () => void act(() => accountsMcpSync(origin(), !v.sync), () => copyText("cfgPage.mcp.syncFailed"))),
    );
    foot.appendChild(line("cfg-hint", v.sync ? copyText("cfgPage.mcp.stopHint") : copyText("cfgPage.mcp.resumeHint")));
    foot.dataset.role = "mcp-sync";
    body.appendChild(foot);
  };

  const act = async (op: () => Promise<AccountMcpView>, failed: () => string): Promise<void> => {
    try {
      paint(await op());
    } catch (e) {
      toast(failed(), saidOfControl(e), { detail: detailOf(e), level: "error" });
    }
  };

  const remove = async (name: string): Promise<void> => {
    const ok = await confirmDialog({
      title: copyText("cfgPage.mcp.removeTitle", { name }),
      action: copyText("cfgPage.mcp.removeAction"),
      danger: true,
      body: copyText("cfgPage.mcp.removeBody", { name, machine: machine() }),
    });
    if (ok) await act(() => accountsMcpRemove(origin(), name), () => copyText("cfgPage.mcp.removeFailed"));
  };

  const load = (): void => {
    row.setStatus("off", copyText("cfgPage.row.reading"));
    accountsMcpRead(origin()).then(paint, (e: unknown) => {
      row.setStatus("warn", copyText("cfgPage.mcp.readFailed"));
      body.replaceChildren(line("cfg-hint", saidOfControl(e)));
    });
  };
  return { row, load };
}

/** 「装在这台的扩展」那一行：数那台那一列（只数、不判）· 点开列名字 ·［去扩展页］。 */
function buildExtRow(origin: () => Origin, machine: () => string, go: (el: HTMLElement) => void): { row: CfgRow; load(): void } {
  const row = cfgRow(copyText("cfgPage.ext.title", { machine: machine() }), false);
  row.element.dataset.role = "ext-row";
  const goBtn = (): HTMLElement => button({ label: copyText("cfgPage.ext.go"), size: "compact", onClick: () => go(row.element) });
  const paint = (l: ExtList): void => {
    const local = isLocalOrigin(origin());
    const col = l.machines.findIndex((m) => (local ? m.here : m.key === origin()));
    row.body.replaceChildren();
    row.setAction(goBtn());
    if (col < 0 || !l.machines[col].reachable) {
      row.setStatus("off", copyText("cfgPage.ext.unknown"));
      row.body.appendChild(goBtn());
      return;
    }
    const at = l.rows.map((r) => ({ r, c: r.cells[col] })).filter((x) => x.c && x.c.state !== "missing");
    const skills = at.filter((x) => x.r.kind === "skill");
    const projMcp = at.filter((x) => x.r.kind === "mcp" && x.c.state === "project");
    row.setStatus(
      "ok",
      projMcp.length > 0
        ? copyText("cfgPage.ext.countWithMcp", { skills: String(skills.length), mcp: String(projMcp.length) })
        : copyText("cfgPage.ext.count", { skills: String(skills.length) }),
    );
    const minis = (title: string, items: typeof at): void => {
      if (items.length === 0) return;
      row.body.appendChild(line("cfg-sub", title));
      const box = document.createElement("div");
      box.className = "cfg-minis";
      for (const { r, c } of items) {
        const m = document.createElement("span");
        m.className = "cfg-mini";
        const where = c.places.some((p) => p.at.level === "user" && p.state !== "missing") ? copyText("cfgPage.ext.global") : copyText("cfgPage.ext.project");
        m.textContent = copyText("cfgPage.ext.mini", { name: r.name, where });
        box.appendChild(m);
      }
      row.body.appendChild(box);
    };
    minis(copyText("cfgPage.ext.skills"), skills);
    minis(copyText("cfgPage.ext.projectMcp"), projMcp);
    const foot = document.createElement("div");
    foot.className = "cfg-foot-row";
    foot.append(goBtn(), line("cfg-hint", copyText("cfgPage.ext.goHint")));
    row.body.appendChild(foot);
  };
  const load = (): void => {
    row.setName(copyText("cfgPage.ext.title", { machine: machine() }));
    row.setStatus("off", copyText("cfgPage.row.reading"));
    extList(false).then(paint, (e: unknown) => {
      row.setStatus("warn", copyText("cfgPage.ext.readFailed"));
      row.body.replaceChildren(line("cfg-hint", saidOfControl(e)));
    });
  };
  return { row, load };
}

export interface ConfigPageSpec extends AliasManagerSpec {
  /** 给人看的这台叫什么（本机 ⇒ 「本机」）。 */
  machine: () => string;
}

/**
 * 页首指路条（「{machine} 上有 N 件要你动手 · 另 M 件可选」［去「文件与数据」］）与页尾一句（「cc-monitor 在 {machine} 上改过你的 N 个文件」［去看］）：
 * 都读那台的 `data-report`（同一份成品，件数不另数）；读不到 ⇒ 两处都不出。
 */
function buildDataPointers(spec: ConfigPageSpec, onSelfPaste: (c: { state: string } | null) => void): { head: HTMLElement; foot: HTMLElement; load(): void } {
  const head = document.createElement("div");
  head.className = "cfg-pointer";
  head.dataset.role = "chores-pointer";
  head.hidden = true;
  const foot = document.createElement("div");
  foot.className = "cfg-foot-pointer";
  foot.dataset.role = "changed-pointer";
  foot.hidden = true;
  const go = (el: HTMLElement, anchor: string): void => {
    el.dispatchEvent(new CustomEvent(SETTINGS_GO_EVENT, { bubbles: true, detail: { page: "data", anchor } }));
  };
  const load = (): void => {
    readDataReport(spec.origin()).then(
      (r) => {
        const open = r.todo.filter((c) => c.state !== "done" && c.state !== "declined");
        const optional = open.filter((c) => c.kind === "optional" || c.kind === "installOptional").length;
        const machine = spec.machine();
        head.hidden = r.chores === 0 && optional === 0;
        head.replaceChildren();
        if (!head.hidden) {
          const dot = document.createElement("span");
          dot.className = "data-dot";
          dot.dataset.tone = r.chores > 0 ? "bad" : "muted";
          const text = r.chores > 0 ? copyText("cfgPage.pointer.chores", { machine, n: r.chores, m: optional }) : copyText("cfgPage.pointer.optional", { machine, m: optional });
          // 稿 12：那几枚分类数小标签（要做 · 要你定 · 要装 · 可选），与「文件与数据」同一套。
          const tags = document.createElement("span");
          tags.className = "data-summary cfg-pointer-tags";
          tags.append(...countTags(r.todo, ["must", "decide", "install", "optional"]));
          head.append(dot, line("cfg-pointer-text", text), tags, button({ label: copyText("cfgPage.pointer.go"), size: "compact", onClick: () => go(head, `chores:${spec.origin()}`) }));
        }
        foot.hidden = r.changedFiles.length === 0;
        foot.replaceChildren();
        if (!foot.hidden) {
          foot.append(
            line("cfg-hint", copyText("cfgPage.pointer.changed", { machine, n: r.changedFiles.length })),
            link(copyText("cfgPage.pointer.changedGo"), () => go(foot, `placed:${spec.origin()}`)),
          );
        }
        onSelfPaste(r.todo.find((c) => c.id === "self-paste") ?? null);
      },
      () => {
        head.hidden = true;
        foot.hidden = true;
      },
    );
  };
  return { head, foot, load };
}

/** 整一栏。 */
export function buildConfigPage(spec: ConfigPageSpec): HTMLElement {
  const root = document.createElement("div");
  root.className = "cfg-page";
  root.dataset.configShown = "";
  root.appendChild(line("cfg-intro", copyText("cfgPage.head.intro")));
  const terminal = buildAliasManager(spec);
  const pointers = buildDataPointers(spec, (c) => terminal.setSelfPaste(c));
  root.appendChild(pointers.head);
  root.append(cap(copyText("cfgPage.cap.terminal")), terminal.element);
  const go = (el: HTMLElement): void => {
    el.dispatchEvent(new CustomEvent(SETTINGS_GO_EVENT, { bubbles: true, detail: { page: "ext" } }));
  };
  const mcp = buildMcpRow(spec.origin, spec.machine);
  const ext = buildExtRow(spec.origin, spec.machine, go);
  const acctBox = document.createElement("div");
  acctBox.className = "cfg";
  acctBox.append(mcp.row.element, ext.row.element);
  root.append(cap(copyText("cfgPage.cap.accounts")), acctBox, pointers.foot);
  let shown = false;
  root.addEventListener(CONFIG_SHOWN_EVENT, () => {
    if (!shown) {
      shown = true;
      terminal.load();
    } else terminal.reread();
    mcp.load();
    ext.load();
    pointers.load();
  });
  return root;
}
