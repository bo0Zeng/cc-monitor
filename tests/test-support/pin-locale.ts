/**
 * 〔vitest `setupFiles`〕把 JS 看得见的「默认 locale」钉在 `vitest.config.ts` 那行 `LANG` 上 —— 每个平台走同一条路。
 *
 * 为什么：Linux / macOS 的 ICU 从 `LANG` 取默认 locale；Windows 的 ICU 取系统用户 locale（`GetUserDefaultLocaleName`），
 * env 到不了 ⇒ windows runner 上默认 en-US，秤 2 的时间戳渲成 `12:34 PM`、指纹全漂。Node 22 没有设默认 locale 的开关，
 * 只能在这一层把「没给 locale」（`undefined` / `[]`，即「跟这台机器走」）换成钉住的那个；给了 locale 的调用一概不动。
 */

/** `zh_CN.UTF-8` → `zh-CN`。不是合法的 BCP-47（`C` / 空）⇒ RangeError：钉子坏了就当场红，不静默落回机器的 locale。 */
function pinnedLocale(lang: string | undefined): string {
  return Intl.getCanonicalLocales((lang ?? "").split(/[.@]/)[0].replace("_", "-"))[0];
}

const PIN = pinnedLocale(process.env.LANG);
const MARK = Symbol.for("cc-monitor.test.pinnedLocale");
const pick = (l: unknown): unknown => (l === undefined || (Array.isArray(l) && l.length === 0) ? PIN : l);

// 同一进程里再跑一次（worker 复用）不重复套壳。
if ((globalThis as Record<symbol, unknown>)[MARK] !== PIN) {
  (globalThis as Record<symbol, unknown>)[MARK] = PIN;

  // 吃默认 locale 的 Intl 构造器：`new X()` 与 `X()` 两种调法都认（秤 2 自己就是 `Intl.DateTimeFormat()` 不带 new）。
  const intl = Intl as unknown as Record<string, unknown>;
  for (const name of ["Collator", "DateTimeFormat", "DisplayNames", "DurationFormat", "ListFormat", "NumberFormat", "PluralRules", "RelativeTimeFormat", "Segmenter"]) {
    const C = intl[name];
    if (typeof C !== "function") continue;
    Object.defineProperty(Intl, name, {
      ...Object.getOwnPropertyDescriptor(Intl, name),
      value: new Proxy(C, {
        construct: (t, [l, ...rest], nt) => Reflect.construct(t, [pick(l), ...rest], nt),
        apply: (t, self, [l, ...rest]) => Reflect.apply(t, self, [pick(l), ...rest]),
      }),
    });
  }

  // 吃默认 locale 的原型方法：[原型, 方法名, locales 是第几个实参]。`Array#toLocaleString` 转调元素的方法，不另包。
  const METHODS: [object, string, number][] = [
    [Date.prototype, "toLocaleString", 0],
    [Date.prototype, "toLocaleDateString", 0],
    [Date.prototype, "toLocaleTimeString", 0],
    [Number.prototype, "toLocaleString", 0],
    [BigInt.prototype, "toLocaleString", 0],
    [String.prototype, "localeCompare", 1],
    [String.prototype, "toLocaleLowerCase", 0],
    [String.prototype, "toLocaleUpperCase", 0],
  ];
  for (const [proto, name, at] of METHODS) {
    const orig = (proto as Record<string, (...a: unknown[]) => unknown>)[name];
    Object.defineProperty(proto, name, {
      ...Object.getOwnPropertyDescriptor(proto, name),
      value: {
        [name](this: unknown, ...args: unknown[]) {
          args[at] = pick(args[at]);
          return orig.apply(this, args);
        },
      }[name],
    });
  }
}
