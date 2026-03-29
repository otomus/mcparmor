/**
 * Expected SHA256 checksums for bundled mcparmor binaries.
 *
 * This file is generated at publish time by the CI pipeline. Each entry maps a
 * platform key (`${process.platform}-${process.arch}`) to the expected SHA256
 * hex digest of the binary bundled in the corresponding optional npm package.
 *
 * When this record is empty (development installs built from source), checksum
 * verification is skipped entirely. In published packages it is populated by CI
 * so that `findBinary()` can detect tampered binaries before returning their path.
 */

// Populated by CI during publish. Format:
//   "<platform>-<arch>": "<sha256_hex>"
// Example:
//   "darwin-arm64": "abcdef0123456789...",
//   "linux-x64":    "fedcba9876543210...",
export const BINARY_CHECKSUMS: Readonly<Record<string, string>> = {};
