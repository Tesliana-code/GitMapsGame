# Real-repository E2E manifest: Tesliana-code/GitMapsGame

**Status: proposed. Nothing in this document has been executed.** Each phase needs a separate founder GO. Commands run as the founder's `gh` account (`Tesliana-code`). No other credentials are needed.

The repository currently has one PR (#1) and no issues, so new numbers start at **#2**. Before each step, the operator confirms that the expected number is still correct. If it is not, every later `#N` in this manifest shifts by the same amount.

## Phase 0: Local readiness (done locally, no external effect)

Run on the founder's WSL checkout of `feat/real-mode-rust`; each must pass with the network disabled:

- `cargo fmt --check`
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo test --locked`
- `cargo build --release --locked`
- `cargo run -- check-template` (the CSP hash matches the shipped script)
- `python3 -m unittest discover -s tests` (synthetic simulator and the Python reference oracle)

## Phase 1: Publish the reviewed branch (GO 1)

The local branch `feat/real-mode-rust` is built on the preservation commit `c31a97e`, which in turn builds on PR #1's head `6b91e9f`. The founder decides whether it updates PR #1 or opens a separate PR. The founder performs every Git operation.

| # | Mutation | Command |
|---|---|---|
| 1.1 | Commit the reviewed work on `feat/real-mode-rust` | founder, in WSL |
| 1.2 | Push the branch and open or update the PR | founder, in WSL |

Expected on the PR:

- `Rust CLI checks`:
  - fmt, clippy, test and release build on Rust 1.92.0;
  - test and release build on the runner's default Rust;
  - MSRV 1.85.0;
  - oracle-vector sync.
- `Synthetic playground checks` runs the Python suite.
- `GitMaps trigger` runs with no permissions and prints one line.
- `GitMaps map` does **not** run, because it is not on `main` yet.

## Phase 2: Real issues, evidence recorded locally (GO 2)

No workflow or Pages involvement. After each step, record the evidence into a new directory. The six frozen fixtures in `tests/fixtures/recorded/` are never touched:

```bash
mkdir -p tests/fixtures/e2e
GH_TOKEN="$(gh auth token)" cargo run --release --locked -- snapshot --repo Tesliana-code/GitMapsGame \
  --record tests/fixtures/e2e/STEP.recording.json -o tests/fixtures/e2e/STEP.snapshot.json
cargo run --release --locked -- derive tests/fixtures/e2e/STEP.snapshot.json -o tests/fixtures/e2e/STEP.map.json
python3 -m gitmaps derive tests/fixtures/e2e/STEP.snapshot.json -o /tmp/STEP.reference.map.json   # oracle cross-check
cmp tests/fixtures/e2e/STEP.map.json /tmp/STEP.reference.map.json
```

| Step | Mutation (exact) | Expected map |
|---|---|---|
| 2.1 | `gh label create gitmaps:crossing --color 18bba7 --description "GitMapsGame crossing"` | 0 crossings; label exists |
| 2.2 | Issue **#2** `E2E crossing A: source terrain`, body `GitMapsGame real-mode E2E fixture.\n\nRequires: none`, label `gitmaps:crossing` | #2 DISCOVERED |
| 2.3 | Issue **#3** `E2E crossing B: requires A`, body `Requires: #2` | #3 LOCKED |
| 2.4 | Issue **#4** `E2E crossing C: requires B`, body `Requires: #3` | #4 LOCKED |
| 2.5 | Issue **#5** `E2E crossing D: missing dependency`, body `Requires: #9999` | #5 INVALID (missing) |
| 2.6 | Issue **#6** `E2E crossing E: cycle with F`, body `Requires: #7`; issue **#7** `E2E crossing F: cycle with E`, body `Requires: #6` | #6, #7 INVALID (cycle) |
| 2.7 | Snapshot twice, with no mutation in between | identical `snapshot.json` and `map.json` sha256 |
| 2.8 | `gh issue edit 3 --add-assignee Tesliana-code` | #3 still **LOCKED**; reason says the claim is ignored |
| 2.9 | `gh issue edit 2 --add-assignee Tesliana-code` | #2 CLAIMED |
| 2.10 | From `main`, create branch `e2e/crossing-a` adding `docs/e2e/crossing-a.md` (one paragraph: "E2E evidence file for crossing #2"). Open PR **#8** `E2E: prove crossing A`, body `Closes #2\n\nGitMapsGame real-mode E2E fixture.` | #2 CLAIMED; PR #8 listed; CI `absent` or `pending` (see note) |
| 2.11 | From `main`, create branch `e2e/ci-failure` adding `tests/test_e2e_failure.py` containing one failing assertion. Open PR **#9** `E2E: failing CI evidence (do not merge)`, body `Closes #4` | #4 LOCKED (claim ignored); PR #9 CI `failure` once its check exists |
| 2.12 | **Merge PR #8 into `main`** (squash) | #2 PROVEN (closed by merged PR #8), #3 CLAIMED (assigned), #4 LOCKED. Review `absent`: the author cannot approve their own PR, so 0 approvals is the honest result. |
| 2.13 | `gh issue close 4 --reason "not planned"`; close PR #9 without merging; delete branch `e2e/ci-failure` | #4 CLOSED_UNPROVEN (not planned) |

Note on 2.10 and 2.11: `main` has no workflows until PR #1 merges. PRs based on `main` therefore run no CI, and the map honestly shows **absent**. To observe real `success` and `failure` evidence, run Phase 3 before steps 2.10–2.13.

After each step, cross-check:

- Every login in `map.json` appears in `gh api repos/Tesliana-code/GitMapsGame/issues/N` or the matching `/pulls/N/reviews`.
- Every SHA resolves through `gh api repos/Tesliana-code/GitMapsGame/commits/SHA`.
- Every CI result matches `.../commits/SHA/check-runs`.

## Phase 3: Activate the published map (GO 3)

| # | Mutation | Command |
|---|---|---|
| 3.1 | Cloudflare Pages project, API token, `cloudflare-pages` environment, secrets and variable | [RELEASE.md](RELEASE.md) §3 |
| 3.2 | Founder review, then merge the release PR into `main` | via the GitHub UI or `gh pr merge` |
| 3.3 | First publish | `gh workflow run gitmaps.yml --ref main` |
| 3.4 | Verify | [RELEASE.md](RELEASE.md) §4: `sha256sum -c` on the live files at `https://gitmapsgame.pages.dev/`, plus a local re-render that matches `map.json` and `index.html` |

Expected:

- The `build` job runs with read-only permissions. It compiles the CLI with `cargo build --release --locked` (about 1 minute) in a step that has no token.
- `cargo --version` in the log shows the runner's preinstalled toolchain.
- The `publish` job runs in the `cloudflare-pages` environment, with no GitHub token scopes, only on `main`.
- Changing an issue (for example assigning one) triggers a rebuild and re-publish, and the change is visible on the page. **This is the gate for any claim of automatic updates.**

## Phase 4: Release and third-party install (GO 4)

| # | Mutation | Command |
|---|---|---|
| 4.0 | Release commit on `main`: remove the pre-release notices in `docs/INSTALL.md`, `README.md` and `examples/workflows/gitmaps.yml`. `PreRelease` in `tests/test_gitmaps_site.py` is inverted to require their absence. | founder-reviewed PR |
| 4.1 | Tag the release commit on `main` | `git tag -a v1 -m "GitMapsGame real mode v1" && git push origin v1` |
| 4.2 | Create a fresh, empty, public throwaway repository under the founder's account | `gh repo create Tesliana-code/gitmaps-install-test --public --add-readme` |
| 4.3 | Follow [INSTALL.md](INSTALL.md) **verbatim** in that repository, then play section 6 end to end | each documented map state is observed on its Pages URL |
| 4.4 | Fix any deviation in INSTALL.md before the Medium article quotes it | |

## Permissions used

| Actor | Scope |
|---|---|
| Founder `gh` login | issues and PRs (Phases 2, 4); repository admin for the environment, secrets and variable (3.1); push (1.2, 4.1) |
| Founder Cloudflare account | create the Pages project and API token (3.1) |
| Workflow `build` job | `contents, issues, pull-requests, checks, statuses: read` |
| Workflow `publish` job | no GitHub scopes; Cloudflare API token (Account · Cloudflare Pages · Edit) from the `cloudflare-pages` environment |
| Trigger workflow | none |

No personal access tokens or TriageAI systems are involved. The only secrets are the two Cloudflare environment secrets, which only the `publish` job can read. `triageai-maps` is out of scope.

## Clean-up options (founder decision)

- Keep the E2E issues as a live public demonstration, or close them as *not planned* once the evidence is recorded.
- Delete the `e2e/*` branches after merge or close.
- Delete `Tesliana-code/gitmaps-install-test` after Phase 4: `gh repo delete Tesliana-code/gitmaps-install-test`.
