# Public demonstration and intellectual-property boundaries

GitMapsGame has two strictly separated modes:

- **Synthetic mode** (`index.html` opened directly, `scripts/mock_pipeline.py`) is a **standalone fictional environment**, created to illustrate reusable software-engineering ideas. It is labelled *SYNTHETIC DEMO* on screen.
- **Real mode** (`gitmaps/`, `action.yml`, `.github/workflows/gitmaps*.yml`) reads a public repository through the read-only GitHub API and publishes a derived map. It is labelled *REAL DATA* and never shows invented activity.

## Not copied from private TriageAI

- No internal TriageAI code, proprietary architecture, schemas, contract fixtures, prompts or datasets.
- No private credentials, customer identities, account information, server addresses, webhook URLs or deployment keys.
- No real contributor records or operational incident evidence in synthetic mode. Real mode shows only GitHub logins returned by the API for the mapped repository.
- No integration with TriageAI production hosting or private repositories. Real mode publishes nothing from non-public repositories unless explicitly overridden, and never touches `triageai-maps`.
- Recorded test fixtures under `tests/fixtures/recorded/` are unmodified read-only GitHub API responses from public repositories. The `cli/cli` recording has issue bodies blanked.

The principles of Git branching, code review, testing, dependency graphs and approval gates are general concepts. Examples are authored from scratch.

## What this proves — and does not prove

**Synthetic mode proves:** a reader can explore a dependency-based game and run deterministic local tests showing how a mock release policy permits or refuses synthetic inputs.

**Real mode proves:** the published map is a reproducible function of a GitHub API snapshot that anyone can download and re-derive. It does **not** prove release authority. PROVEN, CI and approval are displayed facts, not permissions.

**Does not prove:** a private deployment pipeline is healthy; a real signature is validated; authorization cannot be forged; audit history is immutable; contributions were merged or deployed; or a particular customer workflow is production-ready.

For the Medium article, label every synthetic screenshot and snippet **Synthetic GitMapsGame prototype**, and every real-mode screenshot with its repository and snapshot sha256. Real TriageAI engineering results, if mentioned, should be verified separately and described without disclosing protected details.
