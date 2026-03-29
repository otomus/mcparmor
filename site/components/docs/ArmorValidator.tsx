"use client";

import { useState, useCallback, useRef, useEffect } from "react";
import Ajv2020 from "ajv/dist/2020";
import addFormats from "ajv-formats";
import type { ErrorObject } from "ajv/dist/2020";

// ---------------------------------------------------------------------------
// Schema (embedded to avoid cross-package import)
// ---------------------------------------------------------------------------

/* eslint-disable @typescript-eslint/no-explicit-any */
const armorSchema: Record<string, unknown> = {
  $schema: "https://json-schema.org/draft/2020-12/schema",
  $id: "https://mcp-armor.com/spec/v1.0/armor.schema.json",
  title: "MCP Armor Manifest",
  type: "object",
  required: ["$schema", "version", "profile"],
  additionalProperties: false,
  properties: {
    $schema: {
      type: "string",
      const: "https://mcp-armor.com/spec/v1.0/armor.schema.json",
    },
    version: { type: "string", pattern: "^\\d+\\.\\d+$" },
    min_spec: { type: "string", pattern: "^\\d+\\.\\d+$" },
    profile: {
      type: "string",
      enum: ["strict", "sandboxed", "network", "system", "browser"],
    },
    locked: { type: "boolean", default: false },
    timeout_ms: { type: "integer", minimum: 100, maximum: 300000 },
    filesystem: {
      type: "object",
      additionalProperties: false,
      properties: {
        read: { type: "array", items: { type: "string" } },
        write: { type: "array", items: { type: "string" } },
      },
    },
    network: {
      type: "object",
      additionalProperties: false,
      properties: {
        allow: {
          type: "array",
          items: {
            type: "string",
            pattern:
              "^((\\*\\.[a-zA-Z0-9.-]+|[a-zA-Z0-9.-]+):(\\*|[0-9]{1,5})|\\*:[0-9]{1,5})$",
          },
        },
        deny_local: { type: "boolean", default: true },
        deny_metadata: { type: "boolean", default: true },
      },
    },
    spawn: { type: "boolean", default: false },
    env: {
      type: "object",
      additionalProperties: false,
      properties: {
        allow: { type: "array", items: { type: "string" } },
      },
    },
    output: {
      type: "object",
      additionalProperties: false,
      properties: {
        scan_secrets: {
          oneOf: [
            { type: "boolean" },
            { type: "string", const: "strict" },
          ],
        },
        max_size_kb: { type: "integer", minimum: 1, maximum: 102400 },
      },
    },
    audit: {
      type: "object",
      additionalProperties: false,
      properties: {
        enabled: { type: "boolean", default: true },
        retention_days: { type: "integer", minimum: 1, maximum: 365 },
        max_size_mb: { type: "integer", minimum: 1, maximum: 10240 },
        redact_params: { type: "boolean", default: false },
      },
    },
    _source: { type: "string" },
    _reviewed_by: { type: "string" },
    _reviewed_at: { type: "string" },
  },
};
/* eslint-enable @typescript-eslint/no-explicit-any */

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEBOUNCE_DELAY_MS = 300;

const EXAMPLE_MANIFEST = JSON.stringify(
  {
    $schema: "https://mcp-armor.com/spec/v1.0/armor.schema.json",
    version: "1.0",
    profile: "sandboxed",
    filesystem: {
      read: ["/tmp/mcparmor/*"],
      write: ["/tmp/mcparmor/*"],
    },
    network: {
      allow: ["api.github.com:443"],
      deny_local: true,
      deny_metadata: true,
    },
    spawn: false,
    env: { allow: ["GITHUB_TOKEN"] },
    output: { scan_secrets: true },
    timeout_ms: 30000,
  },
  null,
  2,
);

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type ValidationIdle = { status: "idle" };
type ValidationValid = { status: "valid" };
type ValidationParseError = { status: "parse-error"; message: string };
type ValidationSchemaErrors = {
  status: "schema-errors";
  errors: ReadonlyArray<FormattedError>;
};

type ValidationResult =
  | ValidationIdle
  | ValidationValid
  | ValidationParseError
  | ValidationSchemaErrors;

interface FormattedError {
  readonly path: string;
  readonly message: string;
}

// ---------------------------------------------------------------------------
// Validator setup (module-level singleton)
// ---------------------------------------------------------------------------

/** Create the Ajv 2020-12 validator instance once at module scope. */
function createValidator(): Ajv2020 {
  const ajv = new Ajv2020({ allErrors: true, strict: false });
  addFormats(ajv);
  return ajv;
}

const ajv = createValidator();
const validateSchema = ajv.compile(armorSchema);

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/**
 * Attempt to parse a raw string as JSON.
 * Returns the parsed value on success, or a parse-error result on failure.
 */
function tryParseJson(raw: string): { ok: true; value: unknown } | { ok: false; result: ValidationParseError } {
  try {
    return { ok: true, value: JSON.parse(raw) };
  } catch (err: unknown) {
    const message = err instanceof SyntaxError ? err.message : "Invalid JSON";
    return { ok: false, result: { status: "parse-error", message } };
  }
}

/**
 * Format an Ajv error into a user-friendly path + message pair.
 */
function formatAjvError(error: ErrorObject): FormattedError {
  const path = error.instancePath || "/";
  const message = error.message ?? "Unknown validation error";
  return { path, message };
}

/**
 * Validate a raw JSON string against the armor.json schema.
 * Handles empty input, parse errors, and schema validation errors.
 */
function validateArmorJson(raw: string): ValidationResult {
  const trimmed = raw.trim();
  if (trimmed === "") {
    return { status: "idle" };
  }

  const parsed = tryParseJson(trimmed);
  if (!parsed.ok) {
    return parsed.result;
  }

  const isValid = validateSchema(parsed.value);
  if (isValid) {
    return { status: "valid" };
  }

  const errors = (validateSchema.errors ?? []).map(formatAjvError);
  return { status: "schema-errors", errors };
}

// ---------------------------------------------------------------------------
// Debounce hook
// ---------------------------------------------------------------------------

/**
 * Returns a debounced version of the callback. The callback is invoked after
 * `delayMs` milliseconds of inactivity. Cleans up on unmount.
 */
function useDebouncedCallback<T extends (...args: never[]) => void>(
  callback: T,
  delayMs: number,
): T {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const callbackRef = useRef(callback);
  callbackRef.current = callback;

  useEffect(() => {
    return () => {
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
      }
    };
  }, []);

  return useCallback(
    (...args: Parameters<T>) => {
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
      }
      timerRef.current = setTimeout(() => {
        callbackRef.current(...args);
      }, delayMs);
    },
    [delayMs],
  ) as T;
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

/** Renders the green "valid" banner. */
function ValidBanner(): React.ReactElement {
  return (
    <div
      role="status"
      style={{
        padding: "12px 16px",
        borderRadius: "var(--radius-md)",
        backgroundColor: "var(--color-blocked-bg, #f0fdf4)",
        color: "var(--color-blocked, #16a34a)",
        fontWeight: 600,
        fontSize: "var(--text-sm, 14px)",
      }}
    >
      Valid ✓
    </div>
  );
}

/** Renders a red parse-error message. */
function ParseErrorBanner({ message }: { message: string }): React.ReactElement {
  return (
    <div
      role="alert"
      style={{
        padding: "12px 16px",
        borderRadius: "var(--radius-md)",
        backgroundColor: "var(--color-allowed-bg, #fffbeb)",
        color: "var(--color-allowed, #b45309)",
        fontSize: "var(--text-sm, 14px)",
      }}
    >
      <strong>JSON Parse Error:</strong> {message}
    </div>
  );
}

/** Renders a list of schema validation errors with JSON paths. */
function SchemaErrorList({ errors }: { errors: ReadonlyArray<FormattedError> }): React.ReactElement {
  return (
    <div
      role="alert"
      style={{
        padding: "12px 16px",
        borderRadius: "var(--radius-md)",
        backgroundColor: "var(--color-allowed-bg, #fffbeb)",
        color: "var(--color-allowed, #b45309)",
        fontSize: "var(--text-sm, 14px)",
      }}
    >
      <strong style={{ display: "block", marginBottom: "8px" }}>
        Schema validation failed ({errors.length} {errors.length === 1 ? "error" : "errors"}):
      </strong>
      <ul style={{ margin: 0, paddingLeft: "20px", listStyleType: "disc" }}>
        {errors.map((err, idx) => (
          <li key={idx} style={{ marginBottom: "4px" }}>
            <code
              style={{
                fontFamily: "var(--font-mono, monospace)",
                backgroundColor: "var(--color-bg-muted, #f2f0ec)",
                padding: "1px 4px",
                borderRadius: "3px",
              }}
            >
              {err.path}
            </code>{" "}
            — {err.message}
          </li>
        ))}
      </ul>
    </div>
  );
}

/** Renders the appropriate result display for a given validation state. */
function ValidationDisplay({ result }: { result: ValidationResult }): React.ReactElement | null {
  switch (result.status) {
    case "idle":
      return null;
    case "valid":
      return <ValidBanner />;
    case "parse-error":
      return <ParseErrorBanner message={result.message} />;
    case "schema-errors":
      return <SchemaErrorList errors={result.errors} />;
  }
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

/**
 * Client-side armor.json validator widget.
 * Users paste their manifest JSON into the textarea, and validation results
 * appear in real-time (debounced 300ms) using the MCP Armor JSON Schema.
 */
export function ArmorValidator(): React.ReactElement {
  const [input, setInput] = useState("");
  const [result, setResult] = useState<ValidationResult>({ status: "idle" });

  const debouncedValidate = useDebouncedCallback((raw: string) => {
    setResult(validateArmorJson(raw));
  }, DEBOUNCE_DELAY_MS);

  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLTextAreaElement>) => {
      const value = e.target.value;
      setInput(value);
      debouncedValidate(value);
    },
    [debouncedValidate],
  );

  const handleLoadExample = useCallback(() => {
    setInput(EXAMPLE_MANIFEST);
    setResult(validateArmorJson(EXAMPLE_MANIFEST));
  }, []);

  return (
    <div>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: "12px",
        }}
      >
        <label
          htmlFor="armor-validator-input"
          style={{
            fontWeight: 600,
            color: "var(--color-text-primary)",
            fontSize: "var(--text-sm, 14px)",
          }}
        >
          Paste your armor.json below
        </label>
        <button
          type="button"
          onClick={handleLoadExample}
          style={{
            padding: "6px 14px",
            borderRadius: "var(--radius-md)",
            border: "1px solid var(--color-border-strong, #ccc)",
            backgroundColor: "var(--color-bg-muted, #f2f0ec)",
            color: "var(--color-text-primary)",
            fontFamily: "var(--font-mono, monospace)",
            fontSize: "var(--text-sm, 13px)",
            cursor: "pointer",
          }}
        >
          Load example
        </button>
      </div>

      <textarea
        id="armor-validator-input"
        aria-label="armor.json content to validate"
        value={input}
        onChange={handleChange}
        placeholder='{ "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json", ... }'
        spellCheck={false}
        rows={16}
        style={{
          width: "100%",
          fontFamily: "var(--font-mono, monospace)",
          fontSize: "var(--text-sm, 14px)",
          lineHeight: "1.6",
          padding: "16px",
          borderRadius: "var(--radius-md)",
          border: "1px solid var(--color-border-strong, #ccc)",
          backgroundColor: "var(--color-bg-muted, #f2f0ec)",
          color: "var(--color-text-primary)",
          resize: "vertical",
          outline: "none",
          boxSizing: "border-box",
        }}
      />

      <div style={{ marginTop: "12px" }}>
        <ValidationDisplay result={result} />
      </div>
    </div>
  );
}
