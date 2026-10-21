# Install GitMapsGame on your repository

GitMapsGame turns GitHub issues into a map of **crossings**. You declare dependencies in plain issue text, work happens in normal pull requests, and a GitHub Actions workflow publishes a read-only map to GitHub Pages. Every state on the map is derived from the GitHub API. Nothing is entered by hand and nothing is simulated.

Time needed: about 10 minutes.

> **Pre-release: `v1` is not published yet.** Until the `v1` release tag exists, these will fail:
>
> - `uses: Tesliana-code/GitMapsGame@v1`
> - the `raw.githubusercontent.com/.../v1/...` downloads
> - `git clone --branch v1`
>
> Check with `gh api repos/Tesliana-code/GitMapsGame/git/ref/tags/v1`; a 404 means it is not released. Until then, only the local preview works, from a clone of the development branch:
>
> ```bash
> git clone --branch feat/synthetic-playground-v1 https://github.com/Tesliana-code/GitMapsGame.git
> cd GitMapsGame && GH_TOKEN="$(gh auth token)" python3 -m gitmaps build --repo OWNER/REPO --out site
> ```
>
> This notice is removed in the release commit that receives the `v1` tag.

## What you need

- A **public** GitHub repository where you can change Settings (admin role). Private repositories are refused by default; see [Private repositories](#private-repositories).
- GitHub Actions enabled for the repository.
- Optional: the [GitHub CLI](https://cli.github.com/) (`gh`). Every step below also has a web UI path.

## 1. Turn on GitHub Pages from Actions

**Web:** *Settings → Pages → Build and deployment → Source:* select **GitHub Actions**.

**CLI:**

```bash
gh api -X POST repos/OWNER/REPO/pages -f build_type=workflow
```

This creates the `github-pages` environment. By default only your default branch may deploy to it. Keep that rule.

## 2. Add the two workflow files

From the root of your repository's default branch:

```bash
mkdir -p .github/workflows
curl -fsSL https://raw.githubusercontent.com/Tesliana-code/GitMapsGame/v1/examples/workflows/gitmaps.yml \
  -o .github/workflows/gitmaps.yml
curl -fsSL https://raw.githubusercontent.com/Tesliana-code/GitMapsGame/v1/examples/workflows/gitmaps-trigger.yml \
  -o .github/workflows/gitmaps-trigger.yml
```

Read both files before you commit them. They are short:

| File | Runs on | Permissions | Does |
|---|---|---|---|
| `gitmaps-trigger.yml` | pull requests and reviews, including from forks | none | prints one line, which signals the map workflow |
| `gitmaps.yml` | issue changes, trigger completion, pushes to the default branch, every 6 h, manual | build: read-only; deploy: `pages: write`, `id-token: write` | queries the GitHub API, renders the map, deploys it to Pages |

Optional: so CI results refresh the map as soon as they finish, add the `name:` of each CI workflow to the `workflow_run.workflows` list in `gitmaps.yml`. Otherwise CI results appear at the next trigger or scheduled run.

If your default branch is not `main`, change `branches: [main]` in `gitmaps.yml`.

For the strongest supply-chain guarantee, replace `@v1` in `uses: Tesliana-code/GitMapsGame@v1` with the full commit SHA of the release tag:

```bash
gh api repos/Tesliana-code/GitMapsGame/commits/v1 --jq .sha
```

Commit and push to your default branch:

```bash
git add .github/workflows/gitmaps.yml .github/workflows/gitmaps-trigger.yml
git commit -m "Add GitMapsGame map"
git push
```

## 3. Create the crossing label

```bash
gh label create gitmaps:crossing --color 18bba7 --description "GitMapsGame crossing"
```

Web: *Issues → Labels → New label*, name `gitmaps:crossing`.

## 4. Create crossings

A crossing is any issue with the `gitmaps:crossing` label. Dependencies go on their own line in the issue body:

```text
Requires: #12, #15
```

- Use `Requires: none` (or no line at all) for starting terrain.
- Only same-repository references of the form `#N` are accepted. Anything else, such as `other/repo#3`, makes the crossing **INVALID**.
- Lines inside code blocks are ignored.

You need an issue's number before you can depend on it, so create crossings in order:

```bash
A=$(gh issue create --label gitmaps:crossing --title "Design the data model" \
      --body "Requires: none" | sed 's#.*/##')
B=$(gh issue create --label gitmaps:crossing --title "Build the API" \
      --body "Requires: #$A" | sed 's#.*/##')
gh issue create --label gitmaps:crossing --title "Ship the UI" --body "Requires: #$B"
```

## 5. Publish and open the map

Labelling issues starts the workflow. You can also run it by hand:

```bash
gh workflow run gitmaps.yml
gh run watch "$(gh run list --workflow gitmaps.yml --limit 1 --json databaseId --jq '.[0].databaseId')"
```

Open `https://OWNER.github.io/REPO/`. Your first crossing shows **DISCOVERED** and the others show **LOCKED**.

## 6. Play with real work

| You do on GitHub | The map shows |
|---|---|
| Assign yourself the discovered crossing: `gh issue edit $A --add-assignee @me` | A becomes **CLAIMED** |
| Assign yourself a locked crossing | It stays **LOCKED**. A claim never bypasses dependencies. |
| Open a pull request whose description contains `Closes #A` | A lists the PR, its CI result and its review state |
| Merge that PR into the default branch | A becomes **PROVEN**, and B becomes **DISCOVERED** |
| Close an issue by hand, or as *not planned* | **CLOSED_UNPROVEN**; its dependents stay locked |

GitHub only auto-closes an issue when a PR with a closing keyword (`Closes`, `Fixes`, `Resolves`) is merged into the **default branch**. A PR merged into any other branch does not prove a crossing.

## Rules

States, in order of precedence:

| State | Condition |
|---|---|
| `INVALID` | Requires a missing issue, an unresolved reference, or itself through a cycle |
| `LOCKED` | Any required crossing is not PROVEN. This overrides both CLAIMED and a merged closure. |
| `PROVEN` | Closed as *completed*, and the close event's closer is a **merged PR in this repository** |
| `CLOSED_UNPROVEN` | Closed any other way: manually, *not planned*, duplicate, by a commit, or by an unmerged or other-repository PR |
| `CLAIMED` | Open, with an assignee or an open linked PR |
| `DISCOVERED` | Open, every requirement PROVEN, nobody on it |

Evidence is shown per pull request, independently of the state:

- **CI** (combined status of the PR's head commit): `success`, `failure`, `pending`, `absent` (no checks reported) or `unknown` (could not be read).
- **Review** (latest decisive review per reviewer): `approved`, `changes_requested`, `absent` or `unknown`. Approvals on the final head commit are listed separately.
- **Release authority** is not tracked. PROVEN, a green CI result and an approval are separate facts, and none of them is permission to deploy.

## Check what was published

Every site contains `map.json`, `snapshot.json` and `SHA256SUMS`. The map is a pure function of the snapshot, so anyone can rebuild it from the published snapshot and compare:

```bash
git clone --depth 1 --branch v1 https://github.com/Tesliana-code/GitMapsGame.git
cd GitMapsGame
curl -fsSLO https://OWNER.github.io/REPO/snapshot.json
curl -fsSLO https://OWNER.github.io/REPO/map.json
python3 -m gitmaps render snapshot.json --out rebuilt
cmp map.json rebuilt/map.json && echo "map reproduced byte for byte"
```

To preview your repository's current state locally without publishing anything (Python 3 standard library only; tested with 3.12):

```bash
GH_TOKEN="$(gh auth token)" python3 -m gitmaps build --repo OWNER/REPO --out site
# then open site/index.html in a browser
```

## Security model

- **GitHub is the source of truth.** Event payloads only trigger a rebuild. Every run re-queries the API with the workflow's read-only `GITHUB_TOKEN`.
- **No pull request code runs with privileges.** No workflow uses `pull_request_target` or checks out PR code. Fork PRs run only the permission-less trigger.
- **Least privilege.** The build job has read-only permissions. Only the deploy job holds `pages: write`. No personal tokens or repository secrets are needed.
- **Untrusted text stays text.** Issue and PR titles are rendered with `textContent`. Only `https://github.com/` links are clickable. A Content-Security-Policy allows exactly one script and no network access.
- Third-party actions are pinned to full commit SHAs.

## Private repositories

A GitHub Pages site is usually **public, even when the repository is private**. The map would publish issue titles, logins and PR data. Because of that, `build` and `render` refuse non-public repositories.

To view a private repository's map locally only:

```bash
GH_TOKEN="$(gh auth token)" python3 -m gitmaps build --repo OWNER/PRIVATE --out site --allow-private
```

Setting `allow-private: "true"` on the action publishes that data. Do it only if your Pages site's access control is restricted and you accept the exposure.

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| Map did not change after a PR or review | Workflows triggered by first-time fork contributors may wait for approval. `workflow_run` only fires for workflows on the default branch. The 6-hourly schedule catches up. You can also run `gh workflow run gitmaps.yml`. |
| `Branch "x" is not allowed to deploy to github-pages` | Run the workflow from the default branch. |
| Page says *the crossing label does not exist* | Create `gitmaps:crossing` (step 3). |
| `refusing to publish private repository` | See [Private repositories](#private-repositories). |
| `N crossings exceed the limit of 200` | V1 maps at most 200 crossings. |
| Crossing shows evidence gaps | The API reported more items than it returned, or a linked PR could not be read. The state is computed fail-closed. |

## Uninstall

Delete the two workflow files. Then set *Settings → Pages* back to **None**, or run `gh api -X DELETE repos/OWNER/REPO/pages`. You can keep the `gitmaps:crossing` label or delete it.
