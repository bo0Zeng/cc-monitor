/**
 * 按身份原地画（`kit/morph.ts`）：重画不丢正在打的字 · 不丢焦点 · 不丢光标；新树的回调认得的就是人在打字的那一格。
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
  r.appendChild(input);
  return r;
}

describe("morph：整棵新树换进来，正打着的那一格不丢", () => {
  it("输入框有焦点、值为 20:，morph 一棵值为空的新树后：同一格（同一个身份）、焦点还在、值还是 20:、光标还在原处", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    morph(host, [row("a"), row("b")]);
    const input = host.querySelector<HTMLInputElement>('[data-key="b.at"]')!;
    input.focus();
    input.value = "20:";
    input.setSelectionRange(2, 2);
    const fresh = [row("a"), row("b"), row("c")];
    morph(host, fresh);
    const after = host.querySelector<HTMLInputElement>('[data-key="b.at"]')!;
    expect((after.parentElement as HTMLElement).dataset.key).toBe("b");
    expect(fresh[1].contains(after), "是新树里那一格（新树的回调认得它）").toBe(true);
    expect(document.activeElement).toBe(after);
    expect(after.value).toBe("20:");
    expect(after.selectionStart).toBe(2);
    expect(host.querySelectorAll("[data-key$='.at']").length).toBe(3);
  });

  it("没焦点但改过没交的框（值不等于画出来时的值）也留着字；没改过的照新树的值", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    morph(host, [row("a", "1"), row("b", "2")]);
    host.querySelector<HTMLInputElement>('[data-key="a.at"]')!.value = "改了";
    morph(host, [row("a", "9"), row("b", "9")]);
    expect(host.querySelector<HTMLInputElement>('[data-key="a.at"]')!.value).toBe("改了");
    expect(host.querySelector<HTMLInputElement>('[data-key="b.at"]')!.value).toBe("9");
  });

  it("新树里没有那个身份了 ⇒ 那一格跟着没（不凭空留一格）", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    morph(host, [row("a")]);
    const a = host.querySelector<HTMLInputElement>("input")!;
    a.focus();
    a.value = "x";
    morph(host, [row("b")]);
    expect(host.querySelector('[data-key="a.at"]')).toBeNull();
    expect(host.querySelector('[data-key="b.at"]')).not.toBeNull();
  });

  it("同一行的按钮读的就是人打过字的那一格（新树的闭包捕获的是新画的那一格）", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    let sent = "";
    const mk = (): HTMLElement => {
      const r = document.createElement("div");
      const msg = document.createElement("input");
      msg.dataset.key = "msg";
      const send = document.createElement("button");
      send.addEventListener("click", () => (sent = msg.value));
      r.append(msg, send);
      return r;
    };
    morph(host, [mk()]);
    host.querySelector("input")!.value = "你好";
    morph(host, [mk()]);
    host.querySelector("button")!.click();
    expect(sent).toBe("你好");
  });
});

describe("morph：画出来时的值就是基准", () => {
  it("没人改过 ⇒ 下一次照新值；人改了 ⇒ 留着；交上去后端也回成同一个值 ⇒ 又照新值", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const mk = (v: string): HTMLInputElement => {
      const i = document.createElement("input");
      i.dataset.key = "k";
      i.value = v;
      return i;
    };
    const val = (): string => host.querySelector("input")!.value;
    morph(host, [mk("x")]);
    morph(host, [mk("y")]);
    expect(val()).toBe("y");
    host.querySelector("input")!.value = "人打的";
    morph(host, [mk("z")]);
    expect(val()).toBe("人打的");
    morph(host, [mk("人打的")]);
    morph(host, [mk("w")]);
    expect(val()).toBe("w");
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

describe("morph：调用方自己留着的那个节点（原样交回）", () => {
  it("焦点 · 字 · 光标都还在", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const wrap = document.createElement("div");
    const i = document.createElement("input");
    i.dataset.key = "k";
    wrap.appendChild(i);
    morph(host, [wrap]);
    i.focus();
    i.value = "xy";
    i.setSelectionRange(1, 1);
    // 直接交回那个节点（先挪进别的新容器会当场失焦 —— 那样的画法该整棵新画、交给 morph 搬值）。
    morph(host, [document.createElement("hr"), wrap]);
    expect([document.activeElement === i, i.value, host.contains(i), i.selectionStart]).toEqual([true, "xy", true, 1]);
  });
});
