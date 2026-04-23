export function cn(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}

function shouldQuoteCommandArg(value: string): boolean {
  return value.length === 0 || /[\s"\\]/.test(value);
}

function quoteCommandArg(value: string): string {
  if (!shouldQuoteCommandArg(value)) {
    return value;
  }

  return `"${value.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
}

export function serializeCommandLine(
  command: string | null | undefined,
  args: string[] = [],
): string {
  const base = (command ?? "").trim();
  if (!base) {
    return args.map(quoteCommandArg).join(" ");
  }

  if (args.length === 0) {
    return base;
  }

  return [base, ...args.map(quoteCommandArg)].join(" ");
}

export function formatUpdatedAt(value: string): string {
  const normalizedValue = /^\d+$/.test(value) ? Number(value) * 1000 : value;
  return new Intl.DateTimeFormat("en", {
    hour: "2-digit",
    minute: "2-digit",
    month: "short",
    day: "2-digit",
  }).format(new Date(normalizedValue));
}

export function formatUpdatedAtWithSeconds(value: string): string {
  const normalizedValue = /^\d+$/.test(value) ? Number(value) * 1000 : value;
  return new Intl.DateTimeFormat("en", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    month: "short",
    day: "2-digit",
  }).format(new Date(normalizedValue));
}
