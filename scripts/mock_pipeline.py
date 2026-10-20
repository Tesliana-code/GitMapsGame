#!/usr/bin/env python3
"""A deliberately inert release-policy simulation: no subprocess, network or secrets."""
import argparse
import json

SCENARIOS = {
    "approved": {"ci": True, "approved": True, "same_sha": True, "expected": "MOCK_RELEASE_VERIFIED"},
    "ci-failure": {"ci": False, "approved": True, "same_sha": True, "expected": "DENIED_CI"},
    "missing-approval": {"ci": True, "approved": False, "same_sha": True, "expected": "DENIED_APPROVAL"},
    "stale-approval": {"ci": True, "approved": True, "same_sha": False, "expected": "DENIED_SHA"},
}
def evaluate(ci, approved, same_sha):
    if not ci:
        return "DENIED_CI"
    if not approved:
        return "DENIED_APPROVAL"
    if not same_sha:
        return "DENIED_SHA"
    return "MOCK_RELEASE_VERIFIED"

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scenario", choices=SCENARIOS, default="approved")
    args = parser.parse_args()
    item = SCENARIOS[args.scenario]
    result = evaluate(item["ci"], item["approved"], item["same_sha"])
    print(json.dumps({"simulation": True, "scenario": args.scenario,
                      "checks": {k: item[k] for k in ("ci","approved","same_sha")},
                      "result": result, "real_deployment": False}, indent=2))
    assert result == item["expected"]

if __name__ == "__main__":
    main()
