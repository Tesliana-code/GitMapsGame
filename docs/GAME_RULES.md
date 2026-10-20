# Rules of play

The fictional terrain progresses as **UNKNOWN → DISCOVERED → CLAIMED → PROVEN → UNLOCKED**.

- UNKNOWN: missing crossing not yet understood.
- DISCOVERED: defined crossing with concrete acceptance evidence.
- CLAIMED: a player chooses a scoped working surface and proof contract.
- PROVEN: evidence is accepted; a mock in-browser claim and proof is **only a visualization**.
- UNLOCKED: dependencies become available for future work. It does **not** imply release eligibility.

The playground begins with four **DISCOVERED or LOCKED** fictional crossings. Claim and prove them in dependency order. The next crossing becomes DISCOVERED once its prerequisite is PROVEN.

Players may be humans or AI agents; both submit evidence. Identity, permissions and production authority are **never** inferred from participation. The browser demonstration has no backend and is not an access-control system.

## Win condition

The browser shows all four crossings proven and an independently approved **mock** release. The latter intentionally requires separate checks. Winning doesn't confer production permissions.

## Design principles

- Claim a crossing, not a permanent team or specialty.
- A reviewer judges evidence; an implementation author cannot self-certify a release.
- All history or audit labels in this public prototype are illustrative, not tamper-resistant.
- Real deployments require a separate, reviewed and hardened workflow.
