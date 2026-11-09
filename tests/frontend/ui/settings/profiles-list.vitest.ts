// 「别名」那一行里的清单：配置文件按「基于」排成树。界面只排版、只发命令 —— 合并 · 标签 · 值的说法 · 连带谁全是后端答的；
// 这里的替身后端故意不合并（合并表原样回），只钉界面把什么交过去、拿回来又画在哪。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import type { ProfileForm, ProfileOp, ProfileRow, ProfilesBook } from "../../../../src/frontend/ui/profiles-reads";

const clip = vi.hoisted(() => ({ writeClipboard: vi.fn(() => Promise.resolve()) }));
vi.mock("../../../../src/frontend/ui/clipboard", () => clip);

const flush = async (): Promise<void> => {
  for (let i = 0; i < 16; i += 1) await Promise.resolve();
};

const form = (name: string, from: string | null): ProfileForm => ({
  name,
  from,
  account: null,
  tmux: null,
  cwdIf: null,
  cwd: null,
  agent: null,
  args: null,
  launcher: null,
  tmuxSize: null,
  detach: false,
  tab: null,
  tabGroup: null,
  say: null,
});

const SLOT_IDS = ["account", "tmux", "cwdIf", "cwd", "agent", "args", "launcher", "tmuxSize", "detach", "tab", "tabGroup", "say"] as const;

const row = (name: string, from: string | null, over: Partial<ProfileRow> = {}): ProfileRow => ({
  name,
  from,
  own: [],
  agent: [],
  usable: true,
  problem: null,
  kind: "link",
  functionWhy: null,
  functionLine: null,
  said: `说：${name}`,
  form: form(name, from),
  accountShape: null,
  badge: null,
  drop: { label: "删（后端给）", kids: [], restores: false },
  start: `ccm @${name}`,
  ...over,
});

describe("buildProfilesList", () => {
  let book: ProfilesBook;
  let writes: Array<{ changes: ProfileOp[]; ifRev: string | null }>;
  let staleOnce: boolean;
  let undoToasts: Array<{ title: string; undo: () => void; commit: () => void }>;

  beforeEach(() => {
    writes = [];
    staleOnce = false;
    book = {
      path: "/h/.cc-monitor/profiles.toml",
      // 那台写好的显示形；界面照抄，不拿家目录自己缩。
      pathShown: "~/.cc-monitor/profiles.toml",
      rev: "fp-1",
      editedAt: null,
      fileProblem: null,
      // 「删」要先处置的那几段（基于它的）由后端算好给出。
      profiles: [
        row("cc", null, { kind: "function", functionWhy: "和 /usr/bin/cc 同名", drop: { label: "删（后端给）", kids: ["workcc", "cct"], restores: false } }),
        row("workcc", "cc"),
        row("cct", "cc", { drop: { label: "删（后端给）", kids: ["teamcct"], restores: false } }),
        row("teamcct", "cct"),
        row("pcc", null),
      ],
      binDir: "/h/.cc-monitor/bin",
      accounts: ["team", "work"],
      tmux: true,
      // 格名是那台后端写好的（`profiles-read` 的 `slots`）；夹具用占位字，判据按它认格名出自成品。
      slots: SLOT_IDS.map((id) => ({ id, label: `L-${id}` })),
      // 三种 tmux 取名各一句（退不退让后端照 `plan.rs` 写好）；夹具用占位字，判据按它认出自成品。
      tmuxNaming: (["auto", "fixed", "base"] as const).map((mode) => ({ mode, text: `N-${mode}` })),
      sets: [],
    };
    vi.resetModules();
    undoToasts = [];
    vi.doMock("../../../../src/frontend/ui/kit/toast", () => ({
      toast: () => undefined,
      undoToast: (title: string, undo: () => void, commit: () => void) => {
        undoToasts.push({ title, undo, commit });
        return () => undefined;
      },
    }));
    vi.doMock("../../../../src/comms/inward/chan", async (orig) => ({
      ChanError: (await orig<typeof import("../../../../src/comms/inward/chan")>()).ChanError,
      chan: { subscribe: () => Promise.resolve({ want: () => undefined, stop: () => undefined }), call: () => Promise.reject(new Error("不该直接问")) },
    }));
    vi.doMock("../../../../src/frontend/ui/profiles-reads", async (orig) => {
      const real = await orig<typeof import("../../../../src/frontend/ui/profiles-reads")>();
      return {
        ...real,
        readProfiles: () => Promise.resolve(structuredClone(book)),
        profileBases: () => Promise.resolve(book.profiles.map((p) => ({ name: p.name, from: p.from, said: p.said, selectable: true }))),
        resolveProfile: (_o: string, name: string) =>
          Promise.resolve({
            chain: ["cc", name],
            rows: [{ key: "cwd-if", slot: "cwdIf", label: "按目录", vals: ["~", "~/x"], said: "在 ~ 敲进 ~/x", from: "cc", overriddenBy: null }],
            line: `LINE ${name}`,
            lineShort: `SHORT ${name}`,
            lineError: null,
            problem: null,
          }),
        profileImpact: () => Promise.resolve([{ name: "teamcct", changes: [{ slot: "tmux", label: "在哪起", before: "A", after: "B" }], problem: null }]),
        writeProfiles: async (_o: string, changes: ProfileOp[], ifRev: string | null) => {
          writes.push({ changes, ifRev });
          if (staleOnce) {
            staleOnce = false;
            // 那台拒了、码 stale（与线上同形：通道里的拒绝体）。
            const { ChanError } = await import("../../../../src/comms/inward/chan");
            const body = new TextEncoder().encode(JSON.stringify({ code: "stale", said: "被别处改过" }));
            throw new ChanError({ layer: "peer", why: "refused", body }, "被别处改过");
          }
          return Promise.resolve({ wrote: true, rev: "fp-2", reload: null });
        },
      };
    });
  });

  afterEach(() => {
    document.body.replaceChildren();
  });

  async function mount(confirm: () => boolean = () => true) {
    const m = await import("../../../../src/frontend/ui/settings/profiles-list");
    const heads: string[] = [];
    const list = m.buildProfilesList({ origin: () => "devbox", local: true, onHead: (t) => heads.push(t), confirm });
    document.body.appendChild(list.element);
    await list.load();
    await flush();
    return { el: list.element, heads, list };
  }

  const names = (el: HTMLElement): string[] => [...el.querySelectorAll<HTMLElement>(".prof-trow")].map((r) => r.textContent ?? "");

  it("按「基于」排成树：子缩进、竖线连着；每行写后端给的那一句；终端函数那条带标；头部那一行是清单给的", async () => {
    const { el, heads } = await mount();
    const rows = names(el);
    expect(rows[0]).toContain("cc" + copyText("profilesPage.chip.function"));
    expect(rows.map((t) => t.split("说：")[0])).toEqual([
      "cc" + copyText("profilesPage.chip.function"),
      "├ workcc",
      "└ cct",
      "  └ teamcct",
      "pcc",
    ]);
    expect(rows[1]).toContain("说：workcc");
    expect(heads.at(-1)).toBe(copyText("profilesPage.head.line", { n: "5", path: "~/.cc-monitor/profiles.toml" }));
  });

  it("点一行：合并表照后端回的画（标签 · 小字原键 · 来自哪一段）·「等于」那一行；改「假设在这个目录敲」重问", async () => {
    const { el } = await mount();
    el.querySelector<HTMLElement>('.prof-trow[data-name="teamcct"]')!.click();
    await flush();
    const box = el.querySelector<HTMLElement>('[data-role="merge"]')!;
    expect(box.textContent).toContain("按目录");
    expect(box.textContent).toContain("cwd-if");
    // 上屏的是核心写好的短形；整条只进「复制整条」。
    expect(box.querySelector('[data-role="equals"]')!.textContent).toBe("SHORT teamcct");
    box.querySelector<HTMLElement>('[data-role="equals-copy"]')!.click();
    expect(clip.writeClipboard).toHaveBeenCalledWith("LINE teamcct");
  });

  it("改一条：表单回填后端给的那一份；存时交 set（带原来的名字与读回时的版本），存完收起、选中它", async () => {
    const { el } = await mount();
    const edit = [...el.querySelectorAll<HTMLElement>('.prof-trow[data-name="cct"] .cfg-link')][0];
    edit.click();
    await flush();
    const f = el.querySelector<HTMLElement>('[data-role="profile-form"]')!;
    expect((f.querySelector('[data-role="name"]') as HTMLInputElement).value).toBe("cct");
    expect(el.querySelector('[data-role="impact-note"]')!.textContent).toContain("teamcct");
    [...f.querySelectorAll<HTMLButtonElement>(".prof-segb")].find((b) => b.textContent === "work")!.click();
    await flush();
    el.querySelector<HTMLButtonElement>('[data-role="save"]')!.click();
    await flush();
    expect(writes).toEqual([{ changes: [{ op: "set", was: "cct", form: { ...form("cct", "cc"), account: { kind: "account", name: "work" } } }], ifRev: "fp-1" }]);
    expect(el.querySelector('[data-role="profile-form"]')).toBeNull();
  });

  it("★ 表单各格的名字照抄后端交来的 slots（界面不另存一份标签表）", async () => {
    const { el } = await mount();
    [...el.querySelectorAll<HTMLElement>('.prof-trow[data-name="cct"] .cfg-link')][0].click();
    await flush();
    const text = el.querySelector<HTMLElement>('[data-role="profile-form"]')!.textContent ?? "";
    for (const id of ["account", "tmux", "cwdIf", "cwd", "agent", "launcher", "tmuxSize", "detach"]) expect(text, id).toContain(`L-${id}`);
    expect(text).not.toContain(copyText("beProfile.slot.cwdIf"));
  });

  it("标签页分组 · 第一句话两格在「更多」里，是文本格；改了存时交进那一段的表单", async () => {
    const { el } = await mount();
    [...el.querySelectorAll<HTMLElement>('.prof-trow[data-name="cct"] .cfg-link')][0].click();
    await flush();
    const f = (): HTMLElement => el.querySelector<HTMLElement>('[data-role="profile-form"]')!;
    for (const id of ["tabGroup", "say"]) {
      const cell = f().querySelector<HTMLElement>(`[data-slot="${id}"]`);
      expect(cell, id).not.toBeNull();
      cell!.querySelector<HTMLElement>(".cfg-link")!.click();
      await flush();
    }
    const put = async (id: string, v: string): Promise<void> => {
      const i = f().querySelector<HTMLInputElement>(`input[data-key="slot.${id}"]`)!;
      i.value = v;
      i.dispatchEvent(new Event("change"));
      await flush();
    };
    await put("tabGroup", "看图");
    await put("say", "读计划图");
    el.querySelector<HTMLButtonElement>('[data-role="save"]')!.click();
    await flush();
    expect(writes).toEqual([{ changes: [{ op: "set", was: "cct", form: { ...form("cct", "cc"), tabGroup: "看图", say: "读计划图" } }], ifRev: "fp-1" }]);
  });

  it("★ 这台没有 tmux（那台后端判的）⇒ tmux 那一格不藏，旁边说一句；有 tmux 的那台不说", async () => {
    book.profiles = book.profiles.map((p) => (p.name === "cct" ? { ...p, form: { ...p.form, tmux: { mode: "auto", name: "" } } } : p));
    for (const has of [true, false]) {
      book.tmux = has;
      const { el } = await mount();
      [...el.querySelectorAll<HTMLElement>('.prof-trow[data-name="cct"] .cfg-link')][0].click();
      await flush();
      const f = el.querySelector<HTMLElement>('[data-role="profile-form"]')!;
      expect(f.querySelector('[data-role="tmux"]'), "格不藏").not.toBeNull();
      expect(f.querySelector('[data-role="no-tmux"]')?.textContent ?? null).toBe(has ? null : copyText("profilesPage.form.noTmux"));
    }
  });

  it("★ tmux 那一格：选哪一种，下面那句与选项的悬停都是后端成品里那一种的说明（界面不另存）", async () => {
    book.profiles = book.profiles.map((p) => (p.name === "cct" ? { ...p, form: { ...p.form, tmux: { mode: "auto", name: "" } } } : p));
    const { el } = await mount();
    [...el.querySelectorAll<HTMLElement>('.prof-trow[data-name="cct"] .cfg-link')][0].click();
    await flush();
    const sel = () => el.querySelector<HTMLSelectElement>('[data-role="tmux"]')!;
    expect(sel(), "找不到 tmux 那个下拉").toBeTruthy();
    for (const v of ["auto", "fixed", "base"] as const) {
      expect([...sel().options].find((x) => x.value === v)!.title).toBe(`N-${v}`);
      sel().value = v;
      sel().dispatchEvent(new Event("change"));
      await flush();
      expect(el.querySelector('[data-role="tmux-naming"]')?.textContent).toBe(`N-${v}`);
    }
  });

  it("存的时候被别处改过：一个字节没写、表单留着、顶上说一句 ＋ 重新读", async () => {
    staleOnce = true;
    const { el } = await mount();
    [...el.querySelectorAll<HTMLElement>('.prof-trow[data-name="workcc"] .cfg-link')][0].click();
    await flush();
    el.querySelector<HTMLButtonElement>('[data-role="save"]')!.click();
    await flush();
    await flush();
    const top = el.querySelector<HTMLElement>('[data-role="form-top"]')!;
    expect(top.textContent).toContain(copyText("profilesPage.form.stale"));
    expect(top.textContent).toContain(copyText("accountChip.menu.refresh"));
    expect(el.querySelector('[data-role="profile-form"]')).not.toBeNull();
  });

  it("★ 打了一半的字遇到一次重读（存时撞上别处改过 ⇒ 重读重画），字还在、焦点还在", async () => {
    staleOnce = true;
    const { el } = await mount();
    [...el.querySelectorAll<HTMLElement>('.prof-trow[data-name="workcc"] .cfg-link')][0].click();
    await flush();
    const name = el.querySelector<HTMLInputElement>('[data-role="profile-form"] input[data-role="name"]')!;
    name.focus();
    name.value = "半截";
    el.querySelector<HTMLButtonElement>('[data-role="save"]')!.click();
    await flush();
    await flush();
    const after = el.querySelector<HTMLInputElement>('[data-role="profile-form"] input[data-role="name"]')!;
    expect(after.value).toBe("半截");
    expect(document.activeElement).toBe(after);
  });

  it("删一条被别人基于的：三个选择、默认推荐改成基于它的父；点执行交 remove + reparent", async () => {
    const { el } = await mount();
    el.querySelector<HTMLElement>('.prof-trow[data-name="cct"] .cfg-link-danger')!.click();
    await flush();
    const r = el.querySelector<HTMLElement>('[data-role="remove"]')!;
    expect(r.textContent).toContain(copyText("profilesPage.remove.reparent", { parent: "cc" }));
    expect(r.textContent).toContain(copyText("profilesPage.remove.cascade"));
    [...r.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("profilesPage.remove.goReparent", { parent: "cc" }))!.click();
    await flush();
    expect(writes.at(-1)!.changes).toEqual([{ op: "remove", name: "cct", children: "reparent" }]);
  });

  it("N10 删一条没人基于的：不问，直接从清单里拿掉 ＋ 8 秒撤销；撤销 ⇒ 原样回来、一个字节不写；到点 ⇒ 才真交 remove", async () => {
    let asked = 0;
    const { el } = await mount(() => {
      asked += 1;
      return true;
    });
    const before = writes.length;
    el.querySelector<HTMLElement>('.prof-trow[data-name="pcc"] .cfg-link-danger')!.click();
    await flush();
    expect(asked).toBe(0);
    expect(el.querySelector('.prof-trow[data-name="pcc"]')).toBeNull();
    expect(writes.length).toBe(before);
    expect(undoToasts.at(-1)!.title).toBe(copyText("profilesPage.remove.done", { name: "pcc" }));
    undoToasts.at(-1)!.undo();
    await flush();
    expect(el.querySelector('.prof-trow[data-name="pcc"]')).not.toBeNull();
    expect(writes.length).toBe(before);
    el.querySelector<HTMLElement>('.prof-trow[data-name="pcc"] .cfg-link-danger')!.click();
    await flush();
    undoToasts.at(-1)!.commit();
    await flush();
    expect(writes.at(-1)!.changes).toEqual([{ op: "remove", name: "pcc" }]);
  });

  it("写坏的那一段标「现在不能用」、摘要换成后端原话；TOML 写坏 ⇒ 清单换成一张卡、没有表单入口", async () => {
    book.profiles[3] = row("teamcct", "cct", { usable: false, problem: { line: 23, message: "第 23 行：未知选项 tmux-sise", detail: null } });
    const { el } = await mount();
    const b = el.querySelector<HTMLElement>('.prof-trow[data-name="teamcct"]')!;
    expect(b.textContent).toContain(copyText("profilesPage.chip.unusable"));
    expect(b.textContent).toContain("第 23 行：未知选项 tmux-sise");
    document.body.replaceChildren();
    book.fileProblem = { line: 3, message: "坏了", detail: "原话：expected `]`" };
    book.profiles = [];
    const again = await mount();
    const card = again.el.querySelector<HTMLElement>('[data-role="file-problem"]')!;
    expect(card.textContent).toContain(copyText("beProfile.at.line", { line: "3", why: "坏了" }));
    expect(card.textContent, "原话不上屏").not.toContain("expected");
    expect(card.querySelector('[data-part="copy-detail"]'), "有详情 ⇒ 句子后面跟［复制详情］").not.toBeNull();
    expect([...again.el.querySelectorAll("button")].map((x) => x.textContent)).not.toContain(copyText("profilesPage.list.add"));
  });

  it("「手改过」那一句只照后端那一格：给了时刻 ⇒ 说、时刻原样；null ⇒ 不说（界面不记、不比）", async () => {
    const { el } = await mount();
    expect(el.querySelector('[data-role="edited-elsewhere"]')).toBeNull();
    document.body.replaceChildren();
    book.editedAt = "10-06 14:20";
    const again = await mount();
    expect(again.el.querySelector('[data-role="edited-elsewhere"]')!.textContent).toContain(copyText("profilesPage.edited.line", { time: "10-06 14:20" }));
  });

  it("「删」那一格照后端给的：内置的带后端给的标、没有这一格；盖着内置的那一格写后端给的字、不问基于它的、当场交 remove（不藏、不撤销）", async () => {
    book.profiles = [
      row("cc", null, { drop: { label: "还原（后端给）", kids: [], restores: true } }),
      row("cch", "cc", { badge: "内置（后端给）", drop: null }),
    ];
    const { el } = await mount();
    const at = (n: string): HTMLElement => el.querySelector<HTMLElement>(`.prof-trow[data-name="${n}"]`)!;
    expect(at("cch").textContent).toContain("内置（后端给）");
    expect(at("cch").querySelector(".cfg-link-danger")).toBeNull();
    const restore = at("cc").querySelector<HTMLElement>(".cfg-link-danger")!;
    expect(restore.textContent).toBe("还原（后端给）");
    restore.click();
    await flush();
    expect(el.querySelector('[data-role="remove"]')).toBeNull();
    expect(undoToasts).toEqual([]);
    expect(writes.at(-1)!.changes).toEqual([{ op: "remove", name: "cc" }]);
  });
});
