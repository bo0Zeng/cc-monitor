/*--- rotation
{
  "format": 5,
  "name": "先用剩得多的",
  "line": "先用余量最大的号",
  "timeVarying": true,
  "settings": {
    "caps": {},
    "capKind": "soft",
    "accounts": [
      {"start": true}
    ]
  }
}
---*/

// 分 ＝ 余量（两窗取小）。在用的号一直在掉，次序会在它掉到下一个号下面那一刻变：
// 按它的用速算出那一刻交给后端（next_change_at），推演与真用都在那一刻重判。
fn score(ctx) {
  let out = #{};
  let cur = ();
  for a in ctx.accounts {
    out[a.id] = #{ score: a.H, why: "余量 " + a.H.to_int() + "%" };
    if a.id == ctx.current { cur = a; }
  }
  let res = #{ scores: out };
  if cur == () { return res; }
  // 下一个要被越过的号：余量不比在用的号多的那些里最多的一个。
  let next = ();
  for a in ctx.accounts {
    if a.id != ctx.current && a.ok && a.H <= cur.H && (next == () || a.H > next) { next = a.H; }
  }
  if next == () { return res; }
  // 两个窗口各自掉到它之下的时刻，取先到的那个（余量是两窗取小）。
  let soonest = ();
  for w in ["w5h", "w7d"] {
    let r = cur.rate[w];
    let room = cur.cap[w] - cur.used[w];
    if r != () && r > 0.0 && room >= next {
      let at = ctx.now + ((room - next) / r * 60.0).to_int() + 1;
      if soonest == () || at < soonest { soonest = at; }
    }
  }
  if soonest != () { res.next_change_at = soonest; }
  res
}
