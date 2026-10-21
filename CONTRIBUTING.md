# Contributing

Welcome to GitMapsGame! This is a public educational project:

- **Synthetic mode** uses fictional terrain.
- **Real mode** reads public GitHub data and publishes only a read-only map.
- There are zero production integrations.

Good contributions include:

- accessibility improvements;
- diagram corrections;
- additional synthetic failure scenarios;
- offline test coverage;
- clearer explanations of independent release authorization.

Before contributing:

1. Open an Issue describing the crossing you want to improve.
2. State what files you expect to touch and what evidence will prove success.
3. Submit a focused PR with test output and screenshots when useful.
4. Keep all synthetic sample data entirely fictional.
5. Real-mode fixtures:
   - must be unmodified, read-only GitHub API recordings of public repositories;
   - are made with `cargo run --release -- snapshot --repo OWNER/REPO --record FILE`;
   - must use `--redact-bodies` for third-party repositories;
   - the files in `tests/fixtures/recorded/` are frozen.
6. After editing the script in `index.html`, run `cargo run -- check-template --fix`. The page's Content-Security-Policy pins the script's sha256.
7. Never submit credentials, private company code, client data, real deployment URLs or proprietary fixtures.

## Checks

Run these before opening a PR. CI runs the same commands:

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
python3 -m unittest discover -s tests
```

The real-mode CLI is the Rust crate in `src/`. The Python package in `gitmaps/` is its **reference oracle**: Rust output must match it byte for byte.

If you change behaviour:

1. Change the reference first.
2. Regenerate the parity vectors with `python3 scripts/generate_parity_vectors.py` (requires Python 3.12, the oracle's version).
3. Make `cargo test` pass.

The synthetic simulator (`scripts/mock_pipeline.py`) is separate and stays Python.

Maintainers review contributions. Green CI does not imply automatic approval or any deployment authority:

- `synthetic-validation.yml` and `rust.yml` only run tests.
- The real-mode workflows only publish the map.
