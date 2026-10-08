// 「别名」那一行里的清单：配置文件按「基于」排成树。界面只排版、只发命令 —— 合并 · 标签 · 值的说法 · 连带谁全是后端答的；
// 这里的替身后端故意不合并（合并表原样回），只钉界面把什么交过去、拿回来又画在哪。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import type { ProfileForm, ProfileOp, ProfileRow, ProfilesBook } from "../../../../src/frontend/ui/profiles-reads";

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
  busRegister: false,
  busNote: null,
});

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
  ...over,
});

describe("buildProfilesList", () => {
  let book: ProfilesBook;
  let writes: Array<{ changes: ProfileOp[]; fingerprint: string | null }>;
  let staleOnce: boolean;
  let undoToasts: Array<{ title: string; undo: () => void; commit: () => void }>;

  beforeEach(() => {
    writes = [];
    staleOnce = false;
    book = {
      home: "/h",
      path: "/h/.cc-monitor/profiles.toml",
      exists: true,
      fingerprint: "fp-1",
      modified: 1,
      fileProblem: null,
      profiles: [row("cc", null, { kind: "function", functionWhy: "和 /usr/bin/cc 同名" }), row("alphacc", "cc"), row("cct", "cc"), row("betacct", "cct"), row("pcc", null)],
      seed: [],
      migrated: null,
      binDir: "/h/.cc-monitor/bin",
      accounts: ["b", "z"],
    };
    vi.resetModules();
    vi.doMock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));
    undoToasts = [];
    vi.doMock("../../../../src/frontend/ui/kit/toast", () => ({
      toast: () => undefined,
      undoToast: (title: string, undo: () => void, commit: () => void) => {
        undoToasts.push({ title, undo, commit });
        return () => undefined;
      },
    }));
    vi.doMock("../../../../src/comms/inward/chan", () => ({
      ChanError: class ChanError extends Error {},
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
            lineError: null,
            problem: null,
          }),
        profileImpact: () => Promise.resolve([{ name: "betacct", changes: [{ slot: "tmux", label: "在哪起", before: "A", after: "B" }], problem: null }]),
        writeProfiles: (_o: string, changes: ProfileOp[], fingerprint: string | null) => {
          writes.push({ changes, fingerprint });
          if (staleOnce) {
            staleOnce = false;
            return Promise.reject(new real.ProfilesStale("被别处改过"));
          }
          return Promise.resolve({ wrote: true, fingerprint: "fp-2", modified: 2, reload: null });
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
      "├ alphacc",
      "└ cct",
      "  └ betacct",
      "pcc",
    ]);
    expect(rows[1]).toContain("说：alphacc");
    expect(heads.at(-1)).toBe(copyText("profilesPage.head.line", { n: "5", path: "~/.cc-monitor/profiles.toml" }));
  });

  it("点一行：合并表照后端回的画（标签 · 小字原键 · 来自哪一段）·「等于」那一行；改「假设在这个目录敲」重问", async () => {
    const { el } = await mount();
    el.querySelector<HTMLElement>('.prof-trow[data-name="betacct"]')!.click();
    await flush();
    const box = el.querySelector<HTMLElement>('[data-role="merge"]')!;
    expect(box.textContent).toContain("按目录");
    expect(box.textContent).toContain("cwd-if");
    expect(box.querySelector('[data-role="equals"]')!.textContent).toBe("LINE betacct");
  });

  it("改一条：表单回填后端给的那一份；存时交 set（带原来的名字与读回时的指纹），存完收起、选中它", async () => {
    const { el } = await mount();
    const edit = [...el.querySelectorAll<HTMLElement>('.prof-trow[data-name="cct"] .cfg-link')][0];
    edit.click();
    await flush();
    const f = el.querySelector<HTMLElement>('[data-role="profile-form"]')!;
    expect((f.querySelector('[data-role="name"]') as HTMLInputElement).value).toBe("cct");
    expect(el.querySelector('[data-role="impact-note"]')!.textContent).toContain("betacct");
    [...f.querySelectorAll<HTMLButtonElement>(".prof-segb")].find((b) => b.textContent === "z")!.click();
    await flush();
    el.querySelector<HTMLButtonElement>('[data-role="save"]')!.click();
    await flush();
    expect(writes).toEqual([{ changes: [{ op: "set", was: "cct", form: { ...form("cct", "cc"), account: { kind: "account", name: "z" } } }], fingerprint: "fp-1" }]);
    expect(el.querySelector('[data-role="profile-form"]')).toBeNull();
  });

  it("存的时候被别处改过：一个字节没写、表单留着、顶上说一句 ＋ 重新读", async () => {
    staleOnce = true;
    const { el } = await mount();
    [...el.querySelectorAll<HTMLElement>('.prof-trow[data-name="alphacc"] .cfg-link')][0].click();
    await flush();
    el.querySelector<HTMLButtonElement>('[data-role="save"]')!.click();
    await flush();
    const top = el.querySelector<HTMLElement>('[data-role="form-top"]')!;
    expect(top.textContent).toContain(copyText("profilesPage.form.stale"));
    expect(top.textContent).toContain(copyText("profilesPage.form.reread"));
    expect(el.querySelector('[data-role="profile-form"]')).not.toBeNull();
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
    book.profiles[3] = row("betacct", "cct", { usable: false, problem: { line: 23, message: "第 23 行：未知选项 tmux-sise" } });
    const { el } = await mount();
    const b = el.querySelector<HTMLElement>('.prof-trow[data-name="betacct"]')!;
    expect(b.textContent).toContain(copyText("profilesPage.chip.unusable"));
    expect(b.textContent).toContain("第 23 行：未知选项 tmux-sise");
    document.body.replaceChildren();
    book.fileProblem = { line: 3, message: "坏了" };
    book.profiles = [];
    const again = await mount();
    expect(again.el.querySelector('[data-role="file-problem"]')!.textContent).toContain(copyText("profilesPage.file.at", { line: "3", e: "坏了" }));
    expect([...again.el.querySelectorAll("button")].map((x) => x.textContent)).not.toContain(copyText("profilesPage.list.add"));
  });

  it("没有配置文件：给首建那两条的预览；点了才写（init seed）", async () => {
    book.exists = false;
    book.profiles = [];
    book.seed = [row("cc", null), row("cct", "cc")];
    const { el } = await mount();
    const seed = el.querySelector<HTMLElement>('[data-role="seed"]')!;
    expect(names(seed)).toHaveLength(2);
    expect(writes).toEqual([]);
    [...seed.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("profilesPage.seed.create"))!.click();
    await flush();
    expect(writes).toEqual([{ changes: [{ op: "init", seed: true }], fingerprint: "fp-1" }]);
  });
});
