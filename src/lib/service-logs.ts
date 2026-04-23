import type { ServiceLogPayload } from "@/types/service";

export function formatServiceLogPreview(payload: ServiceLogPayload, maxLines = 40): string {
  return payload.lines.slice(-Math.max(1, maxLines)).map((line) => line.text).join("\n");
}
