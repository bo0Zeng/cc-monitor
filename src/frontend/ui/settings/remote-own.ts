/**
 * 「cc-monitor 放了什么 → cc-monitor 的文件」远端那台那一块：照那台后端 `data-report` 的 `own`（它自己家里的每一样）排，
 * 与本机那一块同一套行（名字 · 是什么 · 删了会怎样 · 大小 · 打开），打开 ＝ 在文件窗口里打开。只给位置、不给删。
 */
import { copyText } from "../copy-table";
import { formatBytes } from "../format";
import { describeDataClass } from "./data-section";
import type { OwnItem } from "./data-reads";

/** 每一样是什么（`id` 闭集同后端 `footprint/data.rs::own_table`；认不出的照实写「类别未知」那一句的同族说法）。 */
function whatOf(id: string): string {
  switch (id) {
    case "bin":
      return copyText("rsDataPaths.backend.bin");
    case "staging":
      return copyText("rsDataPaths.backend.staging");
    case "relayKey":
      return copyText("rsDataPaths.backend.relayKey");
    case "listenToken":
      return copyText("rsDataPaths.backend.listenToken");
    case "policy":
      return copyText("rsDataPaths.backend.policy");
    case "profiles":
      return copyText("rsDataPaths.backend.profiles");
    case "profilesMigrated":
      return copyText("rsDataPaths.backend.profilesMigrated");
    case "aliasesPosix":
      return copyText("rsDataPaths.backend.aliasesPosix");
    case "aliasesPs":
      return copyText("rsDataPaths.backend.aliasesPs");
    case "skillLedger":
      return copyText("rsDataPaths.backend.skillLedger");
    case "chores":
      return copyText("rsDataPaths.backend.chores");
    case "lastSeen":
      return copyText("rsDataPaths.backend.lastSeen");
    case "assetCatalog":
      return copyText("rsDataPaths.backend.assetCatalog");
    case "quota":
      return copyText("rsDataPaths.backend.quota");
    case "rotation":
      return copyText("rsDataPaths.backend.rotation");
    case "launchAccounts":
      return copyText("rsDataPaths.backend.launchAccounts");
    case "launchNotes":
      return copyText("rsDataPaths.backend.launchNotes");
    case "knownHosts":
      return copyText("rsDataPaths.backend.knownHosts");
    case "accounts":
      return copyText("rsDataPaths.backend.accounts");
    case "accountsMcp":
      return copyText("rsDataPaths.backend.accountsMcp");
    case "extBackups":
      return copyText("rsDataPaths.backend.extBackups");
    default:
      return copyText("dataPage.own.unknownItem", { id });
  }
}

/** `~/.cc-monitor/x` ⇒ 显示名（去掉家那一段；目录带尾 `/`）。 */
function labelOf(o: OwnItem): string {
  const tail = o.path.replace(/^~\/\.cc-monitor\/?/, "");
  return o.dir ? `${tail}/` : tail;
}

/** 那台家里那几样排成一块；`open` 拿那一样的绝对路径（家目录已展开）；`open` 为 `null` ＝ 那台连不上（画的是上次的），打开置灰。 */
export function remoteOwnBlock(items: readonly OwnItem[], home: string, open: ((abs: string) => void) | null): HTMLElement {
  const block = document.createElement("div");
  block.className = "settings-data-block";
  const head = document.createElement("div");
  head.className = "settings-data-block-head";
  const sub = document.createElement("span");
  sub.className = "settings-data-block-subtitle";
  sub.textContent = "~/.cc-monitor";
  head.appendChild(sub);
  block.appendChild(head);
  const list = document.createElement("ul");
  list.className = "settings-data-list";
  for (const o of items) {
    const li = document.createElement("li");
    li.className = `settings-data-item ${o.exists ? "exists" : "absent"}`;
    li.dataset.kind = o.dir ? "dir" : "file";
    li.dataset.class = o.class;
    li.dataset.own = o.id;
    const label = document.createElement("span");
    label.className = "settings-data-item-label";
    label.textContent = labelOf(o);
    const desc = document.createElement("span");
    desc.className = "settings-data-item-desc";
    desc.textContent = whatOf(o.id);
    const cls = document.createElement("span");
    cls.dataset.dataClass = o.class;
    cls.textContent = describeDataClass(o.class);
    const meta = document.createElement("span");
    meta.className = "settings-data-item-meta";
    meta.textContent = !o.exists ? copyText("data.item.notCreated") : o.size !== null ? formatBytes(o.size) : copyText("data.item.created");
    const actions = document.createElement("span");
    actions.className = "settings-data-item-actions";
    const b = document.createElement("button");
    b.type = "button";
    b.className = "settings-data-item-open";
    b.textContent = copyText("dataPage.own.openRemote");
    b.disabled = !o.exists || open === null;
    if (open === null) b.title = copyText("acctPage.offline.hover");
    const abs = o.path.replace(/^~/, home);
    if (o.exists && open !== null) b.addEventListener("click", () => open(abs));
    actions.appendChild(b);
    const path = document.createElement("div");
    path.className = "settings-data-item-path";
    path.textContent = o.path;
    path.title = abs;
    li.append(label, desc, cls, meta, actions, path);
    list.appendChild(li);
  }
  block.appendChild(list);
  return block;
}
