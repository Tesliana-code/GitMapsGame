"""Command line: snapshot, replay, derive, render (snapshot -> site), build (GitHub -> site), check-template."""
import argparse
import json
import os
import sys

from . import DEFAULT_LABEL, canonical_bytes, sha256_hex
from .derive import check_publishable, derive
from .fetch import snapshot
from .github import GitHubError, LiveTransport, RecordingTransport, ReplayTransport
from .render import TEMPLATE, check_template, refresh_csp, write_site


def redact_bodies(name, response):
    """For third-party fixtures: drop issue bodies (and so any Requires lines) before recording."""
    def walk(node):
        if isinstance(node, dict):
            return {k: ("" if k == "body" else walk(v)) for k, v in node.items()}
        if isinstance(node, list):
            return [walk(v) for v in node]
        return node
    return walk(response)


def _write(path, obj):
    data = canonical_bytes(obj)
    if path == "-":
        sys.stdout.buffer.write(data)
    else:
        with open(path, "wb") as handle:
            handle.write(data)
    return sha256_hex(data)


def _read(path):
    with open(path, "rb") as handle:
        return json.loads(handle.read().decode("utf-8"))


def _site(out_dir, snap_obj, allow_private):
    check_publishable(snap_obj, allow_private=allow_private)  # before anything is written
    digests = write_site(out_dir, snap_obj, derive(snap_obj))
    github_output = os.environ.get("GITHUB_OUTPUT")
    if github_output:
        with open(github_output, "a", encoding="utf-8") as handle:
            handle.write("snapshot-sha256=%s\nmap-sha256=%s\n" % (digests["snapshot.json"], digests["map.json"]))
    for name in sorted(digests):
        print("%s  %s" % (digests[name], name), file=sys.stderr)
    return digests["map.json"]


def main(argv=None):
    parser = argparse.ArgumentParser(prog="python3 -m gitmaps", description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    snap = sub.add_parser("snapshot", help="query GitHub (read-only) and write a snapshot")
    snap.add_argument("--repo", required=True, help="owner/name")
    snap.add_argument("--label", default=DEFAULT_LABEL)
    snap.add_argument("--issue", type=int, action="append", help="select issues by number instead of label")
    snap.add_argument("--record", help="also write raw API exchanges for offline replay")
    snap.add_argument("--redact-bodies", action="store_true", help="blank issue bodies in the recording")
    snap.add_argument("-o", "--output", default="-")

    rep = sub.add_parser("replay", help="rebuild a snapshot from a recording, without network")
    rep.add_argument("recording")
    rep.add_argument("-o", "--output", default="-")

    der = sub.add_parser("derive", help="derive map.json from a snapshot")
    der.add_argument("snapshot")
    der.add_argument("-o", "--output", default="-")
    der.add_argument("--for-publication", action="store_true", help="refuse non-public repositories")
    der.add_argument("--allow-private", action="store_true")

    ren = sub.add_parser("render", help="derive and render a site from a snapshot file")
    ren.add_argument("snapshot")
    ren.add_argument("--out", required=True)
    ren.add_argument("--allow-private", action="store_true")

    bld = sub.add_parser("build", help="snapshot GitHub, derive and render a site")
    bld.add_argument("--repo", required=True, help="owner/name")
    bld.add_argument("--label", default=DEFAULT_LABEL)
    bld.add_argument("--out", required=True)
    bld.add_argument("--allow-private", action="store_true",
                     help="render a non-public repository (never publish the result)")

    chk = sub.add_parser("check-template", help="verify the index.html CSP hash and data slot")
    chk.add_argument("--fix", action="store_true", help="rewrite the CSP hash after editing the script")

    args = parser.parse_args(argv)
    try:
        if args.command == "snapshot":
            transport = LiveTransport()
            if args.record:
                transport = RecordingTransport(transport, redact_bodies if args.redact_bodies else None)
            result = snapshot(transport, args.repo, label=args.label, numbers=args.issue)
            if args.record:
                _write(args.record, {"repo": args.repo, "label": args.label, "numbers": args.issue,
                                     "body_redacted": bool(args.redact_bodies),
                                     "exchanges": transport.exchanges})
            digest = _write(args.output, result)
        elif args.command == "replay":
            recording = _read(args.recording)
            transport = ReplayTransport(recording["exchanges"])
            result = snapshot(transport, recording["repo"], label=recording["label"], numbers=recording["numbers"])
            transport.assert_consumed()
            digest = _write(args.output, result)
        elif args.command == "render":
            digest = _site(args.out, _read(args.snapshot), args.allow_private)
        elif args.command == "build":
            snap_obj = snapshot(LiveTransport(), args.repo, label=args.label)
            digest = _site(args.out, snap_obj, args.allow_private)
        elif args.command == "check-template":
            html = TEMPLATE.read_text(encoding="utf-8")
            if args.fix:
                html = refresh_csp(html)
                TEMPLATE.write_text(html, encoding="utf-8")
            check_template(html)
            digest = "template ok"
        else:
            snap_obj = _read(args.snapshot)
            if args.for_publication:
                check_publishable(snap_obj, allow_private=args.allow_private)
            digest = _write(args.output, derive(snap_obj))
    except (GitHubError, PermissionError, ValueError) as exc:
        print("gitmaps: error: %s" % exc, file=sys.stderr)
        return 2
    print("map sha256 %s" % digest if args.command in ("render", "build") else
          digest if args.command == "check-template" else "sha256 %s" % digest, file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
