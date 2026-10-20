# Mock release simulation

Run locally with Python 3; no packages needed.

```bash
python3 scripts/mock_pipeline.py --scenario approved
python3 scripts/mock_pipeline.py --scenario ci-failure
python3 scripts/mock_pipeline.py --scenario missing-approval
python3 scripts/mock_pipeline.py --scenario stale-approval
python3 -m unittest discover -s tests -v
```

| Scenario | Mock CI | Mock approval | Revision matches | Result |
| --- | --- | --- | --- | --- |
| approved | yes | yes | yes | MOCK_RELEASE_VERIFIED |
| ci-failure | no | yes | yes | DENIED_CI |
| missing-approval | yes | no | yes | DENIED_APPROVAL |
| stale-approval | yes | yes | no | DENIED_SHA |

The program does **not** use subprocess, network, CI credentials, GitHub tokens or deployment targets. The “SHA check” is an abstract boolean used only to teach exact-revision binding.

The browser UI separately models prerequisite unlocking. It is purposefully not a trustworthy authorization service.

The GitHub Actions workflow runs tests on pull requests and pushes to main. It contains no production dispatch.
