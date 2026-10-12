/*--- rotation
{
  "format": 5,
  "name": "先用剩得多的",
  "line": "先用余量最大的号",
  "settings": {
    "caps": {},
    "capKind": "soft",
    "margin": 10,
    "accounts": [
      {"start": true}
    ]
  }
}
---*/

// 分 ＝ 余量（两窗取小）。在用的号多算「多出几个点才换」（margin，默认 10）：别的号要比它多出这么多才排到它前面，
// 免得两个号余量相近时来回换。在用的号越用越少，次序会在它掉到「下一个号 − margin」那一读数时变：约好读数叫醒（不用另声明随时间变）。
fn decide(ctx) {
  let s = ctx.settings;
  let margin = if s.margin != () { s.margin.to_float() } else { 10.0 };
  let scores = #{};
  let cur = ();
  for id in lib::list(s, ctx) {
    let h = lib::left(s, ctx, id, false);
    scores[id] = if id == ctx.current { h + margin } else { h };
    if id == ctx.current { cur = h; }
  }
  let out = lib::decide_with(ctx, scores);
  if cur != () {
    // 下一个要越过它的号：余量不比它多的那些里最多的一个。
    let next = ();
    for id in lib::list(s, ctx) {
      if id == ctx.current || !lib::usable(s, ctx, id, false) { continue; }
      let h = lib::left(s, ctx, id, false);
      if h <= cur + margin && (next == () || h > next) { next = h; }
    }
    if next != () {
      // 在用号的余量掉到 next − margin 以下时那个号就排到前面了（同分照列表，所以要再多掉一点）：每一窗用到 上限 − (next − margin) 多一点的那一读数。
      for w in ["5h", "7d"] {
        out.wake.when.push(#{ acct: ctx.current, w: w, used_at_least: lib::cap(s, ctx, ctx.current, w, false) - (next - margin) + 0.01 });
      }
    }
  }
  out
}
