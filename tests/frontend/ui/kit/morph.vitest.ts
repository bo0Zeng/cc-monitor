/**
 * 按身份原地画（`kit/morph.ts`）：重画不丢正在打的字 · 不丢焦点；同一个身份的节点留着、只挪位置。
 */
import { describe, it, expect, beforeEach } from "vitest";
import { morph, place } from "../../../../src/frontend/ui/kit/morph";

beforeEach(() => document.body.replaceChildren());

function row(key: string, value = ""): HTMLElement {
  const r = document.createElement("div");
  r.dataset.key = key;
  const input = document.createElement("input");
  input.dataset.key = `${key}.at`;
  input.value = value;
  input.defaultValue = value;
  r.appendChild(input);
  return r;
}

describe("morph：整棵新树换进来，正打着的那一格不碰", () => {
  it("输入框有焦点、值为 20:，morph 一棵值为空的新树后：还是同一个节点、焦点还在、值还是 20:、光标还在末尾", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    host.append(row("a"), row("b"));
    const input = host.querySelector<HTMLInputElement>('[data-key="b.at"]')!;
    input.focus();
    input.value = "20:";
    input.setSelectionRange(3, 3);
    morph(host, [row("a"), row("b"), row("c")]);
    const after = host.querySelector<HTMLInputElement>('[data-key="b.at"]')!;
    expect(after).toBe(input);
    expect(document.activeElement).toBe(input);
    expect(after.value).toBe("20:");
    expect(after.selectionStart).toBe(3);
    expect(host.querySelectorAll("[data-key$='.at']").length).toBe(3);
  });

  it("没焦点但改过没交的框（值不等于画出来时的值）也留着；没改过的照新树换", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    host.append(row("a", "1"), row("b", "2"));
    const a = host.querySelector<HTMLInputElement>('[data-key="a.at"]')!;
    const b = host.querySelector<HTMLInputElement>('[data-key="b.at"]')!;
    a.value = "改了";
    morph(host, [row("a", "9"), row("b", "9")]);
    expect(host.querySelector('[data-key="a.at"]')).toBe(a);
    expect(a.value).toBe("改了");
    const nb = host.querySelector<HTMLInputElement>('[data-key="b.at"]')!;
    expect(nb).not.toBe(b);
    expect(nb.value).toBe("9");
  });

  it("新树里没有那个身份了 ⇒ 留着的那一格跟着没（不凭空留一格）", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    host.append(row("a"));
    const a = host.querySelector<HTMLInputElement>("input")!;
    a.focus();
    a.value = "x";
    morph(host, [row("b")]);
    expect(host.contains(a)).toBe(false);
    expect(host.querySelector('[data-key="b.at"]')).not.toBeNull();
  });
});

describe("morph：画出来时的值就是基准", () => {
  it("只设了 value（没设 defaultValue）的框：没人改过 ⇒ 下一次照新值换；人改了 ⇒ 留着", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const mk = (v: string): HTMLInputElement => {
      const i = document.createElement("input");
      i.dataset.key = "k";
      i.value = v;
      return i;
    };
    morph(host, [mk("x")]);
    morph(host, [mk("y")]);
    const i = host.querySelector("input")!;
    expect(i.value).toBe("y");
    i.value = "人打的";
    morph(host, [mk("z")]);
    expect(host.querySelector("input")).toBe(i);
    expect(i.value).toBe("人打的");
    // 交上去了、后端也回成同一个值 ⇒ 没在改了：下一次照新值换
    morph(host, [mk("人打的")]);
    morph(host, [mk("w")]);
    expect(host.querySelector("input")!.value).toBe("w");
  });
});

describe("morph：焦点在一行上（不是输入框）", () => {
  it("新树里同一个 data-key 的那一行接过焦点", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const mk = (k: string): HTMLElement => {
      const r = document.createElement("div");
      r.dataset.key = k;
      r.tabIndex = 0;
      r.appendChild(document.createElement("span"));
      return r;
    };
    host.append(mk("a"), mk("b"));
    host.querySelector<HTMLElement>('[data-key="b"]')!.focus();
    morph(host, [mk("b"), mk("a")]);
    expect((document.activeElement as HTMLElement).dataset.key).toBe("b");
    expect(host.contains(document.activeElement)).toBe(true);
  });
});

describe("place：容器的孩子换成这一串，留着的节点原地不动", () => {
  it("次序变了只挪不建；焦点在挪过的那一个上 ⇒ 还在", () => {
    const host = document.createElement("ul");
    document.body.appendChild(host);
    const [a, b, c] = ["a", "b", "c"].map((k) => {
      const li = document.createElement("li");
      li.tabIndex = 0;
      li.dataset.key = k;
      return li;
    });
    host.append(a, b, c);
    a.focus();
    place(host, [c, b, a]);
    expect([...host.children]).toEqual([c, b, a]);
    expect(document.activeElement).toBe(a);
    place(host, [b]);
    expect([...host.children]).toEqual([b]);
  });
});
