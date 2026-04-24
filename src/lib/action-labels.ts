import type { ServiceName } from "@/types/service";

function serviceLabel(name: ServiceName): string {
  switch (name) {
    case "apache":
      return "Apache";
    case "nginx":
      return "Nginx";
    case "frankenphp":
      return "FrankenPHP";
    case "mysql":
      return "MySQL";
    case "mailpit":
      return "Mailpit";
    case "redis":
      return "Redis";
  }
}

export function serviceActionLabel(
  action: "start" | "stop" | "restart",
  name: ServiceName,
): string {
  const label = serviceLabel(name);

  switch (action) {
    case "start":
      return `Starting ${label}...`;
    case "stop":
      return `Stopping ${label}...`;
    case "restart":
      return `Restarting ${label}...`;
  }
}
