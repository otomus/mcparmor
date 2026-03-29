//! Armor manifest — parsing and validation of `armor.json`.
//!
//! The manifest is the capability declaration that travels with the tool.
//! Every enforcement decision in the broker derives from the parsed manifest.

use serde::{Deserialize, Serialize};

/// The complete parsed armor manifest.
///
/// Deserializes directly from `armor.json`. All optional fields that
/// are absent are treated as their documented defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmorManifest {
    /// Spec version this manifest was written against (e.g. "1.0").
    pub version: String,

    /// Base capability profile. Determines the default allow/deny posture
    /// before per-field overrides are applied.
    pub profile: Profile,

    /// Filesystem capability declarations.
    #[serde(default)]
    pub filesystem: FilesystemPolicy,

    /// Network capability declarations.
    #[serde(default)]
    pub network: NetworkPolicy,

    /// Whether the tool may spawn child processes.
    #[serde(default)]
    pub spawn: bool,

    /// Environment variable policy.
    #[serde(default)]
    pub env: EnvPolicy,

    /// Output scanning and size policy.
    #[serde(default)]
    pub output: OutputPolicy,

    /// Audit log configuration.
    #[serde(default)]
    pub audit: AuditPolicy,

    /// Tool call timeout in milliseconds.
    pub timeout_ms: Option<u32>,

    /// When true, the broker ignores --profile flag overrides.
    /// This is a cooperative lock — not enforced at the kernel level.
    #[serde(default)]
    pub locked: bool,

    /// Minimum armor spec version required to enforce this manifest.
    /// Used by community profiles and version-aware brokers.
    /// Example: "1.0"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_spec: Option<String>,
}

/// Base capability profile determining the default enforcement posture.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    /// Maximum restriction: no filesystem, no network, no spawn, no env.
    Strict,
    /// Filesystem + network as declared, no spawn.
    Sandboxed,
    /// Network-only tool, no local filesystem access.
    Network,
    /// Trusted tool — all capabilities explicitly declared.
    System,
    /// Browser/CDP tool — deny_local is implicitly false.
    Browser,
}

/// Filesystem read/write path declarations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FilesystemPolicy {
    /// Glob patterns for paths the tool may read.
    #[serde(default)]
    pub read: Vec<String>,

    /// Glob patterns for paths the tool may write.
    #[serde(default)]
    pub write: Vec<String>,
}

/// Network allow-list and deny rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicy {
    /// Allowed destinations as `host:port` or `host:*` or `*.domain:port`.
    /// Enforced at Layer 1 on all platforms; at OS level on macOS (Seatbelt)
    /// and Linux 6.7+ (Landlock TCP).
    #[serde(default)]
    pub allow: Vec<String>,

    /// Block connections to 127.0.0.0/8 and ::1.
    #[serde(default = "default_true")]
    pub deny_local: bool,

    /// Block connections to 169.254.0.0/16 (cloud metadata endpoints).
    #[serde(default = "default_true")]
    pub deny_metadata: bool,
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self {
            allow: Vec::new(),
            deny_local: true,
            deny_metadata: true,
        }
    }
}

/// Environment variable pass-through policy.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvPolicy {
    /// Names of environment variables the tool may receive.
    /// All others are stripped at spawn time.
    #[serde(default)]
    pub allow: Vec<String>,
}

/// Output scanning and size limiting policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputPolicy {
    /// Secret scanning behaviour.
    /// - false: disabled
    /// - true: redact detected secrets in-place
    /// - "strict": block the entire response on any detection
    #[serde(default)]
    pub scan_secrets: SecretScanMode,

    /// Maximum response size in KB. Responses exceeding this are truncated.
    pub max_size_kb: Option<u32>,
}

impl Default for OutputPolicy {
    fn default() -> Self {
        Self {
            scan_secrets: SecretScanMode::Disabled,
            max_size_kb: None,
        }
    }
}

/// How secret scanning handles detected secrets in tool responses.
///
/// Deserializes from `armor.json` as:
/// - `false`     → `Disabled`
/// - `true`      → `Redact`
/// - `"strict"`  → `Strict`
///
/// A custom `Deserialize` impl is required because `#[serde(untagged)]` cannot
/// deserialize a unit variant from a string — it treats unit variants as `null`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SecretScanMode {
    /// Secret scanning is disabled. Responses pass through unchanged.
    #[default]
    Disabled,
    /// Detected secrets are redacted in-place before the response reaches the host.
    Redact,
    /// The entire response is blocked if any secret is detected.
    Strict,
}

impl Serialize for SecretScanMode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            SecretScanMode::Disabled => s.serialize_bool(false),
            SecretScanMode::Redact => s.serialize_bool(true),
            SecretScanMode::Strict => s.serialize_str("strict"),
        }
    }
}

impl<'de> Deserialize<'de> for SecretScanMode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // Intermediate representation to handle the oneOf [boolean, "strict"] schema type.
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Bool(bool),
            Str(String),
        }

        match Raw::deserialize(d)? {
            Raw::Bool(false) => Ok(SecretScanMode::Disabled),
            Raw::Bool(true) => Ok(SecretScanMode::Redact),
            Raw::Str(s) if s == "strict" => Ok(SecretScanMode::Strict),
            Raw::Str(s) => Err(serde::de::Error::custom(format!(
                r#"invalid value for scan_secrets: "{s}", expected true, false, or "strict""#
            ))),
        }
    }
}

/// Audit log configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditPolicy {
    /// Whether audit logging is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Number of days to retain audit log entries before auto-pruning.
    pub retention_days: Option<u32>,

    /// Maximum audit log size in megabytes before rotation.
    pub max_size_mb: Option<u32>,

    /// When true, parameter values are omitted from audit log entries.
    /// Only parameter keys are logged — protects PII in audit trails.
    #[serde(default)]
    pub redact_params: bool,
}

impl Default for AuditPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            retention_days: None,
            max_size_mb: None,
            redact_params: false,
        }
    }
}

/// Returns `true` as the default value for serde boolean fields that default to enabled.
fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------------------
    // SecretScanMode — custom Deserialize
    // ---------------------------------------------------------------------------

    #[test]
    fn secret_scan_mode_false_deserializes_to_disabled() {
        let mode: SecretScanMode = serde_json::from_str("false").unwrap();
        assert_eq!(mode, SecretScanMode::Disabled);
    }

    #[test]
    fn secret_scan_mode_true_deserializes_to_redact() {
        let mode: SecretScanMode = serde_json::from_str("true").unwrap();
        assert_eq!(mode, SecretScanMode::Redact);
    }

    #[test]
    fn secret_scan_mode_strict_string_deserializes_to_strict() {
        let mode: SecretScanMode = serde_json::from_str(r#""strict""#).unwrap();
        assert_eq!(mode, SecretScanMode::Strict);
    }

    #[test]
    fn secret_scan_mode_invalid_string_returns_error() {
        let result: Result<SecretScanMode, _> = serde_json::from_str(r#""always""#);
        assert!(result.is_err(), "unknown string must be rejected");
        let msg = result.err().unwrap().to_string();
        assert!(
            msg.contains("invalid value for scan_secrets"),
            "error must describe the problem: {msg}"
        );
    }

    #[test]
    fn secret_scan_mode_number_returns_error() {
        // Numbers are not in the oneOf [boolean, "strict"] schema.
        let result: Result<SecretScanMode, _> = serde_json::from_str("42");
        assert!(result.is_err(), "number must be rejected");
    }

    #[test]
    fn secret_scan_mode_null_returns_error() {
        let result: Result<SecretScanMode, _> = serde_json::from_str("null");
        assert!(result.is_err(), "null must be rejected");
    }

    #[test]
    fn secret_scan_mode_object_returns_error() {
        let result: Result<SecretScanMode, _> = serde_json::from_str(r#"{"mode":"strict"}"#);
        assert!(result.is_err(), "object must be rejected");
    }

    #[test]
    fn secret_scan_mode_empty_string_returns_error() {
        let result: Result<SecretScanMode, _> = serde_json::from_str(r#""""#);
        assert!(result.is_err(), "empty string must be rejected");
    }

    // ---------------------------------------------------------------------------
    // SecretScanMode — custom Serialize
    // ---------------------------------------------------------------------------

    #[test]
    fn secret_scan_mode_disabled_serializes_to_false() {
        let json = serde_json::to_string(&SecretScanMode::Disabled).unwrap();
        assert_eq!(json, "false");
    }

    #[test]
    fn secret_scan_mode_redact_serializes_to_true() {
        let json = serde_json::to_string(&SecretScanMode::Redact).unwrap();
        assert_eq!(json, "true");
    }

    #[test]
    fn secret_scan_mode_strict_serializes_to_strict_string() {
        let json = serde_json::to_string(&SecretScanMode::Strict).unwrap();
        assert_eq!(json, r#""strict""#);
    }

    // ---------------------------------------------------------------------------
    // SecretScanMode — serialize-deserialize round-trip
    // ---------------------------------------------------------------------------

    #[test]
    fn secret_scan_mode_round_trip_disabled() {
        let original = SecretScanMode::Disabled;
        let json = serde_json::to_string(&original).unwrap();
        let recovered: SecretScanMode = serde_json::from_str(&json).unwrap();
        assert_eq!(recovered, original);
    }

    #[test]
    fn secret_scan_mode_round_trip_redact() {
        let original = SecretScanMode::Redact;
        let json = serde_json::to_string(&original).unwrap();
        let recovered: SecretScanMode = serde_json::from_str(&json).unwrap();
        assert_eq!(recovered, original);
    }

    #[test]
    fn secret_scan_mode_round_trip_strict() {
        let original = SecretScanMode::Strict;
        let json = serde_json::to_string(&original).unwrap();
        let recovered: SecretScanMode = serde_json::from_str(&json).unwrap();
        assert_eq!(recovered, original);
    }

    // ---------------------------------------------------------------------------
    // NetworkPolicy defaults
    // ---------------------------------------------------------------------------

    #[test]
    fn network_policy_default_deny_local_is_true() {
        let policy = NetworkPolicy::default();
        assert!(policy.deny_local, "deny_local must default to true");
    }

    #[test]
    fn network_policy_default_deny_metadata_is_true() {
        let policy = NetworkPolicy::default();
        assert!(policy.deny_metadata, "deny_metadata must default to true");
    }

    #[test]
    fn network_policy_default_allow_list_is_empty() {
        let policy = NetworkPolicy::default();
        assert!(policy.allow.is_empty());
    }

    // ---------------------------------------------------------------------------
    // AuditPolicy defaults
    // ---------------------------------------------------------------------------

    #[test]
    fn audit_policy_default_enabled_is_true() {
        let policy = AuditPolicy::default();
        assert!(policy.enabled, "audit must be enabled by default");
    }

    #[test]
    fn audit_policy_default_redact_params_is_false() {
        let policy = AuditPolicy::default();
        assert!(!policy.redact_params);
    }

    #[test]
    fn audit_policy_default_retention_days_is_none() {
        let policy = AuditPolicy::default();
        assert!(policy.retention_days.is_none());
    }

    // ---------------------------------------------------------------------------
    // ArmorManifest — deserialization from JSON
    // ---------------------------------------------------------------------------

    #[test]
    fn minimal_manifest_deserializes_successfully() {
        let json = r#"{
            "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
            "version": "1.0",
            "profile": "strict"
        }"#;
        let manifest: ArmorManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.version, "1.0");
        assert_eq!(manifest.profile, Profile::Strict);
        // Optional fields should take their defaults.
        assert!(manifest.filesystem.read.is_empty());
        assert!(manifest.filesystem.write.is_empty());
        assert!(manifest.network.deny_local);
        assert!(manifest.network.deny_metadata);
        assert!(!manifest.spawn);
        assert!(manifest.env.allow.is_empty());
        assert_eq!(manifest.output.scan_secrets, SecretScanMode::Disabled);
        assert!(manifest.output.max_size_kb.is_none());
        assert!(manifest.audit.enabled);
        assert!(manifest.timeout_ms.is_none());
        assert!(!manifest.locked);
        assert!(manifest.min_spec.is_none());
    }

    #[test]
    fn all_profiles_deserialize_correctly() {
        let cases = [
            ("strict", Profile::Strict),
            ("sandboxed", Profile::Sandboxed),
            ("network", Profile::Network),
            ("system", Profile::System),
            ("browser", Profile::Browser),
        ];
        for (name, expected) in cases {
            let json = format!(r#"{{"version":"1.0","profile":"{name}"}}"#);
            let manifest: ArmorManifest = serde_json::from_str(&json).unwrap();
            assert_eq!(manifest.profile, expected, "profile '{name}' must parse correctly");
        }
    }

    #[test]
    fn unknown_profile_returns_deserialization_error() {
        let json = r#"{"version":"1.0","profile":"ultra"}"#;
        let result: Result<ArmorManifest, _> = serde_json::from_str(json);
        assert!(result.is_err(), "unknown profile must be rejected");
    }

    #[test]
    fn manifest_with_scan_secrets_true_deserializes_to_redact() {
        let json = r#"{"version":"1.0","profile":"strict","output":{"scan_secrets":true}}"#;
        let manifest: ArmorManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.output.scan_secrets, SecretScanMode::Redact);
    }

    #[test]
    fn manifest_with_scan_secrets_strict_string_deserializes_correctly() {
        let json = r#"{"version":"1.0","profile":"strict","output":{"scan_secrets":"strict"}}"#;
        let manifest: ArmorManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.output.scan_secrets, SecretScanMode::Strict);
    }

    #[test]
    fn manifest_with_timeout_ms_deserializes_correctly() {
        let json = r#"{"version":"1.0","profile":"strict","timeout_ms":30000}"#;
        let manifest: ArmorManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.timeout_ms, Some(30000));
    }

    #[test]
    fn manifest_with_locked_true_deserializes_correctly() {
        let json = r#"{"version":"1.0","profile":"strict","locked":true}"#;
        let manifest: ArmorManifest = serde_json::from_str(json).unwrap();
        assert!(manifest.locked);
    }

    #[test]
    fn manifest_with_min_spec_deserializes_correctly() {
        let json = r#"{"version":"1.0","profile":"strict","min_spec":"1.0"}"#;
        let manifest: ArmorManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.min_spec.as_deref(), Some("1.0"));
    }

    #[test]
    fn manifest_min_spec_omitted_from_serialization_when_none() {
        let manifest = ArmorManifest {
            version: "1.0".to_string(),
            profile: Profile::Strict,
            filesystem: FilesystemPolicy::default(),
            network: NetworkPolicy::default(),
            spawn: false,
            env: EnvPolicy::default(),
            output: OutputPolicy::default(),
            audit: AuditPolicy::default(),
            timeout_ms: None,
            locked: false,
            min_spec: None,
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            value.get("min_spec").is_none(),
            "min_spec must be omitted when None: {json}"
        );
    }

    #[test]
    fn manifest_with_missing_version_returns_error() {
        let json = r#"{"profile":"strict"}"#;
        let result: Result<ArmorManifest, _> = serde_json::from_str(json);
        assert!(result.is_err(), "missing version must be rejected");
    }

    #[test]
    fn manifest_with_missing_profile_returns_error() {
        let json = r#"{"version":"1.0"}"#;
        let result: Result<ArmorManifest, _> = serde_json::from_str(json);
        assert!(result.is_err(), "missing profile must be rejected");
    }

    #[test]
    fn manifest_with_empty_json_object_returns_error() {
        let result: Result<ArmorManifest, _> = serde_json::from_str("{}");
        assert!(result.is_err(), "empty object must be rejected (missing required fields)");
    }

    #[test]
    fn manifest_with_json_array_returns_error() {
        let result: Result<ArmorManifest, _> = serde_json::from_str("[]");
        assert!(result.is_err(), "array is not a valid manifest");
    }

    #[test]
    fn manifest_with_json_null_returns_error() {
        let result: Result<ArmorManifest, _> = serde_json::from_str("null");
        assert!(result.is_err(), "null is not a valid manifest");
    }

    // ---------------------------------------------------------------------------
    // ArmorManifest — serialize-deserialize round-trip
    // ---------------------------------------------------------------------------

    #[test]
    fn full_manifest_survives_round_trip() {
        let original = ArmorManifest {
            version: "1.0".to_string(),
            profile: Profile::Network,
            filesystem: FilesystemPolicy {
                read: vec!["/tmp/**".to_string()],
                write: vec!["/workspace/**".to_string()],
            },
            network: NetworkPolicy {
                allow: vec!["api.example.com:443".to_string()],
                deny_local: true,
                deny_metadata: true,
            },
            spawn: true,
            env: EnvPolicy {
                allow: vec!["PATH".to_string(), "HOME".to_string()],
            },
            output: OutputPolicy {
                scan_secrets: SecretScanMode::Strict,
                max_size_kb: Some(1024),
            },
            audit: AuditPolicy {
                enabled: true,
                retention_days: Some(30),
                max_size_mb: Some(100),
                redact_params: true,
            },
            timeout_ms: Some(5000),
            locked: true,
            min_spec: Some("1.0".to_string()),
        };

        let json = serde_json::to_string(&original).unwrap();
        let recovered: ArmorManifest = serde_json::from_str(&json).unwrap();

        assert_eq!(recovered.version, original.version);
        assert_eq!(recovered.profile, original.profile);
        assert_eq!(recovered.filesystem.read, original.filesystem.read);
        assert_eq!(recovered.filesystem.write, original.filesystem.write);
        assert_eq!(recovered.network.allow, original.network.allow);
        assert_eq!(recovered.network.deny_local, original.network.deny_local);
        assert_eq!(recovered.network.deny_metadata, original.network.deny_metadata);
        assert_eq!(recovered.spawn, original.spawn);
        assert_eq!(recovered.env.allow, original.env.allow);
        assert_eq!(recovered.output.scan_secrets, original.output.scan_secrets);
        assert_eq!(recovered.output.max_size_kb, original.output.max_size_kb);
        assert_eq!(recovered.audit.enabled, original.audit.enabled);
        assert_eq!(recovered.audit.retention_days, original.audit.retention_days);
        assert_eq!(recovered.audit.max_size_mb, original.audit.max_size_mb);
        assert_eq!(recovered.audit.redact_params, original.audit.redact_params);
        assert_eq!(recovered.timeout_ms, original.timeout_ms);
        assert_eq!(recovered.locked, original.locked);
        assert_eq!(recovered.min_spec, original.min_spec);
    }

    // ---------------------------------------------------------------------------
    // Profile — serialization matches the schema's lowercase requirement
    // ---------------------------------------------------------------------------

    #[test]
    fn all_profiles_serialize_as_lowercase_strings() {
        let cases = [
            (Profile::Strict, "strict"),
            (Profile::Sandboxed, "sandboxed"),
            (Profile::Network, "network"),
            (Profile::System, "system"),
            (Profile::Browser, "browser"),
        ];
        for (profile, expected) in cases {
            let json = serde_json::to_string(&profile).unwrap();
            assert_eq!(json, format!(r#""{expected}""#), "profile must serialize as lowercase");
        }
    }
}
