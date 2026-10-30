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
# sha256 of the founder-approved and frozen world-map <style> block (approved 2026-10-21,
# promoted from prototypes/world-map). Changing the design requires founder approval.
# 2026-10-30, founder-authorized PLAYERS leaderboard: the only style change is five added lines after
# `.login input{...}`, all using existing tokens (previous hash dee7ddce449ab5a1...):
#   .board{width:100%;border-collapse:collapse;margin:0 0 10px}
#   .board th,.board td{padding:6px;text-align:left;vertical-align:top;border-bottom:2px solid var(--navy)}
#   .board th{color:var(--yellow);font-size:13px;letter-spacing:.06em;border-bottom-color:var(--teal)}
#   .board .n{text-align:right;white-space:nowrap;font-weight:700}.board .pts{color:var(--yellow)}
#   .board .who b{display:block;color:var(--cyan);overflow-wrap:anywhere}.board .who span{font-size:13px}
# 2026-10-30, founder-requested desktop layout fix (map fills its column instead of leaving a void under it;
# previous hash 25d05911dd3d4991...). Exact changes:
#   main             max-width:1240px -> 1600px
#   .layout          align-items:start removed (columns stretch to equal height)
#   added            .layout>.window{display:flex;flex-direction:column}
#   .viewport        max-height:720px;min-height:260px -> flex:1 1 0;min-height:702px (702 = 5 rows of 132px + 42)
#   @media(max-width:900px) added .viewport{flex:none;min-height:260px;max-height:720px} (single column as before)
#   .grid            added margin:0 auto (small worlds centred)
APPROVED_STYLE_SHA256 = "a27a4a3886dad7577a84272a6a3f7bb4356fb3d3a9784b28681015b85f431996"
# Founder-requested story link on the homepage (Quest #3, 2026-10-29).
STORY_URL = "https://medium.com/@ivavrtaric/gamifying-a-private-git-repository-turns-contributors-into-players-2e99f3610d1a"


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
            check_template(self.html.replace("TUTORIAL STARTED.", "TUTORIAL STARTED!"))

    def test_approved_css_preserved(self):
        style = self.html[self.html.index("<style>") + len("<style>"):self.html.index("</style>")]
        self.assertEqual(hashlib.sha256(style.encode("utf-8")).hexdigest(), APPROVED_STYLE_SHA256)

    def test_synthetic_mode_visibly_labelled(self):
        self.assertIn(DATA_SLOT, self.html)
        # Opened without data, the page is the fictional tutorial: chip, status bar and footnote say so.
        self.assertIn('<p class="badge" id="badge">TUTORIAL WORLD · FICTIONAL</p>', self.html)
        self.assertIn('<footer id="foot">TUTORIAL WORLD · FICTIONAL · NO GITHUB CHANGES</footer>', self.html)
        self.assertIn('"WORLD TUTORIAL (FICTIONAL)"', self.html)

    def test_no_external_resources(self):
        # Anchors are navigation, not resources; they are checked separately below.
        for match in re.findall(r'(?:src|href)="([^"]+)"', re.sub(r"<a [^>]*>", "", self.html)):
            self.assertFalse(match.startswith(("http:", "https:", "//")), match)

    def test_only_the_story_link_leaves_the_page(self):
        # The founder-approved READ THE STORY link is the template's only static external anchor.
        anchors = re.findall(r"<a [^>]*>", self.html)
        external = [a for a in anchors if not re.search(r'href="#', a)]
        self.assertEqual(len(external), 1, external)
        self.assertIn('href="%s"' % STORY_URL, external[0])
        self.assertIn('rel="noopener noreferrer"', external[0])

    def test_how_to_play_visible_on_first_visit(self):
        self.assertIn('<details class="window" id="how-to-play" open>', self.html)
        for step in ("① EXPLORE", "② CHOOSE QUEST", "③ BUILD", "④ SUBMIT PR"):
            self.assertIn("<h3>%s</h3>" % step, self.html)
        self.assertIn('href="#play"', self.html)


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
            self.assertNotIn("write-all", text, path)
            if path != self.WORKFLOWS[0]:
                self.assertNotIn("secrets.", text, path)

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

    def test_example_only_deploy_job_can_write_pages(self):
        build, deploy = self.WORKFLOWS[2].read_text().split("\n  deploy:\n")
        self.assertNotIn("write", build.split("\njobs:\n")[1])
        self.assertIn("pages: write", deploy)
        self.assertIn("name: github-pages", deploy)

    def test_cloudflare_publish_is_isolated(self):
        text = self.WORKFLOWS[0].read_text()
        build, publish = text.split("\n  publish:\n")
        build_job = build.split("\njobs:\n")[1]
        self.assertNotIn("write", build_job)
        self.assertNotIn("secrets.", build_job)  # the build (which runs the API queries) never sees Cloudflare credentials
        self.assertNotIn("pages: write", text)
        self.assertIn("    permissions: {}\n", publish)  # no GitHub token scopes in the publishing job
        self.assertIn("if: github.ref == 'refs/heads/main'", publish)
        self.assertIn("name: cloudflare-pages", publish)
        self.assertIn("sha256sum -c SHA256SUMS", publish)
        self.assertRegex(publish, r"npx --yes wrangler@\d+\.\d+\.\d+ pages deploy _site")
        self.assertEqual(sorted(set(re.findall(r"secrets\.(\w+)", text))), ["CLOUDFLARE_ACCOUNT_ID", "CLOUDFLARE_API_TOKEN"])
        for line in publish.split("\n"):
            if "secrets." in line:
                self.assertRegex(line.strip(), r"^CLOUDFLARE_\w+: \$\{\{ secrets\.CLOUDFLARE_\w+ \}\}$")  # env only, never inline in run

    def test_action_inputs_never_expanded_in_shell(self):
        for run_block in (ROOT / "action.yml").read_text().split("run: |")[1:]:
            self.assertNotIn("${{", run_block.split("\n    - name:")[0])

    def test_action_compiles_rust_without_credentials(self):
        text = (ROOT / "action.yml").read_text()
        self.assertNotIn("python", text.lower())
        compile_step, build_step = text.split("\n    - name: Build map from the GitHub API\n")
        compile_step = compile_step.split("id: compile", 1)[1]
        self.assertNotIn("env:", compile_step)
        self.assertNotIn("inputs.token", compile_step)
        for needle in ('cd "$GITHUB_ACTION_PATH"', "cargo build --release --locked", "unset RUSTFLAGS",
                       "RUSTC_WRAPPER", "GH_TOKEN GITHUB_TOKEN", 'CARGO_TARGET_DIR="$RUNNER_TEMP'):
            self.assertIn(needle, compile_step)
        self.assertIn("GH_TOKEN: ${{ inputs.token }}", build_step)
        self.assertIn('"$GITMAPS_BIN" "${args[@]}"', build_step)

    def test_rust_ci_is_read_only_and_runs_required_checks(self):
        text = (ROOT / ".github/workflows/rust.yml").read_text()
        self.assertIn("\npermissions: {}\n", text)
        self.assertNotIn("pull_request_target", text)
        self.assertNotIn("secrets.", text)
        self.assertNotIn("write", text)
        for check in ("cargo fmt --check", "cargo clippy --locked --all-targets -- -D warnings",
                      "cargo test --locked", "cargo build --release --locked", "git diff --exit-code"):
            self.assertIn(check, text)


class PreRelease(unittest.TestCase):
    """Until the v1 tag exists, every instruction that depends on it must say so."""

    def test_v1_dependent_instructions_are_marked(self):
        for name, marker in (("docs/INSTALL.md", "Pre-release: `v1` is not published yet"),
                             ("README.md", "**Pre-release:**"),
                             ("examples/workflows/gitmaps.yml", "PRE-RELEASE: the v1 tag does not exist yet")):
            self.assertIn(marker, (ROOT / name).read_text(), name)


if __name__ == "__main__":
    unittest.main()
