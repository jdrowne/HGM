#!/bin/bash

input=$(cat)
ctx=$(echo "$input" | jq -r '.context_window as $w
  | if $w.current_usage then
      ($w.current_usage | (.input_tokens // 0) + (.cache_creation_input_tokens // 0) + (.cache_read_input_tokens // 0))
    elif $w.used_percentage and $w.context_window_size then
      $w.used_percentage * $w.context_window_size / 100
    else empty end')
size=$(echo "$input" | jq -r '.context_window.context_window_size // empty')
model_id=$(echo "$input" | jq -r '.model.id // empty')
project=$(echo "$input" | jq -r '.workspace.project_dir // .cwd // empty')
ceiling=$(for f in ~/.claude/settings.json "$project/.claude/settings.json" "$project/.claude/settings.local.json"; do
  [ -f "$f" ] && jq -r --arg m "$model_id" '[.modelSettings[$m].autoCompactWindow, .autoCompactWindow]
    | map(select(type == "number")) | first // empty' "$f" 2>/dev/null
done | tail -n 1)
[ -n "$size" ] && { [ -z "$ceiling" ] || [ "$ceiling" -gt "$size" ]; } && ceiling=$size
wk=$(echo "$input" | jq -r '.rate_limits.seven_day.used_percentage // empty')
model=$(echo "$input" | jq -r '.model.display_name // .model.id // empty')
effort=$(echo "$input" | jq -r '.effort.level // empty')
cwd=$(echo "$input" | jq -r '.workspace.current_dir // .cwd // empty')

full=$(printf '\342\226\210')
empty=$(printf '\342\226\221')

bar() {
  local n f b i
  n=$(printf '%.0f' "$2")
  f=$((n / 10))
  [ "$f" -gt 10 ] && f=10
  [ "$f" -lt 0 ] && f=0
  b=''
  for ((i = 1; i <= 10; i++)); do
    if [ "$i" -le "$f" ]; then b="${b}${full}"; else b="${b}${empty}"; fi
  done
  printf '%s %s %s%%' "$1" "$b" "$n"
}

stale() {
  [ ! -s "$1" ] || [ -n "$(find "$1" -mmin +2 2>/dev/null)" ]
}

fable_cache="${TMPDIR:-/tmp}/claude-statusline-fable-weekly"
if stale "$fable_cache" && command -v security >/dev/null 2>&1; then
  token=$(security find-generic-password -s 'Claude Code-credentials' -w 2>/dev/null | jq -r '.claudeAiOauth.accessToken // empty' 2>/dev/null)
  if [ -n "$token" ]; then
    pct=$(curl -s -m 5 https://api.anthropic.com/api/oauth/usage \
      -H "Authorization: Bearer $token" -H 'anthropic-beta: oauth-2025-04-20' 2>/dev/null |
      jq -r 'first(.limits[]? | select(.kind == "weekly_scoped" and .scope.model.display_name == "Fable") | .percent) // empty' 2>/dev/null)
    [ -n "$pct" ] && printf '%s' "$pct" > "$fable_cache"
  fi
fi
fable=$(cat "$fable_cache" 2>/dev/null)

out=''
if [ -n "$ctx" ]; then
  context=$(printf 'Context %.0fK' "$(echo "$ctx / 1000" | bc -l)")
  [ -n "$ceiling" ] && context="${context}/$(printf '%.0fK' "$(echo "$ceiling / 1000" | bc -l)")"
  out="${out}${out:+  |  }${context}"
fi
[ -n "$wk" ] && out="${out}${out:+  |  }$(bar Weekly "$wk")"
[ -n "$fable" ] && out="${out}${out:+  |  }$(bar Fable "$fable")"

if [ -n "$cwd" ]; then
  cache="${TMPDIR:-/tmp}/claude-statusline-issues-$(printf '%s' "$cwd" | cksum | cut -d' ' -f1)"
  trend_file="${cache}-trend"
  if stale "$cache"; then
    # Count issues on origin, not gh's default repo: in a fork, gh resolves to the upstream parent.
    read -r repo count < <(cd "$cwd" 2>/dev/null && {
      origin=$(git remote get-url origin 2>/dev/null)
      GH_PROMPT_DISABLED=1 gh repo view ${origin:+"$origin"} --json nameWithOwner,issues \
        -q '"\(.nameWithOwner) \(.issues.totalCount)"' 2>/dev/null
    })
    if [ -n "$count" ]; then
      printf '%s' "$count" > "$cache"
      since=$(date -u -v-24H +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -u -d '24 hours ago' +%Y-%m-%dT%H:%M:%SZ)
      delta=$(GH_PROMPT_DISABLED=1 gh api graphql \
        -f opened="repo:$repo is:issue created:>=$since" -f closed="repo:$repo is:issue closed:>=$since" \
        -f query='query($opened: String!, $closed: String!) {
          opened: search(type: ISSUE, query: $opened) { issueCount }
          closed: search(type: ISSUE, query: $closed) { issueCount } }' \
        --jq '.data.opened.issueCount - .data.closed.issueCount' 2>/dev/null)
      if [ -n "$delta" ]; then
        if [ "$delta" -gt 0 ]; then
          printf '\342\226\262' > "$trend_file"
        elif [ "$delta" -lt 0 ]; then
          printf '\342\226\274' > "$trend_file"
        else
          : > "$trend_file"
        fi
      fi
    fi
  fi
  issues=$(cat "$cache" 2>/dev/null)
  trend=$(cat "$trend_file" 2>/dev/null)
  [ -n "$issues" ] && out="${out}${out:+  |  }Open issues: ${issues}${trend:+ $trend}"
fi

modeleffort="$model"
[ -n "$effort" ] && modeleffort="${modeleffort}${modeleffort:+ }${effort} effort"
[ -n "$modeleffort" ] && out="${out}${out:+  |  }${modeleffort}"

printf '%s' "$out"
