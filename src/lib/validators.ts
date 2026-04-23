import { z } from "zod";

export const projectNameSchema = z.string().trim().min(2).max(80);

export const projectPathSchema = z.string().trim().min(1, "Project path is required.");
export const recipeTargetPathSchema = z.string().trim().min(3, "Recipe target path is required.");

export const domainSchema = z
  .string()
  .trim()
  .min(3)
  .max(120)
  .regex(/^[a-z0-9-]+(\.[a-z0-9-]+)+$/i, "Invalid local domain");

export const databaseNameSchema = z
  .string()
  .trim()
  .min(1, "Database name is required.")
  .max(64, "Database name must stay under 64 characters.")
  .regex(/^[a-zA-Z0-9_-]+$/, "Use only letters, numbers, underscores, and dashes.");

export const envVarKeySchema = z
  .string()
  .trim()
  .min(1, "Environment key is required.")
  .max(64, "Environment key must stay under 64 characters.")
  .regex(/^[A-Za-z][A-Za-z0-9_]*$/, "Use letters, numbers, and underscores, starting with a letter.");

export const envVarValueSchema = z
  .string()
  .max(4000, "Environment value must stay under 4000 characters.");

export const gitRepositoryUrlSchema = z
  .string()
  .trim()
  .min(1, "Repository URL is required.")
  .regex(
    /^(https?:\/\/|git@|ssh:\/\/|file:\/\/|[A-Za-z]:[\\/]).+/i,
    "Use a valid Git repository URL or local path.",
  );

export const workerCommandLineSchema = z
  .string()
  .trim()
  .min(1, "Worker command line is required.")
  .max(500, "Worker command line must stay under 500 characters.");

export const workerWorkingDirectorySchema = z
  .string()
  .trim()
  .min(1, "Working directory is required.")
  .max(260, "Working directory must stay under 260 characters.");

export const scheduledTaskCommandLineSchema = z
  .string()
  .trim()
  .min(1, "Task command line is required.")
  .max(500, "Task command line must stay under 500 characters.");

export const scheduledTaskWorkingDirectorySchema = z
  .string()
  .trim()
  .min(1, "Working directory is required.")
  .max(260, "Working directory must stay under 260 characters.");

export const scheduledTaskUrlSchema = z
  .string()
  .trim()
  .min(1, "Task URL is required.")
  .url("Use a valid http:// or https:// URL.")
  .refine((value) => /^https?:\/\//i.test(value), "Use a valid http:// or https:// URL.");

export const scheduledTaskCronSchema = z
  .string()
  .trim()
  .refine(
    (value) => value.split(/\s+/).filter(Boolean).length === 5,
    "Cron expression must contain five fields.",
  );

export const scheduledTaskDailyTimeSchema = z
  .string()
  .trim()
  .regex(/^([01]\d|2[0-3]):([0-5]\d)$/, "Use HH:MM in 24-hour time.");

export const scheduledTaskIntervalSecondsSchema = z
  .number()
  .int("Interval must be a whole number.")
  .min(5, "Every X seconds must be at least 5 seconds.");

function isValidDocumentRoot(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed) {
    return false;
  }

  if (/^[a-z]:/i.test(trimmed) || trimmed.startsWith("/") || trimmed.startsWith("\\")) {
    return false;
  }

  const normalized = trimmed.replace(/\\/g, "/");
  const segments = normalized.split("/").filter(Boolean);

  return segments.every((segment) => segment !== "..");
}

export const documentRootSchema = z
  .string()
  .trim()
  .min(1, "Document root is required.")
  .refine(isValidDocumentRoot, "Document root must stay inside the project path.");
