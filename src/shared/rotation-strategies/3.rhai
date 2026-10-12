// 内置策略 3：快到点、还剩很多的号先用；可在用的号快用完了，就先把它排最高、让它用完，
// 用完（到线，lib::usable 说它不能挑了）以后，快到点的那个就是最高的。
// 别的号按 5h 用得少的先，没开窗的算满额，按量号最后；在用的号还能用、又没有快到点的号 ⇒ 在用的排最高（不换）。
// 输入的号不分先后；同分按号名（引擎的次序）。

fn meta() {
  #{
    format: 1,
    ordered: false,
    params: [
      #{ k: "soon_min",  type: "minutes", init: 60, min: 0, max: 300, label: "p_soon_min" },
      #{ k: "soon_left", type: "points",  init: 40, min: 0, max: 100, label: "p_soon_left" },
      #{ k: "near_left", type: "points",  init: 10, min: 0, max: 50,  label: "p_near_left" },
      #{ k: "near_min",  type: "minutes", init: 20, min: 0, max: 120, label: "p_near_min" },
      #{ k: "delay",     type: "minutes", init: 0,  min: 0, max: 120, label: "p_delay" },
      #{ k: "all_out",   type: "choice",  of: ["stop", "go"], init: "stop", label: "p_all_out" },
    ],
    acct_params: [
      #{ k: "line",  type: "line",  init: 90, label: "p_line" },
      #{ k: "slots", type: "slots", init: [], label: "p_slots" },
    ],
  }
}

fn texts() {
  #{
    zh: #{
      d_soon: "快到点、还剩很多的号先用",
      d_finish: "在用的号快用完就先用完",
      p_soon_min: "快到点（分内）",
      p_soon_left: "还剩至少（点）",
      p_near_left: "在用的号只剩（点）算快用完",
      p_near_min: "或按用速（分）内用完",
      p_delay: "换号前延迟（分）",
      p_all_out: "都到上限后",
      p_all_out_stop: "停下",
      p_all_out_go: "接着用",
      p_line: "用到多少换",
      p_slots: "按时段另算",
      w_expiring: "{at} 到点、还剩 {n} 点，先用",
      w_rest: "5h 用了 {n}%",
      w_fresh: "没开窗",
      w_metered: "按量号，最后用",
      w_finish: "只剩 {n} 点，先用完",
      w_finish_eta: "约 {at} 用完，先用完",
      w_keep: "在用，还能用就不换",
    },
    en: #{
      d_soon: "Use ones about to reset with plenty left first",
      d_finish: "finish the current one if it is nearly used up",
      p_soon_min: "Resetting soon (min)",
      p_soon_left: "At least (pts left)",
      p_near_left: "Current nearly used up (pts left)",
      p_near_min: "or used up within (min, by pace)",
      p_delay: "Delay before switching (min)",
      p_all_out: "When all are at their limit",
      p_all_out_stop: "Stop",
      p_all_out_go: "Keep going",
      p_line: "Switch at",
      p_slots: "By time of day",
      w_expiring: "resets {at} with {n} pts left, use it first",
      w_rest: "5h {n}% used",
      w_fresh: "window not started",
      w_metered: "pay-as-you-go, last",
      w_finish: "{n} pts left, finishing it first",
      w_finish_eta: "runs out about {at}, finishing it first",
      w_keep: "in use, stays while usable",
    },
  }
}

fn describe(p, accounts) { ["d_soon", "d_finish"] + lib::describe_common(p, accounts) }

fn priority(ctx) {
  let p = ctx.params;
  let ck = lib::checks(ctx);
  let out = #{};
  let cur = ();
  let expiring = 0;
  for a in ctx.accounts {
    let u = ck[a.id];
    if a.id == ctx.current { cur = a; }
    if !u.ok { out[a.id] = #{ p: (), why: u.why, back: u.back }; continue; }
    if a.kind == "api" { out[a.id] = #{ p: 50, why: "w_metered" }; continue; }
    if !a.w5h.open { out[a.id] = #{ p: 1100, why: "w_fresh" }; continue; }
    let left_s = a.w5h.resets - ctx.now;
    if a.id != ctx.current && left_s <= p.soon_min * 60 && u.left >= p.soon_left {
      // 快到点、还剩很多：离到点越近，数越大。
      out[a.id] = #{ p: 4000 - left_s / 60, why: #{ k: "w_expiring", at: a.w5h.resets, n: u.left } };
      expiring += 1;
    } else {
      out[a.id] = #{ p: 1000 - a.w5h.used.to_int(), why: #{ k: "w_rest", n: a.w5h.used } };
    }
  }

  if cur != () && ck[cur.id].ok {
    let left = ck[cur.id].left;
    // 在用的号快用完 ⇒ 排最高，先用完（到线后 usable 不让挑它，快到点的那个就是最高的了）。
    if left <= p.near_left {
      out[cur.id] = #{ p: 5000, why: #{ k: "w_finish", n: left } };
    } else if ctx.rate != () && ctx.rate > 0 && left / ctx.rate <= p.near_min {
      out[cur.id] = #{ p: 5000, why: #{ k: "w_finish_eta", at: ctx.now + (left / ctx.rate * 60.0).to_int() } };
    } else if expiring == 0 {
      out[cur.id] = #{ p: 5000, why: "w_keep" };
    }
  }

  out = lib::delay(ctx, out, p.delay);
  lib::all_out(ctx, out)
}
