import { tauriInvoke } from "@/lib/tauri";
import type {
  RedisClearDatabaseResult,
  RedisConnectionOptions,
  RedisDeleteKeysResult,
  RedisKeyListResult,
  RedisKeyValue,
  RedisListKeysInput,
  RedisManagerStatus,
} from "@/types/redis-manager";

export const redisManagerApi = {
  status: (options?: RedisConnectionOptions) =>
    tauriInvoke<RedisManagerStatus>("get_redis_manager_status", { options }),
  listKeys: (input: RedisListKeysInput) =>
    tauriInvoke<RedisKeyListResult>("list_redis_keys", { input }),
  getKey: (key: string, options?: RedisConnectionOptions) =>
    tauriInvoke<RedisKeyValue>("get_redis_key", { input: { key, options } }),
  setStringKey: (key: string, value: string, options?: RedisConnectionOptions) =>
    tauriInvoke<RedisKeyValue>("set_redis_string_key", { input: { key, value, options } }),
  deleteKeys: (keys: string[], options?: RedisConnectionOptions) =>
    tauriInvoke<RedisDeleteKeysResult>("delete_redis_keys", { input: { keys, options } }),
  clearDatabase: (confirmation: string, options?: RedisConnectionOptions) =>
    tauriInvoke<RedisClearDatabaseResult>("clear_redis_database", {
      input: { confirmation, options },
    }),
};
