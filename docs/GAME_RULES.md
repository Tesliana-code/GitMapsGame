# Rules of play

The fictional terrain progresses as **UNKNOWN → DISCOVERED → CLAIMED → PROVEN → UNLOCKED**.

- UNKNOWN: missing crossing not yet understood.
- DISCOVERED: defined crossing with concrete acceptance evidence.
- CLAIMED: a player chooses a scoped working surface and proof contract.
- PROVEN: evidence is accepted; a mock in-browser claim and proof is **only a visualization**.
- UNLOCKED: dependencies become available for future work. It does **not** imply release eligibility.

The playground begins with four **DISCOVERED or LOCKED** fictional crossings. Claim and prove them in dependency order. The next crossing becomes DISCOVERED once its prerequisite is PROVEN.

Players may be humans or AI agents; both submit evidence. Identity, permissions and production authority are **never** inferred from participation. The browser demonstration has no backend and is not an access-control system.

For a first-visit walkthrough (explore, choose a quest, build, submit a PR), see [How to play](../README.md#how-to-play).

## Real mode

On a real repository, the same vocabulary is derived from GitHub data only:

| State | Condition |
|---|---|
| `INVALID` | missing dependency, unresolved reference, or a dependency cycle |
| `LOCKED` | a required crossing is not PROVEN (overrides CLAIMED and a merged closure) |
| `PROVEN` | closed as completed by a merged pull request in the same repository |
| `CLOSED_UNPROVEN` | closed without that evidence |
| `CLAIMED` | open, with an assignee or an open linked pull request |
| `DISCOVERED` | open, all requirements PROVEN, unclaimed |

### PLAYERS leaderboard

Scores are derived only from crossing evidence already in `map.json`; a crossing scores at most once per player.

| Points | Condition | Attributed to |
|---|---|---|
| 10 | crossing is `PROVEN` | author of its merged closing pull request (`closed_by`, same repository) |
| 2 | crossing is `CLAIMED` | each assignee, and each author of an open linked pull request in the same repository |

A PROVEN crossing never also scores as CLAIMED. Other states score nothing. Generic contributor roles (reviewers, authors of closed or unmerged pull requests) never score. Players are ranked by points, then PROVEN count, then login (case-insensitive).

**Scores confer no merge, review, deployment or release authority** and grant no repository permissions. They are a display of public GitHub facts, not an access-control or trust signal.

CI (`success`, `failure`, `pending`, `absent`, `unknown`) and review (`approved`, `changes_requested`, `absent`, `unknown`) are shown as separate facts. Cross-repository closing PRs do not satisfy PROVEN in V1. See [INSTALL.md](INSTALL.md).

## Win condition (synthetic mode)

The browser shows all four crossings proven and an independently approved **mock** release. The latter intentionally requires separate checks. Winning doesn't confer production permissions.

## Design principles

- Claim a crossing, not a permanent team or specialty.
- A reviewer judges evidence; an implementation author cannot self-certify a release.
- All history or audit labels in this public prototype are illustrative, not tamper-resistant.
- Real deployments require a separate, reviewed and hardened workflow.
