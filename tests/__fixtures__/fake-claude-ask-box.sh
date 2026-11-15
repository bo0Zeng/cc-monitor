#!/usr/bin/env bash
# 替身：画跟 Claude Code（实测 2.1.322）批准框同形的框，供读屏认框的夹具截屏（`claude-ask-box-*.screen.txt`）。不是 claude，不联网，只画字。
# 用法：fake-claude-ask-box.sh '<种类>|<命令>|<说明>[|<标题>]' …   种类空 ⇒ 主会话那一条（标题不带 from、底行不带 ctrl+x 那一截）。
# 几条一起给 ⇒ 排成一队：标题右边画 `k of n`（k 第几条、n 这一批共几条）；按 `1` 或 Esc 换下一条，排空了回到输入框。
# 排法（照窗口宽，`stty size` 读）：
# - 标题与 `k of n` 放得下一行 ⇒ 一行，`k of n` 靠右；放不下 ⇒ 「Bash command」与「· from the <种类> agent」两栏照 flex 收缩分宽、各自折行，`k of n` 不画；
# - 命令连左边一格放得下 ⇒ 原样一行；放不下 ⇒ 每行前面 `│ `，按词折（比一行还长的词就地硬折，wrap-ansi hard 那一种）；
# - Tip · 说明 · 底行放不下 ⇒ 截断、末尾 `…`；选项放不下 ⇒ 按词折、续行缩进 6 格；
# - 框画在窗底；比窗高 ⇒ 上面那一截顶出屏外。
set -u
export LC_ALL=C.UTF-8

read -r ROWS COLS < <(stty size)
queue=("$@")
total=${#queue[@]}
at=0

wrap() { # wrap <宽> <文字>：按词折，超宽的词就地硬折；一行一个输出
  local w=$1 line="" word ch i words
  read -ra words <<<"$2"
  for word in "${words[@]}"; do
    if ((${#word} > w)); then
      if [ -n "$line" ]; then line+=" "; fi
      for ((i = 0; i < ${#word}; i++)); do
        ch=${word:i:1}
        if ((${#line} >= w)); then printf '%s\n' "${line% }"; line=""; fi
        line+=$ch
      done
    elif [ -z "$line" ]; then
      line=$word
    elif ((${#line} + 1 + ${#word} <= w)); then
      line+=" $word"
    else
      printf '%s\n' "$line"
      line=$word
    fi
  done
  printf '%s\n' "$line"
}

cut_to() { # cut_to <宽> <文字>：放不下 ⇒ 截到宽、末尾 `…`
  local w=$1 t=$2
  if ((${#t} <= w)); then printf '%s\n' "$t"; else printf '%s…\n' "${t:0:w-1}"; fi
}

rep() { local s="" i; for ((i = 0; i < $2; i++)); do s+=$1; done; printf '%s' "$s"; }

box() { # box <请求>：框的每一行
  local kind cmd desc title
  IFS='|' read -r kind cmd desc title <<<"$1"
  title=${title:-Bash command}
  local inner=$((COLS - 2)) right="" count=""
  [ -n "$kind" ] && right="· from the $kind agent"
  ((total > 1)) && count="$((at + 1)) of $total"
  if [ -n "$kind" ]; then cut_to "$COLS" "✻ Waiting for 1 background agent to finish"; else cut_to "$COLS" "✻ Thinking…"; fi
  rep ─ "$COLS"; echo
  local head=$title
  [ -n "$right" ] && head+=" $right"
  local need=${#head}
  [ -n "$count" ] && need=$((need + 1 + ${#count}))
  if ((need <= inner)); then
    if [ -n "$count" ]; then
      printf ' %s%*s\n' "$head" $((inner - ${#head})) "$count"
    else
      printf ' %s\n' "$head"
    fi
  else
    # flex 收缩：两栏按原宽分摊超出的那几格，中间隔一格
    local l=${#title} r=${#right}
    local over=$((l + 1 + r - inner))
    local wl=$((l - (over * l + (l + r) / 2) / (l + r)))
    local wr=$((inner - 1 - wl))
    mapfile -t L < <(wrap "$wl" "$title")
    mapfile -t R < <(wrap "$wr" "$right")
    local n=${#L[@]} i
    ((${#R[@]} > n)) && n=${#R[@]}
    for ((i = 0; i < n; i++)); do
      local row
      row=$(printf ' %-*s %s' "$wl" "${L[i]:-}" "${R[i]:-}")
      printf '%s\n' "${row%"${row##*[! ]}"}"
    done
  fi
  printf ' '; cut_to $((inner)) 'Tip: auto mode handles these prompts for you — choose "switch to auto mode" below'
  printf ' '; cut_to "$inner" "$desc"
  rep ╌ "$COLS"; echo
  local l
  if ((${#cmd} <= inner)); then
    printf ' %s\n' "$cmd"
  else
    while IFS= read -r l; do printf ' │ %s\n' "$l"; done < <(wrap $((COLS - 4)) "$cmd")
  fi
  rep ╌ "$COLS"; echo
  echo " Do you want to proceed?"
  local opts=("Yes" "Yes, and always allow access to /work from this project" "Yes, and switch to auto mode · auto mode handles these prompts for you" "No")
  local k first
  for ((k = 0; k < ${#opts[@]}; k++)); do
    first=1
    while IFS= read -r l; do
      if ((first)); then
        if ((k == 0)); then printf ' ❯ %d. %s\n' $((k + 1)) "$l"; else printf '   %d. %s\n' $((k + 1)) "$l"; fi
        first=0
      else
        printf '      %s\n' "$l"
      fi
    done < <(wrap $((COLS - 7)) "${opts[k]}")
  done
  local foot="Esc to cancel · Tab to amend"
  [ -n "$kind" ] && foot+=" · ctrl+x ctrl+k twice to stop background agents"
  printf ' '; cut_to "$inner" "$foot"
}

draw() { # 清屏，把 $@ 这几行画在窗底（多出的顶出屏外），光标停在末行
  local lines=("$@") i
  local pad=$((ROWS - ${#lines[@]}))
  printf '\033[H\033[2J'
  for ((i = 0; i < pad; i++)); do echo; done
  for ((i = 0; i < ${#lines[@]}; i++)); do
    if ((i + 1 < ${#lines[@]})); then printf '%s\n' "${lines[i]}"; else printf '%s' "${lines[i]}"; fi
  done
}

prompt() {
  draw "● main" "" "$(rep ─ "$COLS")" "❯ " "$(rep ─ "$COLS")" "  ? for shortcuts"
}

stty -echo -icanon
while ((at < total)); do
  mapfile -t B < <(box "${queue[at]}")
  draw "${B[@]}"
  while :; do
    IFS= read -rsn1 key
    case $key in
      1 | $'\e') break ;;
    esac
  done
  at=$((at + 1))
done
prompt
while :; do read -rsn1 _ || sleep 3600; done
