"""Tests for ArmorManifest."""

import json
from pathlib import Path

import pytest

from mcparmor._manifest import ArmorManifest, ManifestLoadError

# ---------------------------------------------------------------------------
# Shared fixtures
# ---------------------------------------------------------------------------

_MINIMAL_VALID: dict = {"version": "1"}

_FULL_MANIFEST: dict = {
    "version": "1",
    "profile": "sandboxed",
    "locked": False,
    "network": {
        "allow": [
            "api.github.com:443",
            "*.example.com:443",
            "*:80",
            "*:*",
        ],
    },
    "filesystem": {
        "read": ["/etc/*", "/home/user/docs/**"],
        "write": ["/tmp/**"],
    },
}


def _write_json(path: Path, data: dict) -> Path:
    """Write a dict as JSON to the given path and return it."""
    path.write_text(json.dumps(data), encoding="utf-8")
    return path


# ---------------------------------------------------------------------------
# load() — file parsing
# ---------------------------------------------------------------------------


def test_load_valid_file(tmp_path: Path) -> None:
    """load() returns an ArmorManifest from a valid JSON file."""
    manifest_path = _write_json(tmp_path / "armor.json", _FULL_MANIFEST)
    manifest = ArmorManifest.load(manifest_path)
    assert manifest.profile == "sandboxed"


def test_load_accepts_string_path(tmp_path: Path) -> None:
    """load() accepts a plain string path as well as a Path object."""
    manifest_path = _write_json(tmp_path / "armor.json", _MINIMAL_VALID)
    manifest = ArmorManifest.load(str(manifest_path))
    assert manifest is not None


def test_load_missing_file_raises(tmp_path: Path) -> None:
    """load() raises ManifestLoadError when the file does not exist."""
    with pytest.raises(ManifestLoadError, match="not found"):
        ArmorManifest.load(tmp_path / "nonexistent.json")


def test_load_invalid_json_raises(tmp_path: Path) -> None:
    """load() raises ManifestLoadError when the file contains invalid JSON."""
    bad_path = tmp_path / "bad.json"
    bad_path.write_text("{not valid json", encoding="utf-8")
    with pytest.raises(ManifestLoadError, match="Invalid JSON"):
        ArmorManifest.load(bad_path)


def test_load_missing_version_raises(tmp_path: Path) -> None:
    """load() raises ManifestLoadError when the 'version' field is absent."""
    no_version = _write_json(tmp_path / "armor.json", {"profile": "sandboxed"})
    with pytest.raises(ManifestLoadError, match="version"):
        ArmorManifest.load(no_version)


# ---------------------------------------------------------------------------
# from_dict() — dict parsing
# ---------------------------------------------------------------------------


def test_from_dict_minimal() -> None:
    """from_dict() succeeds with only the required 'version' field."""
    manifest = ArmorManifest.from_dict({"version": "1"})
    assert manifest.profile is None
    assert manifest.is_locked() is False


def test_from_dict_full() -> None:
    """from_dict() correctly populates all fields from a full manifest dict."""
    manifest = ArmorManifest.from_dict(_FULL_MANIFEST)
    assert manifest.profile == "sandboxed"
    assert manifest.is_locked() is False


def test_from_dict_missing_version_raises() -> None:
    """from_dict() raises ManifestLoadError when 'version' is absent."""
    with pytest.raises(ManifestLoadError, match="version"):
        ArmorManifest.from_dict({"profile": "sandboxed"})


def test_from_dict_empty_dict_raises() -> None:
    """from_dict() raises ManifestLoadError for an empty dict."""
    with pytest.raises(ManifestLoadError):
        ArmorManifest.from_dict({})


# ---------------------------------------------------------------------------
# profile property
# ---------------------------------------------------------------------------


def test_profile_is_none_when_absent() -> None:
    """profile returns None when the field is not in the manifest."""
    manifest = ArmorManifest.from_dict({"version": "1"})
    assert manifest.profile is None


def test_profile_returns_declared_value() -> None:
    """profile returns the exact string declared in the manifest."""
    manifest = ArmorManifest.from_dict({"version": "1", "profile": "strict"})
    assert manifest.profile == "strict"


# ---------------------------------------------------------------------------
# is_locked()
# ---------------------------------------------------------------------------


def test_is_locked_defaults_to_false() -> None:
    """is_locked() returns False when the locked field is absent."""
    manifest = ArmorManifest.from_dict({"version": "1"})
    assert manifest.is_locked() is False


def test_is_locked_true_when_declared() -> None:
    """is_locked() returns True when locked: true is set."""
    manifest = ArmorManifest.from_dict({"version": "1", "locked": True})
    assert manifest.is_locked() is True


def test_is_locked_explicit_false() -> None:
    """is_locked() returns False when locked: false is explicitly set."""
    manifest = ArmorManifest.from_dict({"version": "1", "locked": False})
    assert manifest.is_locked() is False


# ---------------------------------------------------------------------------
# allows_network() — allow patterns
# ---------------------------------------------------------------------------


def test_network_exact_host_and_port_match() -> None:
    """allows_network() returns True for an exact host:port match."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "network": {"allow": ["api.github.com:443"]},
    })
    assert manifest.allows_network("api.github.com", 443) is True


def test_network_exact_host_wrong_port() -> None:
    """allows_network() returns False when the port does not match."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "network": {"allow": ["api.github.com:443"]},
    })
    assert manifest.allows_network("api.github.com", 80) is False


def test_network_wildcard_host_glob() -> None:
    """allows_network() returns True when the host matches a *.domain glob."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "network": {"allow": ["*.example.com:443"]},
    })
    assert manifest.allows_network("api.example.com", 443) is True
    assert manifest.allows_network("other.example.com", 443) is True


def test_network_wildcard_host_does_not_match_root() -> None:
    """*.example.com should not match example.com itself."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "network": {"allow": ["*.example.com:443"]},
    })
    assert manifest.allows_network("example.com", 443) is False


def test_network_wildcard_port() -> None:
    """allows_network() returns True when port is * in the pattern."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "network": {"allow": ["api.github.com:*"]},
    })
    assert manifest.allows_network("api.github.com", 80) is True
    assert manifest.allows_network("api.github.com", 443) is True


def test_network_star_host_matches_any() -> None:
    """allows_network() returns True when host pattern is *."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "network": {"allow": ["*:443"]},
    })
    assert manifest.allows_network("anything.io", 443) is True


def test_network_deny_metadata_ip() -> None:
    """allows_network() returns False for 169.254.x.x addresses regardless of allow list."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "network": {"allow": ["*:*"]},
    })
    assert manifest.allows_network("169.254.169.254", 80) is False
    assert manifest.allows_network("169.254.0.1", 443) is False


def test_network_deny_localhost() -> None:
    """allows_network() returns False for localhost regardless of allow list."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "network": {"allow": ["*:*"]},
    })
    assert manifest.allows_network("localhost", 8080) is False


def test_network_deny_loopback_ipv4() -> None:
    """allows_network() returns False for 127.0.0.1 regardless of allow list."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "network": {"allow": ["*:*"]},
    })
    assert manifest.allows_network("127.0.0.1", 80) is False


def test_network_deny_loopback_ipv6() -> None:
    """allows_network() returns False for ::1 regardless of allow list."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "network": {"allow": ["*:*"]},
    })
    assert manifest.allows_network("::1", 443) is False


def test_network_empty_allow_list() -> None:
    """allows_network() returns False when network.allow is empty."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "network": {"allow": []},
    })
    assert manifest.allows_network("api.github.com", 443) is False


def test_network_no_network_section() -> None:
    """allows_network() returns False when network is absent from the manifest."""
    manifest = ArmorManifest.from_dict({"version": "1"})
    assert manifest.allows_network("api.github.com", 443) is False


# ---------------------------------------------------------------------------
# allows_path_read()
# ---------------------------------------------------------------------------


def test_path_read_matches_glob() -> None:
    """allows_path_read() returns True when path matches a read glob."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "filesystem": {"read": ["/etc/*"]},
    })
    assert manifest.allows_path_read("/etc/hosts") is True


def test_path_read_non_matching_path() -> None:
    """allows_path_read() returns False when no read pattern covers the path."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "filesystem": {"read": ["/etc/*"]},
    })
    assert manifest.allows_path_read("/etc/passwd") is True
    assert manifest.allows_path_read("/home/user/secret") is False


def test_path_read_no_filesystem_section() -> None:
    """allows_path_read() returns False when filesystem is absent."""
    manifest = ArmorManifest.from_dict({"version": "1"})
    assert manifest.allows_path_read("/etc/hosts") is False


def test_path_read_passwd_blocked_when_not_in_patterns() -> None:
    """allows_path_read() returns False for /etc/passwd when only /tmp/** is allowed."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "filesystem": {"read": ["/tmp/**"]},
    })
    assert manifest.allows_path_read("/etc/passwd") is False


# ---------------------------------------------------------------------------
# allows_path_write()
# ---------------------------------------------------------------------------


def test_path_write_matches_glob() -> None:
    """allows_path_write() returns True when path matches a write glob."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "filesystem": {"write": ["/tmp/**"]},
    })
    assert manifest.allows_path_write("/tmp/output.txt") is True


def test_path_write_non_matching_path() -> None:
    """allows_path_write() returns False when no write pattern covers the path."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "filesystem": {"write": ["/tmp/**"]},
    })
    assert manifest.allows_path_write("/etc/passwd") is False


def test_path_write_no_filesystem_section() -> None:
    """allows_path_write() returns False when filesystem is absent."""
    manifest = ArmorManifest.from_dict({"version": "1"})
    assert manifest.allows_path_write("/tmp/out") is False


# ---------------------------------------------------------------------------
# Edge cases — missing optional fields
# ---------------------------------------------------------------------------


def test_null_network_section_is_tolerated() -> None:
    """A null network value is treated as an absent section."""
    manifest = ArmorManifest.from_dict({"version": "1", "network": None})
    assert manifest.allows_network("api.github.com", 443) is False


def test_null_filesystem_section_is_tolerated() -> None:
    """A null filesystem value is treated as an absent section."""
    manifest = ArmorManifest.from_dict({"version": "1", "filesystem": None})
    assert manifest.allows_path_read("/tmp/file") is False


def test_null_allow_list_is_tolerated() -> None:
    """A null network.allow value is treated as an empty list."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "network": {"allow": None},
    })
    assert manifest.allows_network("api.github.com", 443) is False


def test_malformed_network_pattern_does_not_crash() -> None:
    """A network.allow pattern without ':' is silently skipped."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "network": {"allow": ["no-colon-here"]},
    })
    assert manifest.allows_network("no-colon-here", 80) is False


def test_extra_fields_are_ignored() -> None:
    """Unexpected extra fields in the manifest dict do not cause errors."""
    manifest = ArmorManifest.from_dict({
        "version": "1",
        "unknown_future_field": True,
        "also_unknown": {"nested": "data"},
    })
    assert manifest is not None
