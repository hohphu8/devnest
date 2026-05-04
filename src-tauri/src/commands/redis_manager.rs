use crate::error::AppError;
use crate::models::service::{ServiceName, ServiceStatus};
use crate::state::AppState;
use crate::storage::repositories::ServiceRepository;
use redis::{Commands, Value};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

const REDIS_HOST: &str = "127.0.0.1";
const DEFAULT_DB_INDEX: u8 = 0;
const CLEAR_DB_CONFIRMATION_PREFIX: &str = "CLEAR DB";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisConnectionOptions {
    pub db_index: Option<u8>,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisListKeysInput {
    pub options: Option<RedisConnectionOptions>,
    pub cursor: Option<String>,
    pub pattern: Option<String>,
    pub type_filter: Option<String>,
    pub page_size: Option<u16>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisKeyInput {
    pub options: Option<RedisConnectionOptions>,
    pub key: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisSetStringKeyInput {
    pub options: Option<RedisConnectionOptions>,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisDeleteKeysInput {
    pub options: Option<RedisConnectionOptions>,
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisClearDatabaseInput {
    pub options: Option<RedisConnectionOptions>,
    pub confirmation: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisManagerStatus {
    pub host: String,
    pub port: u16,
    pub db_index: u8,
    pub connected: bool,
    pub redis_version: Option<String>,
    pub key_count: Option<i64>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisKeyMetadata {
    pub key: String,
    pub key_type: String,
    pub ttl_seconds: Option<i64>,
    pub size_bytes: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisKeyListResult {
    pub cursor: String,
    pub next_cursor: String,
    pub complete: bool,
    pub db_index: u8,
    pub pattern: String,
    pub type_filter: Option<String>,
    pub keys: Vec<RedisKeyMetadata>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisKeyValue {
    pub key: String,
    pub key_type: String,
    pub ttl_seconds: Option<i64>,
    pub size_bytes: Option<i64>,
    pub value: Option<String>,
    pub editable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisDeleteKeysResult {
    pub success: bool,
    pub deleted: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisClearDatabaseResult {
    pub success: bool,
    pub db_index: u8,
}

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

fn redis_target(connection: &Connection) -> Result<(u16, bool, String), AppError> {
    let service = ServiceRepository::get(connection, ServiceName::Redis.as_str())?;
    let port = service
        .port
        .unwrap_or_else(|| i64::from(ServiceName::Redis.default_port().unwrap_or(6379)));
    if !(1..=65535).contains(&port) {
        return Err(AppError::new_validation(
            "REDIS_PORT_INVALID",
            "The active Redis service port is invalid.",
        ));
    }

    Ok((
        port as u16,
        service.status == ServiceStatus::Running,
        service
            .last_error
            .unwrap_or_else(|| "Redis is not currently running.".to_string()),
    ))
}

fn normalized_options(options: Option<RedisConnectionOptions>) -> RedisConnectionOptions {
    options.unwrap_or(RedisConnectionOptions {
        db_index: Some(DEFAULT_DB_INDEX),
        username: None,
        password: None,
    })
}

fn db_index(options: &RedisConnectionOptions) -> u8 {
    options.db_index.unwrap_or(DEFAULT_DB_INDEX)
}

fn percent_encode_redis_url_part(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        let keep = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        if keep {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn redis_url(port: u16, options: &RedisConnectionOptions) -> String {
    let db = db_index(options);
    let username = options.username.as_deref().unwrap_or_default().trim();
    let password = options.password.as_deref().unwrap_or_default();

    if !username.is_empty() && !password.is_empty() {
        return format!(
            "redis://{}:{}@{REDIS_HOST}:{port}/{db}",
            percent_encode_redis_url_part(username),
            percent_encode_redis_url_part(password)
        );
    }

    if !password.is_empty() {
        return format!(
            "redis://:{}@{REDIS_HOST}:{port}/{db}",
            percent_encode_redis_url_part(password)
        );
    }

    format!("redis://{REDIS_HOST}:{port}/{db}")
}

fn map_redis_error(code: &str, message: &str, error: redis::RedisError) -> AppError {
    let raw = error.to_string().to_ascii_lowercase();
    let details = if raw.contains("noauth")
        || raw.contains("wrongpass")
        || raw.contains("auth")
        || raw.contains("invalid username")
    {
        "Redis rejected the supplied credentials."
    } else if raw.contains("permission") || raw.contains("noperm") {
        "The selected Redis credentials cannot run this manager operation."
    } else if raw.contains("refused") || raw.contains("timed out") || raw.contains("reset") {
        "DevNest could not reach the local Redis service."
    } else {
        "Redis did not complete the requested manager operation."
    };

    AppError::with_details(code, message, details)
}

fn should_retry_flushdb_without_async(error: &redis::RedisError) -> bool {
    let raw = error.to_string().to_ascii_lowercase();
    raw.contains("syntax")
        || raw.contains("unknown")
        || raw.contains("wrong number")
        || raw.contains("wrong arity")
}

fn open_redis_connection(
    connection: &Connection,
    options: Option<RedisConnectionOptions>,
) -> Result<(redis::Connection, u16, RedisConnectionOptions), AppError> {
    let (port, running, last_error) = redis_target(connection)?;
    if !running {
        return Err(AppError::with_details(
            "REDIS_SERVICE_STOPPED",
            "Start Redis before opening Redis Manager.",
            last_error,
        ));
    }

    let options = normalized_options(options);
    let client = redis::Client::open(redis_url(port, &options)).map_err(|error| {
        map_redis_error(
            "REDIS_CONNECTION_FAILED",
            "DevNest could not prepare the local Redis connection.",
            error,
        )
    })?;
    let redis_connection = client.get_connection().map_err(|error| {
        map_redis_error(
            "REDIS_CONNECTION_FAILED",
            "DevNest could not connect to local Redis.",
            error,
        )
    })?;

    Ok((redis_connection, port, options))
}

fn validate_key(key: &str) -> Result<String, AppError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(AppError::new_validation(
            "REDIS_KEY_INVALID",
            "Redis key is required.",
        ));
    }
    Ok(trimmed.to_string())
}

fn confirmation_token(db_index: u8) -> String {
    format!("{CLEAR_DB_CONFIRMATION_PREFIX} {db_index}")
}

fn ttl_for_metadata(ttl: i64) -> Option<i64> {
    if ttl < 0 { None } else { Some(ttl) }
}

fn redis_type(redis_connection: &mut redis::Connection, key: &str) -> Result<String, AppError> {
    redis::cmd("TYPE")
        .arg(key)
        .query::<String>(redis_connection)
        .map_err(|error| {
            map_redis_error(
                "REDIS_KEY_LOOKUP_FAILED",
                "DevNest could not inspect the selected Redis key.",
                error,
            )
        })
}

fn redis_ttl(redis_connection: &mut redis::Connection, key: &str) -> Option<i64> {
    redis::cmd("TTL")
        .arg(key)
        .query::<i64>(redis_connection)
        .ok()
}

fn redis_size(redis_connection: &mut redis::Connection, key: &str, key_type: &str) -> Option<i64> {
    if key_type == "string" {
        if let Ok(size) = redis::cmd("STRLEN").arg(key).query::<i64>(redis_connection) {
            return Some(size);
        }
    }
    redis::cmd("MEMORY")
        .arg("USAGE")
        .arg(key)
        .query::<Option<i64>>(redis_connection)
        .ok()
        .flatten()
}

fn key_metadata(
    redis_connection: &mut redis::Connection,
    key: String,
) -> Result<RedisKeyMetadata, AppError> {
    let key_type = redis_type(redis_connection, &key)?;
    let ttl_seconds = redis_ttl(redis_connection, &key).and_then(ttl_for_metadata);
    let size_bytes = redis_size(redis_connection, &key, &key_type);
    Ok(RedisKeyMetadata {
        key,
        key_type,
        ttl_seconds,
        size_bytes,
    })
}

fn info_value(info: &str, key: &str) -> Option<String> {
    info.lines().find_map(|line| {
        let (left, right) = line.split_once(':')?;
        (left == key).then(|| right.trim().to_string())
    })
}

#[tauri::command]
pub fn get_redis_manager_status(
    options: Option<RedisConnectionOptions>,
    state: tauri::State<'_, AppState>,
) -> Result<RedisManagerStatus, AppError> {
    let connection = connection_from_state(&state)?;
    let (port, running, last_error) = redis_target(&connection)?;
    let options = normalized_options(options);
    let db = db_index(&options);
    if !running {
        return Ok(RedisManagerStatus {
            host: REDIS_HOST.to_string(),
            port,
            db_index: db,
            connected: false,
            redis_version: None,
            key_count: None,
            message: format!("Start Redis before opening Redis Manager. {last_error}"),
        });
    }

    match open_redis_connection(&connection, Some(options.clone())) {
        Ok((mut redis_connection, _, _)) => {
            let info = redis::cmd("INFO")
                .arg("server")
                .query::<String>(&mut redis_connection)
                .unwrap_or_default();
            let key_count = redis::cmd("DBSIZE")
                .query::<i64>(&mut redis_connection)
                .ok();
            Ok(RedisManagerStatus {
                host: REDIS_HOST.to_string(),
                port,
                db_index: db,
                connected: true,
                redis_version: info_value(&info, "redis_version"),
                key_count,
                message: "Connected to local Redis.".to_string(),
            })
        }
        Err(error) => Ok(RedisManagerStatus {
            host: REDIS_HOST.to_string(),
            port,
            db_index: db,
            connected: false,
            redis_version: None,
            key_count: None,
            message: error.message,
        }),
    }
}

#[tauri::command]
pub fn list_redis_keys(
    input: RedisListKeysInput,
    state: tauri::State<'_, AppState>,
) -> Result<RedisKeyListResult, AppError> {
    let connection = connection_from_state(&state)?;
    let (mut redis_connection, _, options) = open_redis_connection(&connection, input.options)?;
    let cursor = input
        .cursor
        .as_deref()
        .unwrap_or("0")
        .trim()
        .parse::<u64>()
        .map_err(|_| {
            AppError::new_validation("REDIS_SCAN_CURSOR_INVALID", "Redis cursor is invalid.")
        })?;
    let pattern = input
        .pattern
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("*")
        .to_string();
    let page_size = input.page_size.unwrap_or(100).clamp(10, 500);
    let type_filter = input
        .type_filter
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "all")
        .map(ToOwned::to_owned);

    let mut next_cursor = cursor;
    let mut metadata = Vec::new();
    let mut scan_count = 0;

    loop {
        scan_count += 1;
        let (scan_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(next_cursor)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(page_size)
            .query(&mut redis_connection)
            .map_err(|error| {
                map_redis_error(
                    "REDIS_SCAN_FAILED",
                    "DevNest could not scan Redis keys.",
                    error,
                )
            })?;
        next_cursor = scan_cursor;

        for key in keys {
            if let Ok(item) = key_metadata(&mut redis_connection, key) {
                if type_filter
                    .as_deref()
                    .map(|filter| item.key_type == filter)
                    .unwrap_or(true)
                {
                    metadata.push(item);
                }
            }
            if metadata.len() >= usize::from(page_size) {
                break;
            }
        }

        if next_cursor == 0 || metadata.len() >= usize::from(page_size) || scan_count >= 20 {
            break;
        }
    }

    Ok(RedisKeyListResult {
        cursor: cursor.to_string(),
        next_cursor: next_cursor.to_string(),
        complete: next_cursor == 0,
        db_index: db_index(&options),
        pattern,
        type_filter,
        keys: metadata,
    })
}

#[tauri::command]
pub fn get_redis_key(
    input: RedisKeyInput,
    state: tauri::State<'_, AppState>,
) -> Result<RedisKeyValue, AppError> {
    let key = validate_key(&input.key)?;
    let connection = connection_from_state(&state)?;
    let (mut redis_connection, _, _) = open_redis_connection(&connection, input.options)?;
    let metadata = key_metadata(&mut redis_connection, key.clone())?;
    if metadata.key_type != "string" {
        return Ok(RedisKeyValue {
            key,
            key_type: metadata.key_type,
            ttl_seconds: metadata.ttl_seconds,
            size_bytes: metadata.size_bytes,
            value: None,
            editable: false,
        });
    }

    let value = redis_connection
        .get::<_, Option<Vec<u8>>>(&key)
        .map_err(|error| {
            map_redis_error(
                "REDIS_KEY_READ_FAILED",
                "DevNest could not read the Redis string key.",
                error,
            )
        })?;
    Ok(RedisKeyValue {
        key,
        key_type: metadata.key_type,
        ttl_seconds: metadata.ttl_seconds,
        size_bytes: metadata.size_bytes,
        value: value.map(|bytes| String::from_utf8_lossy(&bytes).to_string()),
        editable: true,
    })
}

#[tauri::command]
pub fn set_redis_string_key(
    input: RedisSetStringKeyInput,
    state: tauri::State<'_, AppState>,
) -> Result<RedisKeyValue, AppError> {
    let key = validate_key(&input.key)?;
    let connection = connection_from_state(&state)?;
    let (mut redis_connection, _, options) = open_redis_connection(&connection, input.options)?;
    redis::cmd("SET")
        .arg(&key)
        .arg(&input.value)
        .query::<Value>(&mut redis_connection)
        .map_err(|error| {
            map_redis_error(
                "REDIS_KEY_WRITE_FAILED",
                "DevNest could not save the Redis string key.",
                error,
            )
        })?;
    get_redis_key(
        RedisKeyInput {
            options: Some(options),
            key,
        },
        state,
    )
}

#[tauri::command]
pub fn delete_redis_keys(
    input: RedisDeleteKeysInput,
    state: tauri::State<'_, AppState>,
) -> Result<RedisDeleteKeysResult, AppError> {
    let keys = input
        .keys
        .into_iter()
        .map(|key| validate_key(&key))
        .collect::<Result<Vec<_>, _>>()?;
    if keys.is_empty() {
        return Err(AppError::new_validation(
            "REDIS_KEYS_REQUIRED",
            "Select at least one Redis key to delete.",
        ));
    }
    let connection = connection_from_state(&state)?;
    let (mut redis_connection, _, _) = open_redis_connection(&connection, input.options)?;
    let deleted = match redis::cmd("UNLINK")
        .arg(&keys)
        .query::<i64>(&mut redis_connection)
    {
        Ok(count) => count,
        Err(_) => redis::cmd("DEL")
            .arg(&keys)
            .query::<i64>(&mut redis_connection)
            .map_err(|error| {
                map_redis_error(
                    "REDIS_KEY_DELETE_FAILED",
                    "DevNest could not delete the selected Redis keys.",
                    error,
                )
            })?,
    };

    Ok(RedisDeleteKeysResult {
        success: true,
        deleted,
    })
}

#[tauri::command]
pub fn clear_redis_database(
    input: RedisClearDatabaseInput,
    state: tauri::State<'_, AppState>,
) -> Result<RedisClearDatabaseResult, AppError> {
    let options = normalized_options(input.options);
    let db = db_index(&options);
    let expected = confirmation_token(db);
    if input.confirmation.trim() != expected {
        return Err(AppError::new_validation(
            "REDIS_CLEAR_CONFIRMATION_REQUIRED",
            format!("Type `{expected}` to clear Redis DB {db}."),
        ));
    }
    let connection = connection_from_state(&state)?;
    let (mut redis_connection, _, _) = open_redis_connection(&connection, Some(options))?;
    let async_result = redis::cmd("FLUSHDB")
        .arg("ASYNC")
        .query::<()>(&mut redis_connection);

    if let Err(error) = async_result {
        if should_retry_flushdb_without_async(&error) {
            redis::cmd("FLUSHDB")
                .query::<()>(&mut redis_connection)
                .map_err(|fallback_error| {
                    map_redis_error(
                        "REDIS_CLEAR_FAILED",
                        "DevNest could not clear the selected Redis database.",
                        fallback_error,
                    )
                })?;
        } else {
            return Err(map_redis_error(
                "REDIS_CLEAR_FAILED",
                "DevNest could not clear the selected Redis database.",
                error,
            ));
        }
    }

    Ok(RedisClearDatabaseResult {
        success: true,
        db_index: db,
    })
}

#[cfg(test)]
mod tests {
    use super::{confirmation_token, percent_encode_redis_url_part};

    #[test]
    fn clear_db_confirmation_is_db_scoped() {
        assert_eq!(confirmation_token(0), "CLEAR DB 0");
        assert_eq!(confirmation_token(7), "CLEAR DB 7");
    }

    #[test]
    fn redis_url_parts_are_percent_encoded() {
        assert_eq!(percent_encode_redis_url_part("dev user"), "dev%20user");
        assert_eq!(percent_encode_redis_url_part("p@ss:word"), "p%40ss%3Aword");
    }
}
