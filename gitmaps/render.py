"""Render a derived map into a static, self-contained site directory."""
import base64
import hashlib
import os
import re
from pathlib import Path

from . import canonical_bytes, sha256_hex

TEMPLATE = Path(__file__).resolve().parents[1] / "index.html"
DATA_SLOT = '<script type="application/json" id="gitmaps-data">null</script>'
_MAIN_SCRIPT = re.compile(r"<script>(.*?)</script>", re.DOTALL)
_CSP_HASH = re.compile(r"script-src 'sha256-([A-Za-z0-9+/=]+)'")


def script_hash(html):
    scripts = _MAIN_SCRIPT.findall(html)
    if len(scripts) != 1:
        raise ValueError("template must contain exactly one executable inline script")
    return base64.b64encode(hashlib.sha256(scripts[0].encode("utf-8")).digest()).decode("ascii")


def check_template(html):
    """The Content-Security-Policy must allow exactly the shipped script and nothing else."""
    declared = _CSP_HASH.findall(html)
    if declared != [script_hash(html)]:
        raise ValueError("template CSP script hash is stale; expected sha256-%s" % script_hash(html))
    if html.count(DATA_SLOT) != 1:
        raise ValueError("template must contain exactly one empty data slot")


def refresh_csp(html):
    """Maintainer helper: rewrite the CSP so it allows the template's current script."""
    return _CSP_HASH.sub(lambda _: "script-src 'sha256-%s'" % script_hash(html), html, count=1)


def inline_json(obj):
    """Map bytes made safe inside <script>: no '<', '>' or '&' can close or confuse the element."""
    text = canonical_bytes(obj).decode("ascii")
    return text.replace("<", "\\u003c").replace(">", "\\u003e").replace("&", "\\u0026")


def render_html(map_obj, template=None):
    html = template if template is not None else TEMPLATE.read_text(encoding="utf-8")
    check_template(html)
    if map_obj.get("mode") != "real":
        raise ValueError("only real-mode maps are rendered; synthetic mode is the unmodified template")
    slot = '<script type="application/json" id="gitmaps-data">%s</script>' % inline_json(map_obj)
    return html.replace(DATA_SLOT, slot)


def write_site(out_dir, snapshot_obj, map_obj):
    """Write index.html, map.json, snapshot.json and SHA256SUMS. Returns {name: sha256}."""
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    files = {
        "index.html": render_html(map_obj).encode("utf-8"),
        "map.json": canonical_bytes(map_obj),
        "snapshot.json": canonical_bytes(snapshot_obj),
    }
    digests = {name: sha256_hex(data) for name, data in files.items()}
    files["SHA256SUMS"] = "".join("%s  %s\n" % (digests[n], n) for n in sorted(digests)).encode("ascii")
    for name, data in files.items():
        tmp = out / (name + ".tmp")
        tmp.write_bytes(data)
        os.replace(tmp, out / name)
    return digests
