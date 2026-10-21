"""Rendering, publication safeguards and workflow hardening checks (offline)."""
import contextlib
import copy
import hashlib
import io
import json
import re
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from gitmaps import canonical_bytes, sha256_hex  # noqa: E402
from gitmaps.__main__ import main  # noqa: E402
from gitmaps.derive import derive  # noqa: E402
from gitmaps.render import DATA_SLOT, TEMPLATE, check_template, inline_json, render_html, write_site  # noqa: E402

RECORDED = ROOT / "tests" / "fixtures" / "recorded"
# sha256 of the two approved CSS lines from PR #1 (commit 6b91e9f, index.html lines 5-6).
APPROVED_CSS_SHA256 = "152a06d9b2c245883604645d548fbfce8ae8d7a79f8abd7e72ff14d4bf4bb0ec"


def recorded_snapshot(name="cli-cli"):
    return json.loads((RECORDED / ("%s.snapshot.json" % name)).read_text())


def quiet(argv):
    with contextlib.redirect_stderr(io.StringIO()):
        return main(argv)


class Template(unittest.TestCase):
    def setUp(self):
        self.html = TEMPLATE.read_text(encoding="utf-8")

    def test_csp_matches_script_and_blocks_network(self):
        check_template(self.html)
        self.assertIn("default-src 'none'", self.html)
        self.assertNotIn("connect-src", self.html)
        self.assertNotIn("unsafe-eval", self.html)

    def test_stale_csp_detected(self):
        with self.assertRaises(ValueError):
            check_template(self.html.replace("World reset.", "World reset!"))

    def test_approved_css_preserved(self):
        lines = self.html.split("\n")
        start = lines.index("<style>") + 1
        self.assertEqual(hashlib.sha256(("\n".join(lines[start:start + 2]) + "\n").encode()).hexdigest(),
                         APPROVED_CSS_SHA256)

    def test_synthetic_mode_visibly_labelled(self):
        self.assertIn(DATA_SLOT, self.html)
        self.assertIn("SYNTHETIC DEMO", self.html)
        self.assertIn("every crossing, contributor, check and approval on this page is fictional", self.html)

    def test_no_external_resources(self):
        for match in re.findall(r'(?:src|href)="([^"]+)"', self.html):
            self.assertFalse(match.startswith(("http:", "https:", "//")), match)


class Rendering(unittest.TestCase):
    def test_inline_json_cannot_break_out(self):
        evil = {"title": "</script><script>alert(1)</script> & <!--"}
        text = inline_json(evil)
        self.assertNotIn("<", text)
        self.assertNotIn(">", text)
        self.assertEqual(json.loads(text), evil)

    def test_render_embeds_exact_map(self):
        m = derive(recorded_snapshot())
        html = render_html(m)
        embedded = re.search(r'<script type="application/json" id="gitmaps-data">(.*?)</script>', html, re.S).group(1)
        self.assertEqual(json.loads(embedded), m)
        check_template(html.replace(embedded, "null"))  # executable script untouched

    def test_synthetic_map_not_renderable_as_real(self):
        with self.assertRaises(ValueError):
            render_html({"mode": "synthetic"})

    def test_site_is_deterministic_and_checksummed(self):
        snap = recorded_snapshot()
        with tempfile.TemporaryDirectory() as a, tempfile.TemporaryDirectory() as b:
            write_site(a, snap, derive(snap))
            write_site(b, snap, derive(snap))
            for name in ("index.html", "map.json", "snapshot.json", "SHA256SUMS"):
                self.assertEqual((Path(a) / name).read_bytes(), (Path(b) / name).read_bytes(), name)
            self.assertEqual((Path(a) / "map.json").read_bytes(), (RECORDED / "cli-cli.map.json").read_bytes())
            for line in (Path(a) / "SHA256SUMS").read_text().splitlines():
                digest, name = line.split("  ")
                self.assertEqual(sha256_hex((Path(a) / name).read_bytes()), digest)
            self.assertEqual(sorted(p.name for p in Path(a).iterdir()),
                             ["SHA256SUMS", "index.html", "map.json", "snapshot.json"])


class PrivatePublication(unittest.TestCase):
    def private_snapshot_file(self, directory, visibility="private"):
        snap = copy.deepcopy(recorded_snapshot())  # constructed: recorded public data relabelled non-public
        snap["repository"]["is_private"] = visibility == "private"
        snap["repository"]["visibility"] = visibility
        path = Path(directory) / "snapshot.json"
        path.write_bytes(canonical_bytes(snap))
        return path

    def test_render_refuses_private_and_writes_nothing(self):
        for visibility in ("private", "internal"):
            with tempfile.TemporaryDirectory() as tmp:
                out = Path(tmp) / "site"
                code = quiet(["render", str(self.private_snapshot_file(tmp, visibility)), "--out", str(out)])
                self.assertEqual(code, 2, visibility)
                self.assertFalse(out.exists(), visibility)

    def test_explicit_override_renders(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "site"
            self.assertEqual(quiet(["render", str(self.private_snapshot_file(tmp)), "--out", str(out),
                                    "--allow-private"]), 0)
            self.assertTrue((out / "index.html").exists())

    def test_public_renders_by_default(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(quiet(["render", str(RECORDED / "gitmapsgame.snapshot.json"), "--out", tmp]), 0)


class WorkflowHardening(unittest.TestCase):
    WORKFLOWS = [ROOT / ".github/workflows/gitmaps.yml", ROOT / ".github/workflows/gitmaps-trigger.yml",
                 ROOT / "examples/workflows/gitmaps.yml", ROOT / "examples/workflows/gitmaps-trigger.yml"]

    def test_no_privileged_pr_triggers_and_default_deny(self):
        for path in self.WORKFLOWS:
            text = path.read_text()
            self.assertNotIn("pull_request_target", text, path)
            self.assertIn("\npermissions: {}\n", text, path)
            self.assertNotIn("secrets.", text, path)
            self.assertNotIn("write-all", text, path)

    def test_external_actions_pinned_to_commit(self):
        for path in sorted((ROOT / ".github/workflows").glob("*.yml")) + self.WORKFLOWS[2:] + [ROOT / "action.yml"]:
            for ref in re.findall(r"^\s*(?:-\s+)?uses:\s*(\S+)", path.read_text(), re.M):
                if ref == "./" or ref == "Tesliana-code/GitMapsGame@v1":
                    continue
                self.assertRegex(ref, r"^[\w.-]+/[\w.-]+@[0-9a-f]{40}$", "%s in %s" % (ref, path))

    def test_trigger_workflow_is_inert(self):
        for path in self.WORKFLOWS[1::2]:
            text = path.read_text()
            self.assertNotIn("uses:", text)
            self.assertNotIn("${{", text)

    def test_only_deploy_job_can_write_pages(self):
        for path in (self.WORKFLOWS[0], self.WORKFLOWS[2]):
            build, deploy = path.read_text().split("\n  deploy:\n")
            self.assertNotIn("write", build.split("\njobs:\n")[1])
            self.assertIn("pages: write", deploy)
            self.assertIn("name: github-pages", deploy)

    def test_action_inputs_never_expanded_in_shell(self):
        run_block = (ROOT / "action.yml").read_text().split("run: |", 1)[1]
        self.assertNotIn("${{", run_block)



class PreRelease(unittest.TestCase):
    """Until the v1 tag exists, every instruction that depends on it must say so."""

    def test_v1_dependent_instructions_are_marked(self):
        for name, marker in (("docs/INSTALL.md", "Pre-release: `v1` is not published yet"),
                             ("README.md", "**Pre-release:**"),
                             ("examples/workflows/gitmaps.yml", "PRE-RELEASE: the v1 tag does not exist yet")):
            self.assertIn(marker, (ROOT / name).read_text(), name)


if __name__ == "__main__":
    unittest.main()
