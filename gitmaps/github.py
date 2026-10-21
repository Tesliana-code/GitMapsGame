"""Minimal GitHub GraphQL client with live, recording and replay transports (stdlib only)."""
import copy
import json
import os
import urllib.error
import urllib.request

API_URL = "https://api.github.com/graphql"


class GitHubError(RuntimeError):
    pass


class LiveTransport:
    """Sends read-only GraphQL queries. The token is never logged or returned."""

    def __init__(self, token=None, url=API_URL, timeout=30):
        self._token = token or os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
        if not self._token:
            raise GitHubError("no token: set GH_TOKEN or GITHUB_TOKEN (read-only scope is sufficient)")
        self._url = url
        self._timeout = timeout

    def __call__(self, name, query, variables):
        body = json.dumps({"query": query, "variables": variables}).encode("utf-8")
        request = urllib.request.Request(self._url, data=body, method="POST", headers={
            "Authorization": "bearer " + self._token,
            "Content-Type": "application/json",
            "User-Agent": "gitmapsgame",
        })
        try:
            with urllib.request.urlopen(request, timeout=self._timeout) as response:
                return json.loads(response.read().decode("utf-8"))
        except urllib.error.HTTPError as exc:
            raise GitHubError("GitHub GraphQL HTTP %s for %s" % (exc.code, name)) from None
        except urllib.error.URLError as exc:
            raise GitHubError("GitHub GraphQL unreachable for %s: %s" % (name, exc.reason)) from None


class RecordingTransport:
    """Wraps another transport and keeps every exchange for offline replay."""

    def __init__(self, inner, redact=None):
        self._inner = inner
        self._redact = redact
        self.exchanges = []

    def __call__(self, name, query, variables):
        response = self._inner(name, query, variables)
        if self._redact:
            response = self._redact(name, response)
        # Store a private copy: callers may modify the returned object (fetch merges linked-PR pages
        # into it), and the recording must keep the response exactly as GitHub sent it.
        self.exchanges.append({"name": name, "variables": copy.deepcopy(variables), "response": copy.deepcopy(response)})
        return response


class ReplayTransport:
    """Serves recorded responses in order; any deviation from the recording is an error."""

    def __init__(self, exchanges):
        self._exchanges = list(exchanges)
        self._position = 0

    def __call__(self, name, query, variables):
        if self._position >= len(self._exchanges):
            raise GitHubError("replay exhausted at %s %r" % (name, variables))
        expected = self._exchanges[self._position]
        if expected["name"] != name or expected["variables"] != variables:
            raise GitHubError("replay mismatch: expected %s %r, got %s %r" % (
                expected["name"], expected["variables"], name, variables))
        self._position += 1
        return expected["response"]

    def assert_consumed(self):
        if self._position != len(self._exchanges):
            raise GitHubError("replay has %d unused exchanges" % (len(self._exchanges) - self._position))
