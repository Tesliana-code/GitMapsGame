# Contributing

Welcome to GitMapsGame! This is a public educational project. Synthetic mode uses fictional terrain. Real mode reads public GitHub data and publishes only a read-only map. There are zero production integrations.

Good contributions include accessibility improvements, diagram corrections, additional synthetic failure scenarios, offline test coverage, and clearer explanations of independent release authorization.

Before contributing:
1. Open an Issue describing the crossing you want to improve.
2. State what files you expect to touch and what evidence will prove success.
3. Submit a focused PR with test output and screenshots when useful.
4. Keep all synthetic sample data entirely fictional. Real-mode fixtures must be unmodified, read-only GitHub API recordings of public repositories (`python3 -m gitmaps snapshot --record`), with issue bodies redacted for third-party repositories.
5. After editing the script in `index.html`, run `python3 -m gitmaps check-template --fix`.
6. Never submit credentials, private company code, client data, real deployment URLs or proprietary fixtures.

Maintainers review contributions; green CI does not imply automatic approval or any deployment authority. `synthetic-validation.yml` only runs tests. The real-mode workflows only publish the map.
