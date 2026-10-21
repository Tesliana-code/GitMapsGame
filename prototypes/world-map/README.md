# World-map previews and QA tooling

The founder-approved world-map design was **promoted to production `index.html` on 2026-10-21** and is frozen. Its `<style>` block is hash-locked by `tests/test_gitmaps_site.py`. This folder now holds only the preview builder and the QA checks, and they render the production template.

## Build the four local previews

```bash
python3 prototypes/world-map/build_previews.py            # renders production index.html; --fix-csp refreshes its script hash
```

Output goes to `site/prototype/`, which is gitignored:

| Preview | Data |
|---|---|
| `real-populated/` | Frozen recorded `cli/cli` snapshot: real GitHub data, no dependency edges |
| `real-empty/` | Frozen recorded `Tesliana-code/GitMapsGame` snapshot: real, no crossings |
| `tutorial/` | No data: the fictional tutorial world |
| `constructed-stress/` | Constructed test world, not from a live repository. Labelled by one 10px footnote ("TEST WORLD · CONSTRUCTED DATA · LOCAL PREVIEW") and the Evidence drawer. Links disabled, local only, never publish. |

## Test

```bash
npm install --prefix /tmp/gm-jsdom jsdom@24.1.3
NODE_PATH=/tmp/gm-jsdom/node_modules node prototypes/world-map/smoke.cjs   # interaction checks (jsdom)
prototypes/world-map/browser_check.sh                                      # real Chromium (Edge, headless), CSP enforced
```

## Open in Opera (from WSL)

```bash
OPERA="/mnt/c/Users/Super administrator/AppData/Local/Programs/Opera/opera.exe"
"$OPERA" "file:///E:/GITGAME/GitMapsGame/site/prototype/real-populated/index.html"
"$OPERA" "file:///E:/GITGAME/GitMapsGame/site/prototype/real-empty/index.html"
"$OPERA" "file:///E:/GITGAME/GitMapsGame/site/prototype/tutorial/index.html"
"$OPERA" "file:///E:/GITGAME/GitMapsGame/site/prototype/constructed-stress/index.html"
```

Deep links to try: `#play`, `#progress`, `#player=babakks` (real-populated), `#crossing=3718`, `#tutorial` (real-empty).

AI assistance: the design implementation, this generator and these tests were produced with AI assistance (Claude Code) for founder review.
