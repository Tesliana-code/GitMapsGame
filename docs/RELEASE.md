# GitMapsGame V1 release runbook

**Status: release candidate, locally verified. Nothing below has been executed.** Every step is performed by the founder. Each external step needs its own GO.

GitMapsGame's own world is published to **Cloudflare Pages**:

1. `gitmaps.yml` builds the site with the Rust CLI on a GitHub-hosted runner.
2. Its `publish` job uploads the generated static files with `wrangler pages deploy`.

GitHub Pages is not used for this repository. Third-party installations still use GitHub Pages, as described in [INSTALL.md](INSTALL.md).

**Public START GAME URL:** `https://gitmapsgame.pages.dev/#play`

This assumes the Cloudflare project name `gitmapsgame` is still available; on 2026-10-21 the name did not resolve. If Cloudflare assigns a different `*.pages.dev` name, use that name instead, everywhere below. No custom domain is part of V1.

## 0. Local release-candidate evidence (done)

Run from the WSL checkout on `feat/real-mode-rust`:

```bash
cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings
cargo test --locked && cargo build --release --locked
python3 -m unittest discover -s tests
python3 prototypes/world-map/build_previews.py
NODE_PATH=/tmp/gm-jsdom/node_modules node prototypes/world-map/smoke.cjs
prototypes/world-map/browser_check.sh
```

## 1. Commit and pull request (GO 1)

`feat/real-mode-rust` descends from PR #1's head (`6b91e9f`) through the preservation commit `c31a97e`. Review the working tree, then commit without AI co-author trailers and without changing your Git identity:

```bash
git status --short          # review every path; site/ and target/ are ignored
git add -A
git commit                  # message reviewed by the founder
git push -u origin feat/real-mode-rust
gh pr create --base main --head feat/real-mode-rust --draft \
  --title "GitMapsGame V1: Rust real mode and world-map interface"
```

Then mark PR #1 as superseded and close it. Its commits are included in the new PR.

## 2. GitHub-hosted CI (on the PR)

```bash
gh pr checks <PR> --watch
```

All of these must pass:

| Workflow | Jobs |
|---|---|
| Rust CLI checks | `lint-and-test` (Rust 1.92.0), `runner-default-toolchain`, `msrv` (1.85.0), `oracle-vectors` (Python 3.12) |
| Synthetic playground checks | `mock-tests` (Python suite) |
| GitMaps trigger | `signal`: no permissions, prints one line |

`GitMaps map` does not run from a PR. Record each job's `cargo --version` and the run URLs in the PR.

## 3. Cloudflare and GitHub configuration (GO 2)

| # | Action | How |
|---|---|---|
| 3.1 | Create the Pages project for direct upload, production branch `main` | Cloudflare dashboard → Workers & Pages → Create → Pages → *Upload assets*, project name `gitmapsgame`. Or, logged in to wrangler: `npx wrangler@4.148.0 pages project create gitmapsgame --production-branch main` (needs Node.js 22+) |
| 3.2 | Create an API token | Cloudflare → My Profile → API Tokens → Custom token. Permission **Account · Cloudflare Pages · Edit**, limited to your account. Nothing else. |
| 3.3 | Create the GitHub environment `cloudflare-pages`, deployable from `main` only | Settings → Environments → New environment → Deployment branches: *Selected branches* → `main` |
| 3.4 | Add environment secrets | `gh secret set CLOUDFLARE_API_TOKEN --env cloudflare-pages` and `gh secret set CLOUDFLARE_ACCOUNT_ID --env cloudflare-pages` |
| 3.5 | Add the environment variable | `gh variable set CLOUDFLARE_PAGES_PROJECT --env cloudflare-pages --body gitmapsgame` |

The secrets are visible only to the `publish` job, which runs on `main` only and has no GitHub token scopes. The build job, which queries GitHub, never sees them.

## 4. Merge, first publish and verification (GO 3)

```bash
gh pr merge <PR> --merge                 # after review; this activates gitmaps.yml on main
gh workflow run gitmaps.yml --ref main
gh run watch "$(gh run list --workflow gitmaps.yml --limit 1 --json databaseId --jq '.[0].databaseId')"
```

Verify the published files against a local rebuild:

```bash
mkdir -p /tmp/gm-verify && cd /tmp/gm-verify
for f in index.html map.json snapshot.json SHA256SUMS; do curl -fsSLO "https://gitmapsgame.pages.dev/$f"; done
sha256sum -c SHA256SUMS
cd /mnt/e/GITGAME/GitMapsGame
cargo run --release --locked -- render /tmp/gm-verify/snapshot.json --out /tmp/gm-verify/rebuilt
cmp /tmp/gm-verify/map.json /tmp/gm-verify/rebuilt/map.json && cmp /tmp/gm-verify/index.html /tmp/gm-verify/rebuilt/index.html
```

Expected:

- The `build` job has read-only permissions and compiles the CLI with `cargo build --release --locked`.
- The `publish` job runs in `cloudflare-pages`, passes `sha256sum -c`, and wrangler prints the deployment URL.
- The page opens at `https://gitmapsgame.pages.dev/#play`. GitMapsGame has no crossings yet, so START GAME opens the SETUP QUEST.

## 5. Real crossing and PR sequence (GO 4)

Run [E2E_MANIFEST.md](E2E_MANIFEST.md) Phase 2, issues #2–#9 with PR #8 merged and PR #9 closed, against the live repository. After each step:

1. Record the snapshot locally as the manifest describes.
2. Wait for the triggered `GitMaps map` run.
3. Compare the published `map.json` with the recorded one.

This is the gate for claiming that the map refreshes automatically from GitHub events. Until it passes, the documentation says only that the map reflects GitHub on its next refresh.

## 6. Installation check on a separate repository (GO 5)

1. Remove the pre-release notices (manifest step 4.0), then tag `v1` (manifest 4.1).
2. Create `Tesliana-code/gitmaps-install-test` (manifest 4.2).
3. Follow [INSTALL.md](INSTALL.md) verbatim, using GitHub Pages, and play section 6 end to end (manifest 4.3).
4. Fix any deviation before the Medium article quotes the instructions (manifest 4.4).

## Release gates

| Gate | Evidence |
|---|---|
| Local RC | step 0 output (this branch) |
| Hosted CI | all jobs green on the PR |
| Publication | `sha256sum -c` passes on the live files, and a local re-render matches `map.json` and `index.html` |
| Live gameplay | E2E Phase 2 states observed on the published map after triggered rebuilds |
| Third-party install | INSTALL.md followed verbatim on a fresh repository |
| Publication review | decision on the `cli/cli` recorded fixture (public logins and titles) |
