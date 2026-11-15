/**
 * **标签栏顶那一排**：照用户排的那一份（`config.json` 的 `tabBar.head`，读写 `global-dests.ts`）摆入口清单里的那几颗（`global-dests.ts`），
 * 右端「更多 ⋯」恒在最右：菜单里是不在栏上的那几项（照清单先后）＋ 末一行「调整栏顶按钮…」。栏顶右键 ⇒ 开编辑那一处（设置页「栏顶按钮」）。
 * 每一颗点了跑什么由宿主给（`main.ts`）；这里只摆。
 */
import { copyText } from "./copy-table";
import { icon } from "./kit/icon";
import { attachTooltip } from "./kit/tooltip";
import { openMenu, type MenuItem } from "./kit/menu";
import { DEFAULT_HEAD, HEAD_ENTRIES, overflowOf, headEntry, type HeadId } from "./global-dests";

export interface TabBarHeadHost {
  /** 每一项点了跑什么（`anchor` ＝ 点的那一颗：栏上那颗或 ⋯）。 */
  run: Readonly<Record<HeadId, (anchor: HTMLButtonElement) => void>>;
  /** 那一项此刻的键位（写好的字；没有 ⇒ `undefined`）。 */
  chord(id: HeadId): string | undefined;
  /** 把这一排交给标签栏（`TabManager.mountHeadActions`）。 */
  mount(buttons: HTMLElement[], end: HTMLElement[]): void;
  /** 开编辑那一处（设置页「栏顶按钮」那一节）。 */
  edit(): void;
}

export class TabBarHead {
  /** 右端「更多 ⋯」（计划那一侧有需手动的 ⇒ 宿主挂 `data-needs`）。 */
  readonly more: HTMLButtonElement;
  private readonly buttons = new Map<HeadId, HTMLButtonElement>();
  private head: HeadId[] = [...DEFAULT_HEAD];
  /** 栏顶那一行（标签栏的 `.tab-bar-head`，摆上之后才有）：右键监听挂在它上面、只挂一次。 */
  private row: HTMLElement | null = null;

  constructor(private readonly host: TabBarHeadHost) {
    for (const e of HEAD_ENTRIES) {
      const b = this.iconButton(e.id, e.icon, e.label(), () => this.hintOf(e.id));
      b.addEventListener("click", () => host.run[e.id](b));
      this.buttons.set(e.id, b);
    }
    // 起会话那两颗留着老的角色名（截图台架按它点）。
    this.buttons.get("new")!.dataset.role = "new-session";
    this.buttons.get("sets")!.dataset.role = "set-menu";
    const more = copyText("history.row.more");
    this.more = this.iconButton("more", "more", more, () => more);
    this.more.classList.add("head-more-trigger");
    this.more.addEventListener("click", () => {
      openMenu({ el: this.more, align: "end" }, this.moreItems());
    });
  }

  /** 照这一排摆（⋯ 恒在最右）；摆上了 ⇒ 整条栏顶（按钮之间的空白也算）右键开编辑那一处。 */
  show(head: readonly HeadId[]): void {
    this.head = [...head];
    this.host.mount(
      this.head.map((id) => this.buttons.get(id)!),
      [this.more],
    );
    const row = this.more.closest<HTMLElement>(".tab-bar-head");
    if (row !== null && row !== this.row) {
      this.row = row;
      row.addEventListener("contextmenu", (e) => {
        e.preventDefault();
        e.stopPropagation();
        this.host.edit();
      });
    }
  }

  /** 那一项此刻的锚：在栏上 ⇒ 那一颗；不在 ⇒ ⋯。 */
  anchorOf(id: HeadId): HTMLButtonElement {
    return this.head.includes(id) ? this.buttons.get(id)! : this.more;
  }

  /** ⋯ 里那几行：不在栏上的（照清单先后，右侧写键位）＋ 分隔 ＋「调整栏顶按钮…」。 */
  moreItems(): MenuItem[] {
    const items: MenuItem[] = overflowOf(this.head).map((id) => {
      const e = headEntry(id);
      return { id, icon: e.icon, label: e.label(), detail: this.host.chord(id), onClick: () => this.host.run[id](this.more) };
    });
    if (items.length > 0) items.push({ divider: true, label: "" });
    items.push({ icon: "sliders", label: copyText("tabBarHead.edit.open"), onClick: () => this.host.edit() });
    return items;
  }

  private hintOf(id: HeadId): string {
    const name = headEntry(id).label();
    const chord = this.host.chord(id);
    return chord ? `${name} · ${chord}` : name;
  }

  /** 栏顶一颗图标按钮（`styles.css` 的 `.tab-bar-icon`）。 */
  private iconButton(id: HeadId | "more", name: Parameters<typeof icon>[0], label: string, hint: () => string): HTMLButtonElement {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "tab-bar-icon";
    b.dataset.head = id;
    b.setAttribute("aria-label", label);
    b.appendChild(icon(name));
    attachTooltip(b, hint);
    return b;
  }
}
