# Native Rust migration: engineering notes

Status: **local integration** (2026-10-21).

- The Rust CLI covers every reference command: `snapshot` (with `--record` and `--redact-bodies`), `replay`, `derive`, `render`, `build` and `check-template`.
- `action.yml` now compiles and runs the Rust CLI.
- `.github/workflows/rust.yml` runs fmt, clippy, tests, the release build, MSRV and oracle-vector sync.
- Everything is **locally tested only**. GitHub-hosted runs, Pages publication, third-party installation and live gameplay are pending. V1 is not released.

## Baseline

- The Python reference is preserved at commit `c31a97e` on `preserve/real-mode-python-v1`, plus an off-repo archive in `/mnt/e/GITGAME/_preserve/`.
- `gitmaps/` (Python) stays in place as the **reference oracle** until parity sign-off. It is then moved to `reference/python/`.
- The six files in `tests/fixtures/recorded/` are frozen. `tests/parity.rs::frozen_fixtures_unchanged` locks their sha256 values.

## Reader experience

```bash
GH_TOKEN="$(gh auth token)" cargo run --release -- build --repo OWNER/REPO --out site
```

Requirements: the Rust toolchain (1.85 or newer, verified with 1.85.0 and 1.92.0) and nothing else. TLS is rustls on ring with bundled webpki roots: no OpenSSL, no cmake, no system CA lookup. The only system libraries the binary links are `libc` and `libgcc_s`. A clean `--release --locked` build takes about 43 s and produces a 3.1 MB binary (measured on the WSL development host). `index.html` is compiled into the binary.

## Parity contract

The Rust output must be byte-identical to the reference for every input:

| Evidence | Source of expected bytes |
|---|---|
| Recorded real GitHub replays: snapshot, map, `index.html`, `SHA256SUMS` | frozen reference outputs and reference render hashes |
| Canonical JSON (`ensure_ascii`, indent 2, key order, surrogate pairs) | `tests/fixtures/parity/canonical.json` |
| Python `repr()` in INVALID reasons | `repr.json`; `src/pyrepr_table.rs` generated from Python 3.12 (Unicode 15.0) |
| `Requires:` grammar | `requires.json` |
| Derivation: 120 seeded random snapshots, plus rejected inputs | `derive.json` |
| Fetch normalisation: 13 constructed GraphQL scenarios | `fetch.json` |
| Render: `index.html` for 95 public cases; publication refusal for 25 non-public cases | `render.json` |
| `check-template` and `--fix`: 11 template variants | `template.json` |
| GraphQL query text | `src/queries.rs`, generated from `gitmaps/fetch.py` |

`python3 scripts/generate_parity_vectors.py` regenerates every vector from the reference. It is deterministic and its output is rustfmt-stable.

**Live differential (read-only, 2026-10-21).** Python and Rust each ran `snapshot --record` back to back on five public targets:

- `Tesliana-code/GitMapsGame`;
- 9 selected `cli/cli` issues;
- the `cli/cli` labels `gh-org` and `dependencies`;
- the `actions/checkout` label `documentation`.

Across those targets: 31 issues, 23 PRs and 67 reviews.

- Snapshots, recordings, each language replaying the other's recording, and rendered sites were byte-identical.
- Live `build` produced identical sites.
- The live GitMapsGame and `cli/cli` snapshots still equal the frozen fixture hashes.
- No token appears in any artifact.

**Test strength:** 17 of 17 targeted mutations are detected. They cover derivation, case folding, CI mapping, JSON layout and escaping, `repr`, review ordering, the merge check, base64 padding, CSP scanning, the data slot, `SHA256SUMS` layout, publication refusal, and recording integrity.

## Security notes

- **Transport:** HTTPS only and no redirects, so the Authorization header can only reach `api.github.com`. 30 s timeout, 32 MiB response cap. The token comes from `GH_TOKEN`/`GITHUB_TOKEN`, is sent only in the header, and never appears in errors or output (tested).
- **Recording** stores an untouched copy of each response. The Python reference had a defect here; it is fixed (see below).
- **Publication:** private and internal repositories are refused before any file is written (tested through the CLI).
- **Dependencies:** 33 crates. The RustSec sweep found all 25 advisories for those crates patched or unaffected.
  - `serde_json`, `libc` and `ureq-proto` are pinned to releases at least 14 days old.
  - `rustls 0.23.46`, `rustls-webpki 0.103.16` and `webpki-roots 1.0.10` are newer than 14 days on purpose. They are the patched releases for RUSTSEC-2026-0346 to -0349, -0382 and -0381. Older versions would reintroduce those issues.
- **Action (`action.yml`):**
  - It compiles in a separate step that has no token. It also unsets `GH_TOKEN`/`GITHUB_TOKEN`, `RUSTFLAGS` and `RUSTC_WRAPPER`, so dependency build scripts never see credentials and cannot be redirected.
  - It builds from the action directory, so a checked-out `.cargo/config.toml` doesn't apply. This was tested with a hostile workspace config and environment.
  - The target directory lives in `$RUNNER_TEMP`.
  - Residual risk: cargo also reads `~/.cargo/config.toml` and `CARGO_HOME`, which earlier steps in the *owner's own* workflow could change. That is within the workflow owner's trust boundary.

## Known discrepancies and findings

1. **Reference defect, fixed:** `RecordingTransport` (Python) stored the same response object that `Fetcher.complete_linked_pull_requests` later modifies. Recordings of issues with more than 25 linked PRs could therefore not be replayed. The fix stores deep copies. Regression test: `tests/test_gitmaps_fetch.py::Recording`; it fails on `c31a97e` and passes now. No frozen fixture changed.
2. **Issue numbers above u64 in `Requires:`:** Python accepts them, Rust treats them as unresolved. This is unreachable, since GitHub issue numbers are 32-bit.
3. **`repr()` Unicode version** is pinned to the reference's Unicode 15.0 table. Regenerate if the reference Python version changes.
4. **CLI error wording** is not part of the contract. Exit codes match (2 on failure). One intentional difference: Rust exits 2 for I/O errors where Python would raise a traceback.
5. **`check-template` path:** Python always checks its package's own `index.html`. Rust checks `index.html` in the current directory, or a path argument, because an installed binary has no source tree.

## Remaining before release

- Run `rust.yml` and `action.yml` on GitHub-hosted runners. That needs a push, which only the founder does.
  - GitHub's Ubuntu image ships Rust 1.99 with rustfmt but **not clippy**, so CI installs the pinned 1.92.0 toolchain with clippy.
- Pages publication, the E2E manifest phases, and a third-party install following INSTALL.md verbatim.
- Publication review of the `cli/cli` fixture: public logins and titles. **Open.**
- After sign-off: move `gitmaps/` to `reference/python/`, which is a separate approval.

## AI assistance

The Rust port, the parity-vector generator, the Python fix, the Rust action and CI workflow, the documentation updates, these notes and the verification runs were produced with AI assistance (Claude Code). The founder reviews them before any commit. All Git operations are performed by the founder.
