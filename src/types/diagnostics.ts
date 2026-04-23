export type DiagnosticLevel = "info" | "warning" | "error";

export interface DiagnosticItem {
  id: string;
  projectId: string;
  level: DiagnosticLevel;
  code: string;
  title: string;
  message: string;
  suggestion?: string;
  createdAt: string;
}

export interface DiagnosticFixResult {
  success: true;
  code: string;
  message: string;
}
