export interface FrankenphpProductionExportFile {
  relativePath: string;
  kind: string;
  content: string;
}

export interface FrankenphpProductionExportPreview {
  projectId: string;
  projectName: string;
  slug: string;
  generatedAt: string;
  assumptions: string[];
  warnings: string[];
  files: FrankenphpProductionExportFile[];
}

export interface FrankenphpProductionExportWriteResult {
  success: true;
  path: string;
  warnings: string[];
  files: string[];
}
