"""ArmorManifest — parsed representation of an armor.json capability manifest."""

import fnmatch
import json
from pathlib import Path


# Known deny-list entries in the spec.
_DENY_METADATA_PREFIX = "169.254."
_DENY_LOCAL_HOSTS = {"localhost", "127.0.0.1", "::1"}


class ManifestLoadError(ValueError):
    """Raised when an armor.json file cannot be loaded or is structurally invalid."""


class ArmorManifest:
    """
    Parsed representation of an armor.json capability manifest.

    Provides structured access to the declared profile, network allow-list,
    and filesystem read/write allow-lists. Use :meth:`load` to construct
    from a file path or :meth:`from_dict` to construct from an already-parsed
    dictionary.
    """

    def __init__(
        self,
        *,
        version: str,
        profile: str | None,
        locked: bool,
        network_allow: list[str],
        fs_read: list[str],
        fs_write: list[str],
    ) -> None:
        self._version = version
        self._profile = profile
        self._locked = locked
        self._network_allow = network_allow
        self._fs_read = fs_read
        self._fs_write = fs_write

    # ------------------------------------------------------------------
    # Construction
    # ------------------------------------------------------------------

    @classmethod
    def load(cls, path: str | Path) -> "ArmorManifest":
        """
        Parse an armor.json file from disk.

        Args:
            path: Path to the armor.json file.

        Returns:
            An ArmorManifest instance populated from the file.

        Raises:
            ManifestLoadError: If the file does not exist, contains invalid
                JSON, or is missing the required ``version`` field.
        """
        resolved = Path(path)
        if not resolved.exists():
            raise ManifestLoadError(f"Manifest file not found: {path}")

        try:
            data = json.loads(resolved.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            raise ManifestLoadError(f"Invalid JSON in manifest {path}: {exc}") from exc

        return cls.from_dict(data)

    @classmethod
    def from_dict(cls, data: dict) -> "ArmorManifest":
        """
        Parse an ArmorManifest from a plain dictionary.

        Args:
            data: A dictionary matching the armor.json schema.

        Returns:
            An ArmorManifest instance.

        Raises:
            ManifestLoadError: If the ``version`` field is missing.
        """
        if "version" not in data:
            raise ManifestLoadError("Manifest is missing the required 'version' field.")

        network = data.get("network", {}) or {}
        filesystem = data.get("filesystem", {}) or {}

        return cls(
            version=data["version"],
            profile=data.get("profile"),
            locked=bool(data.get("locked", False)),
            network_allow=list(network.get("allow", []) or []),
            fs_read=list(filesystem.get("read", []) or []),
            fs_write=list(filesystem.get("write", []) or []),
        )

    # ------------------------------------------------------------------
    # Properties
    # ------------------------------------------------------------------

    @property
    def profile(self) -> str | None:
        """The declared profile name, or None if not specified."""
        return self._profile

    def is_locked(self) -> bool:
        """Return True if the manifest has locked: true, preventing profile overrides."""
        return self._locked

    # ------------------------------------------------------------------
    # Network checks
    # ------------------------------------------------------------------

    def allows_network(self, host: str, port: int) -> bool:
        """
        Check whether the manifest permits outbound network access to ``host:port``.

        Deny rules are checked before allow patterns:
        - Metadata IPs (169.254.0.0/16) are always denied.
        - Localhost addresses (localhost, 127.0.0.1, ::1) are always denied.

        Allow patterns are ``host:port`` strings where:
        - Host may be an exact value, a ``*.domain.com`` glob, or ``*``.
        - Port may be a number or ``*``.

        Args:
            host: Hostname or IP address to check.
            port: Port number to check.

        Returns:
            True if the host/port combination is explicitly allowed and not denied.
        """
        if _is_deny_metadata(host):
            return False
        if _is_deny_local(host):
            return False
        return any(_pattern_matches(pattern, host, port) for pattern in self._network_allow)

    # ------------------------------------------------------------------
    # Filesystem checks
    # ------------------------------------------------------------------

    def allows_path_read(self, path: str) -> bool:
        """
        Check whether the manifest permits reading the given path.

        Args:
            path: The filesystem path to check.

        Returns:
            True if at least one filesystem.read glob pattern matches ``path``.
        """
        return _any_glob_matches(self._fs_read, path)

    def allows_path_write(self, path: str) -> bool:
        """
        Check whether the manifest permits writing to the given path.

        Args:
            path: The filesystem path to check.

        Returns:
            True if at least one filesystem.write glob pattern matches ``path``.
        """
        return _any_glob_matches(self._fs_write, path)


# ------------------------------------------------------------------
# Private helpers
# ------------------------------------------------------------------


def _is_deny_metadata(host: str) -> bool:
    """Return True if the host is in the link-local metadata address range (169.254.x.x)."""
    return host.startswith(_DENY_METADATA_PREFIX)


def _is_deny_local(host: str) -> bool:
    """Return True if the host refers to localhost or a loopback address."""
    return host.lower() in _DENY_LOCAL_HOSTS


def _pattern_matches(pattern: str, host: str, port: int) -> bool:
    """
    Return True if a single ``host:port`` pattern matches the given host and port.

    Args:
        pattern: A pattern string such as ``api.github.com:443``, ``*.domain.com:*``,
            or ``*:80``.
        host: The hostname or IP to test.
        port: The port number to test.
    """
    if ":" not in pattern:
        # Malformed pattern — treat as non-matching rather than crashing.
        return False

    pattern_host, pattern_port = pattern.rsplit(":", 1)
    host_matches = fnmatch.fnmatch(host, pattern_host)
    port_matches = pattern_port == "*" or pattern_port == str(port)
    return host_matches and port_matches


def _any_glob_matches(patterns: list[str], path: str) -> bool:
    """Return True if any glob pattern from ``patterns`` matches ``path``."""
    return any(fnmatch.fnmatch(path, pattern) for pattern in patterns)
