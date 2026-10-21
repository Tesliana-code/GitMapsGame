"""GitMapsGame real mode: derive a dependency map from a GitHub repository snapshot."""
import hashlib
import json

SNAPSHOT_SCHEMA = "gitmaps.snapshot/v1"
MAP_SCHEMA = "gitmaps.map/v1"
DEFAULT_LABEL = "gitmaps:crossing"
MAX_CROSSINGS = 200


def canonical_bytes(obj):
    """Stable serialisation: identical input objects always yield identical bytes."""
    return (json.dumps(obj, sort_keys=True, ensure_ascii=True, indent=2, separators=(",", ": ")) + "\n").encode("ascii")


def sha256_hex(data):
    return hashlib.sha256(data).hexdigest()
