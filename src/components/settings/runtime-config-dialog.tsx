import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import type { RuntimeConfigField, RuntimeConfigSchema, RuntimeConfigValues } from "@/types/runtime-config";
import type { RuntimeInventoryItem } from "@/types/runtime";

interface RuntimeConfigDialogProps {
  error?: string;
  loading: boolean;
  openFileLoading: boolean;
  runtime: RuntimeInventoryItem;
  saving: boolean;
  schema: RuntimeConfigSchema | null;
  values: RuntimeConfigValues | null;
  onClose: () => void;
  onOpenFile: () => Promise<void> | void;
  onSave: (patch: Record<string, string>) => Promise<void> | void;
}

function renderFieldInput(
  field: RuntimeConfigField,
  value: string,
  disabled: boolean,
  onChange: (nextValue: string) => void,
) {
  if (field.kind === "toggle" || field.kind === "select") {
    return (
      <select
        className="select"
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
        value={value}
      >
        {field.options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    );
  }

  return (
    <input
      className="input"
      disabled={disabled}
      onChange={(event) => onChange(event.target.value)}
      placeholder={field.placeholder ?? undefined}
      type={field.kind === "number" ? "number" : "text"}
      value={value}
    />
  );
}

export function RuntimeConfigDialog({
  error,
  loading,
  openFileLoading,
  runtime,
  saving,
  schema,
  values,
  onClose,
  onOpenFile,
  onSave,
}: RuntimeConfigDialogProps) {
  const [draft, setDraft] = useState<Record<string, string>>({});

  useEffect(() => {
    if (!values) {
      setDraft({});
      return;
    }

    setDraft(values.values);
  }, [values]);

  const currentValues = values?.values ?? {};
  const dirty = Object.keys(draft).some((key) => draft[key] !== currentValues[key]);

  return (
    <div className="wizard-overlay" onClick={onClose} role="dialog" aria-modal="true">
      <div className="runtime-config-dialog" onClick={(event) => event.stopPropagation()}>
        <div className="runtime-tools-header">
          <div>
            <h2>Runtime Config</h2>
            <p>
              {runtime.runtimeType === "php"
                ? `Manage generated php.ini values for PHP ${runtime.version}.`
                : runtime.runtimeType === "mysql"
                  ? `Open the managed MySQL config file for ${runtime.version}.`
                  : runtime.runtimeType === "frankenphp"
                    ? `Open the managed FrankenPHP Caddyfile for ${runtime.version}. Structured editing is not available yet.`
                    : `Manage generated ${runtime.runtimeType === "apache" ? "Apache" : "Nginx"} bootstrap settings for ${runtime.version}.`}
            </p>
          </div>
          <div className="runtime-table-actions">
            <Button
              busy={openFileLoading}
              busyLabel="Opening config file..."
              disabled={loading || openFileLoading || saving}
              onClick={() => void onOpenFile()}
            >
              Open File
            </Button>
            <Button disabled={saving} onClick={onClose}>
              Close
            </Button>
          </div>
        </div>

        <div className="detail-grid">
          <div className="detail-item">
            <span className="detail-label">Runtime</span>
            <strong>
              {runtime.runtimeType.toUpperCase()} {runtime.version}
            </strong>
          </div>
          <div className="detail-item">
            <span className="detail-label">Source</span>
            <strong>{runtime.source}</strong>
          </div>
        </div>

        <div className="detail-item">
          <span className="detail-label">Config Path</span>
          <strong className="mono detail-value">{schema?.configPath ?? values?.configPath ?? "-"}</strong>
        </div>

        {error ? <span className="error-text">{error}</span> : null}

        {loading ? (
          <Card>
            <span className="helper-text">Loading managed runtime config...</span>
          </Card>
        ) : schema?.openFileOnly ? (
          <Card>
            <div className="runtime-config-empty">
              <strong>Structured editing is not available for this runtime yet.</strong>
              <span className="helper-text">
                DevNest currently supports opening the generated config file for review in your
                default Windows editor.
              </span>
            </div>
          </Card>
        ) : (
          <div className="runtime-config-sections">
            {schema?.sections.map((section) => (
              <Card key={section.id}>
                <div className="runtime-tools-section-copy">
                  <h3>{section.title}</h3>
                  {section.description ? (
                    <span className="helper-text">{section.description}</span>
                  ) : null}
                </div>

                <div className="form-grid runtime-config-grid">
                  {section.fields.map((field) => (
                    <label className="field runtime-config-field" key={field.key}>
                      <span className="field-label">{field.label}</span>
                      {renderFieldInput(
                        field,
                        draft[field.key] ?? "",
                        saving,
                        (nextValue) =>
                          setDraft((currentDraft) => ({
                            ...currentDraft,
                            [field.key]: nextValue,
                          })),
                      )}
                      {field.description ? (
                        <span className="helper-text">{field.description}</span>
                      ) : null}
                    </label>
                  ))}
                </div>
              </Card>
            ))}
          </div>
        )}

        <div className="confirm-dialog-actions runtime-config-actions">
          {!schema?.openFileOnly ? (
            <Button
              busy={saving}
              busyLabel="Saving config..."
              disabled={saving || loading || !dirty}
              onClick={() => void onSave(draft)}
              variant="primary"
            >
              Save Config
            </Button>
          ) : null}
        </div>
      </div>
    </div>
  );
}
