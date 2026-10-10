/**
 * 判据 / 截图台架的假后端交骨架索引用：逐行 ⇒ 帧面那一形（按列排）。口径照后端 `faces/read_face.rs::pack_index`
 * （那边的判据 `the_index_is_packed_by_column_with_offsets_as_differences` 钉形状，跨语言金样钉界面读法）。
 */
type Row = Record<string, unknown>;

export function packIndex(from: number, end: number, rows: readonly Row[]): Record<string, unknown> {
  const num = (r: Row, k: string): number => (typeof r[k] === "number" ? (r[k] as number) : 0);
  const classes: string[] = [];
  const [o, n, t, fd, u]: unknown[][] = [[], [], [], [], []];
  const table = (keys: string[]) => {
    const tb: Record<string, unknown[]> = { at: [] };
    for (const k of keys) tb[k] = [];
    let last = -1;
    return {
      tb,
      push(i: number, vals: unknown[]): void {
        tb.at.push(i - last - 1);
        last = i;
        keys.forEach((k, j) => tb[k].push(vals[j]));
      },
    };
  };
  const BODY = ["ch", "cj", "pl", "cb", "cl"];
  const [body, sp, inputs] = [table(BODY), table(["v"]), table(["x", "ts"])];
  let prev = from;
  rows.forEach((r, i) => {
    o.push(num(r, "o") - prev);
    n.push(num(r, "n"));
    prev = num(r, "o") + num(r, "n");
    if (typeof r.t === "string") {
      if (!classes.includes(r.t)) classes.push(r.t);
      t.push(classes.indexOf(r.t) + 1);
    } else t.push(0);
    fd.push(num(r, "fd"));
    u.push(typeof r.u === "string" ? r.u : null);
    if (BODY.some((k) => num(r, k) !== 0)) body.push(i, BODY.map((k) => num(r, k)));
    if (typeof r.sp === "string") sp.push(i, [r.sp]);
    if (typeof r.x === "string") inputs.push(i, [r.x, typeof r.ts === "string" ? r.ts : ""]);
  });
  return { from, end, count: rows.length, classes, o, n, t, fd, u, body: body.tb, sp: sp.tb, inputs: inputs.tb };
}
