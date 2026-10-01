/**
 * 账号页里「各账号共用的 MCP」那一块：这台账号库里各号共用一份用户级 MCP（同步由那台后端做），
 * 这里只列那台后端答的名字与冲突、交「删一条」「用哪一版」两个意图。界面不比较任何定义（成品里也没有定义）。
 */
import { copyText } from "../copy-table";
import type { Origin } from "../ipc/origin";
import { accountsMcpPick, accountsMcpRead, accountsMcpRemove, type AccountMcpView } from "../account-ops";
import { askConfirm } from "../ask-dialog";
import { showActionFailureToast } from "../error-toast";
import { saidOfControl } from "../control-said";

function line(parent: HTMLElement, cls: string, text: string): HTMLElement {
  const el = document.createElement("div");
  el.className = cls;
  el.textContent = text;
  parent.appendChild(el);
  return el;
}

function button(text: string): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.textContent = text;
  return b;
}

/** 一块：说明三句 ＋ 名字（各带「删除」）＋ 两边都改了的那几条（各版一个按钮）＋ 那台后端的提示。 */
export function renderSharedMcp(origin: Origin): HTMLElement {
  const box = document.createElement("div");
  box.className = "accounts-mcp";
  line(box, "accounts-meta accounts-mcp-title", copyText("accountsMcp.block.title"));
  line(box, "accounts-hint accounts-mcp-shared", copyText("accountsMcp.block.shared"));
  line(box, "accounts-hint accounts-mcp-delete-here", copyText("accountsMcp.block.deleteHere"));
  line(box, "accounts-hint accounts-mcp-new-sessions", copyText("accountsMcp.block.newSessions"));
  const body = document.createElement("div");
  body.className = "accounts-mcp-body";
  box.appendChild(body);

  const paint = (v: AccountMcpView): void => {
    body.innerHTML = "";
    if (v.servers.length === 0) line(body, "accounts-info accounts-mcp-empty", copyText("accountsMcp.list.empty"));
    for (const name of v.servers) {
      const row = document.createElement("div");
      row.className = "accounts-mcp-row";
      line(row, "accounts-mcp-name", name);
      const del = button(copyText("accountsMcp.list.remove"));
      del.addEventListener("click", () => void remove(name, del));
      row.appendChild(del);
      body.appendChild(row);
    }
    for (const c of v.conflicts) {
      const row = document.createElement("div");
      row.className = "accounts-mcp-conflict";
      line(row, "accounts-info warn", copyText("accountsMcp.conflict.head", { name: c.name }));
      for (const ch of c.choices) {
        const label =
          ch.from === null
            ? copyText(ch.gone ? "accountsMcp.conflict.gone" : "accountsMcp.conflict.shared")
            : copyText("accountsMcp.conflict.from", { holders: ch.holders.join("、") });
        const b = button(label);
        b.addEventListener("click", () => void pick(c.name, ch.from, b));
        row.appendChild(b);
      }
      body.appendChild(row);
    }
    for (const n of v.notes) line(body, "accounts-hint accounts-mcp-note", n);
  };

  const done = (title: string, v: AccountMcpView): void => {
    const lines = v.changed.length > 0 ? [copyText("accountsMcp.change.synced", { accounts: v.changed.join("、") })] : [];
    showActionFailureToast(title, [...lines, ...v.notes].join("\n") || copyText("accountsMcp.block.newSessions"), {
      level: "info",
      durationMs: 6000,
    });
    paint(v);
  };

  const remove = async (name: string, btn: HTMLButtonElement): Promise<void> => {
    if (!(await askConfirm(copyText("accountsMcp.remove.confirm", { name })))) return;
    btn.disabled = true;
    try {
      done(copyText("accountsMcp.remove.done", { name }), await accountsMcpRemove(origin, name));
    } catch (e) {
      showActionFailureToast(copyText("accountsMcp.remove.failed"), saidOfControl(e), { level: "error" });
      btn.disabled = false;
    }
  };

  const pick = async (name: string, from: string | null, btn: HTMLButtonElement): Promise<void> => {
    btn.disabled = true;
    try {
      done(copyText("accountsMcp.pick.done", { name }), await accountsMcpPick(origin, name, from));
    } catch (e) {
      showActionFailureToast(copyText("accountsMcp.pick.failed"), saidOfControl(e), { level: "error" });
      btn.disabled = false;
    }
  };

  line(body, "accounts-info accounts-mcp-reading", copyText("accountsMcp.read.reading"));
  accountsMcpRead(origin).then(paint, (e: unknown) => {
    body.innerHTML = "";
    line(body, "accounts-info accounts-mcp-fail", copyText("accountsMcp.read.failed", { e: saidOfControl(e) }));
  });
  return box;
}
