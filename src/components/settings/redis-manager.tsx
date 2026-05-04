import { useEffect, useMemo, useState } from "react";
import { redisManagerApi } from "@/lib/api/redis-manager-api";
import { getAppErrorMessage } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import type {
  RedisConnectionOptions,
  RedisKeyListResult,
  RedisKeyMetadata,
  RedisKeyTypeFilter,
  RedisKeyValue,
  RedisManagerStatus,
} from "@/types/redis-manager";
import type { ServiceState } from "@/types/service";

interface RedisManagerProps {
  service?: Pick<ServiceState, "status">;
}

const REDIS_KEY_TYPES: RedisKeyTypeFilter[] = [
  "all",
  "string",
  "hash",
  "list",
  "set",
  "zset",
  "stream",
];

function formatTtl(ttl?: number | null): string {
  if (ttl === undefined || ttl === null) {
    return "No expiry";
  }
  return `${ttl}s`;
}

function formatSize(size?: number | null): string {
  if (size === undefined || size === null) {
    return "Unknown";
  }
  if (size < 1024) {
    return `${size} B`;
  }
  return `${(size / 1024).toFixed(1)} KB`;
}

function typeFilterLabel(typeFilter: RedisKeyTypeFilter): string {
  return typeFilter === "all" ? "all" : typeFilter;
}

function statusTone(status?: RedisManagerStatus | null): "success" | "warning" | "error" {
  if (!status) {
    return "warning";
  }
  return status.connected ? "success" : "error";
}

export function RedisManager({ service }: RedisManagerProps) {
  const [dbIndex, setDbIndex] = useState(0);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [status, setStatus] = useState<RedisManagerStatus | null>(null);
  const [keyResult, setKeyResult] = useState<RedisKeyListResult | null>(null);
  const [selectedKeys, setSelectedKeys] = useState<string[]>([]);
  const [selectedKey, setSelectedKey] = useState<RedisKeyValue | null>(null);
  const [keyDraft, setKeyDraft] = useState("");
  const [newKeyName, setNewKeyName] = useState("");
  const [newKeyValue, setNewKeyValue] = useState("");
  const [pattern, setPattern] = useState("*");
  const [typeFilter, setTypeFilter] = useState<RedisKeyTypeFilter>("all");
  const [cursor, setCursor] = useState("0");
  const [loading, setLoading] = useState(false);
  const [actionLoading, setActionLoading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [clearConfirmOpen, setClearConfirmOpen] = useState(false);
  const [clearConfirmation, setClearConfirmation] = useState("");

  const options = useMemo<RedisConnectionOptions>(
    () => ({
      dbIndex,
      username: username.trim() || null,
      password: password || null,
    }),
    [dbIndex, password, username],
  );
  const clearToken = `CLEAR DB ${dbIndex}`;
  const canScanMore = Boolean(keyResult && !keyResult.complete && cursor !== "0");
  const scanSummary = keyResult
    ? `${keyResult.keys.length} shown. ${
        keyResult.complete ? "SCAN cursor reached 0." : `Next cursor ${keyResult.nextCursor}.`
      }`
    : "Run Scan to browse keys.";

  async function refreshStatus(nextOptions = options) {
    const nextStatus = await redisManagerApi.status(nextOptions);
    setStatus(nextStatus);
    return nextStatus;
  }

  async function loadKeys(nextCursor = cursor, nextOptions = options) {
    const result = await redisManagerApi.listKeys({
      options: nextOptions,
      cursor: nextCursor,
      pattern,
      typeFilter,
      pageSize: 100,
    });
    setCursor(result.nextCursor);
    setKeyResult(result);
    setSelectedKeys([]);
    return result;
  }

  async function refreshRedisManager(nextCursor = "0") {
    setLoading(true);
    setError(null);
    try {
      const nextStatus = await refreshStatus();
      if (!nextStatus.connected) {
        setKeyResult(null);
        setSelectedKey(null);
        return;
      }
      await loadKeys(nextCursor);
    } catch (invokeError) {
      setError(getAppErrorMessage(invokeError, "Redis Manager could not refresh."));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void refreshRedisManager("0");
  }, [dbIndex, service?.status]);

  async function handleOpenKey(key: string) {
    setActionLoading(`key:${key}`);
    setError(null);
    try {
      const value = await redisManagerApi.getKey(key, options);
      setSelectedKey(value);
      setKeyDraft(value.value ?? "");
    } catch (invokeError) {
      setError(getAppErrorMessage(invokeError, "Could not read the selected Redis key."));
    } finally {
      setActionLoading(null);
    }
  }

  async function handleSaveSelectedKey() {
    if (!selectedKey?.editable) {
      return;
    }
    setActionLoading("save-key");
    setError(null);
    try {
      const saved = await redisManagerApi.setStringKey(selectedKey.key, keyDraft, options);
      setSelectedKey(saved);
      setKeyDraft(saved.value ?? "");
      await loadKeys("0");
    } catch (invokeError) {
      setError(getAppErrorMessage(invokeError, "Could not save the Redis string key."));
    } finally {
      setActionLoading(null);
    }
  }

  async function handleCreateStringKey() {
    setActionLoading("create-key");
    setError(null);
    try {
      const saved = await redisManagerApi.setStringKey(newKeyName, newKeyValue, options);
      setSelectedKey(saved);
      setKeyDraft(saved.value ?? "");
      setNewKeyName("");
      setNewKeyValue("");
      await loadKeys("0");
    } catch (invokeError) {
      setError(getAppErrorMessage(invokeError, "Could not create the Redis string key."));
    } finally {
      setActionLoading(null);
    }
  }

  async function handleDeleteSelectedKeys() {
    setActionLoading("delete-keys");
    setError(null);
    try {
      await redisManagerApi.deleteKeys(selectedKeys, options);
      setSelectedKey((current) =>
        current && selectedKeys.includes(current.key) ? null : current,
      );
      await loadKeys("0");
    } catch (invokeError) {
      setError(getAppErrorMessage(invokeError, "Could not delete the selected Redis keys."));
    } finally {
      setActionLoading(null);
    }
  }

  async function handleClearDatabase() {
    setActionLoading("clear-db");
    setError(null);
    try {
      await redisManagerApi.clearDatabase(clearConfirmation, options);
      setClearConfirmOpen(false);
      setClearConfirmation("");
      setSelectedKey(null);
      await loadKeys("0");
      await refreshStatus();
    } catch (invokeError) {
      setError(getAppErrorMessage(invokeError, "Could not clear the selected Redis DB."));
    } finally {
      setActionLoading(null);
    }
  }

  function toggleSelectedKey(key: string) {
    setSelectedKeys((current) =>
      current.includes(key) ? current.filter((item) => item !== key) : [...current, key],
    );
  }

  return (
    <Card className="redis-manager-card">
      <div className="page-header">
        <div>
          <h2>Redis Manager</h2>
          <p>Browse local Redis keys, edit string values, and clear the selected DB.</p>
        </div>
        <span className="status-chip" data-tone={statusTone(status)}>
          {status?.connected ? "connected" : "offline"}
        </span>
      </div>

      <div className="redis-manager-status">
        <div className="detail-item">
          <span className="detail-label">Target</span>
          <strong>{status ? `${status.host}:${status.port}` : "127.0.0.1:6379"}</strong>
        </div>
        <div className="detail-item">
          <span className="detail-label">DB</span>
          <strong>{dbIndex}</strong>
        </div>
        <div className="detail-item">
          <span className="detail-label">Version</span>
          <strong>{status?.redisVersion ?? "-"}</strong>
        </div>
        <div className="detail-item">
          <span className="detail-label">Keys</span>
          <strong>{status?.keyCount ?? "-"}</strong>
        </div>
        <div className="detail-item redis-manager-status-message">
          <span className="detail-label">Status</span>
          <strong>{status?.message ?? "Redis Manager has not connected yet."}</strong>
        </div>
      </div>

      <div className="redis-connection-bar">
        <label>
          <span className="detail-label">DB Index</span>
          <input
            className="input"
            min={0}
            max={15}
            onChange={(event) => setDbIndex(Number(event.target.value))}
            type="number"
            value={dbIndex}
          />
        </label>
        <label>
          <span className="detail-label">Username</span>
          <input
            className="input"
            onChange={(event) => setUsername(event.target.value)}
            placeholder="Optional"
            value={username}
          />
        </label>
        <label>
          <span className="detail-label">Password</span>
          <input
            className="input"
            onChange={(event) => setPassword(event.target.value)}
            placeholder="Optional"
            type="password"
            value={password}
          />
        </label>
        <Button
          busy={loading}
          busyLabel="Refreshing Redis..."
          onClick={() => void refreshRedisManager("0")}
          variant="primary"
        >
          Refresh
        </Button>
      </div>

      {error ? <span className="error-text">{error}</span> : null}

      <div className="redis-manager-panel">
        <div className="redis-manager-toolbar">
          <input
            className="input"
            onChange={(event) => setPattern(event.target.value)}
            placeholder="Pattern, for example app:*"
            value={pattern}
          />
          <select
            className="select"
            onChange={(event) => setTypeFilter(event.target.value as RedisKeyTypeFilter)}
            value={typeFilter}
          >
            {REDIS_KEY_TYPES.map((type) => (
              <option key={type} value={type}>
                {type === "all" ? "All types" : type}
              </option>
            ))}
          </select>
          <Button busy={loading} onClick={() => void loadKeys("0")} variant="primary">
            Scan
          </Button>
          <Button disabled={!canScanMore} onClick={() => void loadKeys(cursor)}>
            {canScanMore ? "Scan More" : "Scan Complete"}
          </Button>
          <span className="redis-scan-summary">{scanSummary}</span>
          <Button
            className="button-danger"
            disabled={selectedKeys.length === 0 || actionLoading === "delete-keys"}
            onClick={() => void handleDeleteSelectedKeys()}
          >
            Delete Selected
          </Button>
          <Button className="button-danger" onClick={() => setClearConfirmOpen(true)}>
            Clear DB
          </Button>
        </div>

        <div className="redis-key-workbench">
          <div className="runtime-table-shell redis-key-table-shell">
            <table className="runtime-table redis-key-table">
              <thead>
                <tr>
                  <th>Select</th>
                  <th>Key</th>
                  <th>Type</th>
                  <th>TTL</th>
                  <th>Size</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {keyResult?.keys.length ? (
                  keyResult.keys.map((item: RedisKeyMetadata) => (
                    <tr key={item.key}>
                      <td>
                        <input
                          aria-label={`Select ${item.key}`}
                          checked={selectedKeys.includes(item.key)}
                          onChange={() => toggleSelectedKey(item.key)}
                          type="checkbox"
                        />
                      </td>
                      <td>
                        <div className="runtime-table-type">
                          <strong className="mono">{item.key}</strong>
                        </div>
                      </td>
                      <td>{item.keyType}</td>
                      <td>{formatTtl(item.ttlSeconds)}</td>
                      <td>{formatSize(item.sizeBytes)}</td>
                      <td>
                        <Button
                          busy={actionLoading === `key:${item.key}`}
                          disabled={actionLoading !== null}
                          onClick={() => void handleOpenKey(item.key)}
                          size="sm"
                        >
                          Inspect
                        </Button>
                      </td>
                    </tr>
                  ))
                ) : (
                  <tr>
                    <td colSpan={6}>
                      <span className="helper-text">
                        {status?.connected
                          ? `No ${typeFilterLabel(typeFilter)} keys matched ${pattern.trim() || "*"}.`
                          : "Start Redis or update credentials before scanning keys."}
                      </span>
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>

          <aside className="redis-key-editor">
            <div className="redis-key-editor-section">
              <h3>String Key</h3>
              <input
                className="input"
                onChange={(event) => setNewKeyName(event.target.value)}
                placeholder="New key"
                value={newKeyName}
              />
              <textarea
                className="textarea"
                onChange={(event) => setNewKeyValue(event.target.value)}
                placeholder="String value"
                value={newKeyValue}
              />
              <Button
                busy={actionLoading === "create-key"}
                disabled={!newKeyName.trim()}
                onClick={() => void handleCreateStringKey()}
                variant="primary"
              >
                Set String
              </Button>
            </div>

            <div className="redis-key-editor-section">
              <h3>Selected Value</h3>
              {selectedKey ? (
                <>
                  <div className="detail-item">
                    <span className="detail-label">Key</span>
                    <strong className="mono detail-value">{selectedKey.key}</strong>
                  </div>
                  <div className="detail-grid">
                    <div className="detail-item">
                      <span className="detail-label">Type</span>
                      <strong>{selectedKey.keyType}</strong>
                    </div>
                    <div className="detail-item">
                      <span className="detail-label">TTL</span>
                      <strong>{formatTtl(selectedKey.ttlSeconds)}</strong>
                    </div>
                  </div>
                  {selectedKey.editable ? (
                    <>
                      <textarea
                        className="textarea redis-value-textarea"
                        onChange={(event) => setKeyDraft(event.target.value)}
                        value={keyDraft}
                      />
                      <Button
                        busy={actionLoading === "save-key"}
                        onClick={() => void handleSaveSelectedKey()}
                        variant="primary"
                      >
                        Save Value
                      </Button>
                    </>
                  ) : (
                    <span className="helper-text">
                      V1 edits string keys only. This key is metadata-only.
                    </span>
                  )}
                </>
              ) : (
                <span className="helper-text">Select a key to inspect its metadata and value.</span>
              )}
            </div>
          </aside>
        </div>
      </div>

      {clearConfirmOpen ? (
        <div
          className="wizard-overlay"
          data-nested-modal="true"
          onClick={() => {
            if (actionLoading !== "clear-db") {
              setClearConfirmOpen(false);
            }
          }}
          role="dialog"
          aria-modal="true"
        >
          <div className="confirm-dialog" onClick={(event) => event.stopPropagation()}>
            <div className="confirm-dialog-copy">
              <h3>Clear Redis DB {dbIndex}?</h3>
              <p>
                This runs <strong>FLUSHDB ASYNC</strong> only for DB {dbIndex}. Type{" "}
                <strong>{clearToken}</strong> to confirm.
              </p>
              <input
                className="input"
                onChange={(event) => setClearConfirmation(event.target.value)}
                placeholder={clearToken}
                value={clearConfirmation}
              />
            </div>
            <div className="confirm-dialog-actions">
              <Button onClick={() => setClearConfirmOpen(false)}>Cancel</Button>
              <Button
                className="button-danger"
                disabled={clearConfirmation !== clearToken || actionLoading === "clear-db"}
                onClick={() => void handleClearDatabase()}
              >
                Clear DB
              </Button>
            </div>
          </div>
        </div>
      ) : null}
    </Card>
  );
}
