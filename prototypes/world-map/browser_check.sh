#!/usr/bin/env bash
# Real-browser smoke check of the four prototype previews (WSL + a Windows Chromium browser).
# The browser enforces the page CSP, so a stale script hash leaves the map empty and this check fails.
# Each headless run uses its own throwaway profile under the Windows temp folder (a shared profile
# can be held by a lingering instance); your own browser profile and windows are never touched.
#
#   prototypes/world-map/browser_check.sh
#
# Uses headless Microsoft Edge (Chromium, the same engine as Opera). Opera itself has no headless
# --dump-dom mode, so Opera is checked by hand with the launch commands in README.md.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BROWSER="/mnt/c/Program Files (x86)/Microsoft/Edge/Application/msedge.exe"
WIN_TEMP="$(cmd.exe /c 'echo %TEMP%' 2>/dev/null | tr -d '\r')"
WORK="$(wslpath "$WIN_TEMP")/gitmaps-browser-check"
mkdir -p "$WORK" "$ROOT/site/prototype/_screens"
fail=0
declare -A TILES=([real-populated]=9 [real-empty]=0 [tutorial]=4 [constructed-stress]=33)
for p in real-populated real-empty tutorial constructed-stress; do
  url="file:///$(wslpath -w "$ROOT/site/prototype/$p/index.html" | sed 's#\\#/#g')"
  dom="$(cd "$WORK" && timeout 90 "$BROWSER" --headless=new --disable-gpu --no-first-run --user-data-dir="$WIN_TEMP\\gitmaps-browser-check\\profile-$p-dom-$$" --dump-dom "$url" 2>/dev/null || true)"
  tiles=$( (grep -o 'class="tile"' <<<"$dom" || true) | wc -l)
  pixels=$( (grep -o '<svg class="px"' <<<"$dom" || true) | wc -l)
  status=$( (grep -o '<div class="statusbar"[^>]*>.*</div>' <<<"$dom" || true) | head -1 | sed 's/<[^>]*>/ /g' | tr -s ' ')
  (cd "$WORK" && timeout 90 "$BROWSER" --headless=new --disable-gpu --no-first-run --user-data-dir="$WIN_TEMP\\gitmaps-browser-check\\profile-$p-shot-$$" \
     --hide-scrollbars --window-size=1360,1700 --screenshot="$WIN_TEMP\\gitmaps-browser-check\\$p.png" "$url" >/dev/null 2>&1 || true)
  cp "$WORK/$p.png" "$ROOT/site/prototype/_screens/$p.png" 2>/dev/null || true
  if [ "$tiles" -eq "${TILES[$p]}" ] && [ "$pixels" -ge 7 ] && [ -n "$status" ]; then
    echo "PASS $p: script ran under CSP; tiles=$tiles pixel-lettering=$pixels;$status"
  else
    echo "FAIL $p: tiles=$tiles (want ${TILES[$p]}) pixel-lettering=$pixels status='$status'"; fail=1
  fi
done
rm -rf "$WORK"/profile-*-$$ 2>/dev/null || true
echo "screenshots: $ROOT/site/prototype/_screens/"
exit $fail
