export interface RedisConnectionOptions {
  dbIndex?: number;
  username?: string | null;
  password?: string | null;
}

export type RedisKeyTypeFilter =
  | "all"
  | "string"
  | "hash"
  | "list"
  | "set"
  | "zset"
  | "stream";

export interface RedisManagerStatus {
  host: string;
  port: number;
  dbIndex: number;
  connected: boolean;
  redisVersion?: string | null;
  keyCount?: number | null;
  message: string;
}

export interface RedisKeyMetadata {
  key: string;
  keyType: string;
  ttlSeconds?: number | null;
  sizeBytes?: number | null;
}

export interface RedisListKeysInput {
  options?: RedisConnectionOptions;
  cursor?: string;
  pattern?: string;
  typeFilter?: RedisKeyTypeFilter;
  pageSize?: number;
}

export interface RedisKeyListResult {
  cursor: string;
  nextCursor: string;
  complete: boolean;
  dbIndex: number;
  pattern: string;
  typeFilter?: string | null;
  keys: RedisKeyMetadata[];
}

export interface RedisKeyValue extends RedisKeyMetadata {
  value?: string | null;
  editable: boolean;
}

export interface RedisDeleteKeysResult {
  success: true;
  deleted: number;
}

export interface RedisClearDatabaseResult {
  success: true;
  dbIndex: number;
}
