export type ServerType = "apache" | "nginx" | "frankenphp";
export type FrameworkType = "laravel" | "wordpress" | "php" | "unknown";
export type ProjectStatus = "running" | "stopped" | "error";
export type FrankenphpMode = "classic" | "octane";

export interface Project {
  id: string;
  name: string;
  path: string;
  domain: string;
  serverType: ServerType;
  phpVersion: string;
  framework: FrameworkType;
  documentRoot: string;
  sslEnabled: boolean;
  databaseName?: string | null;
  databasePort?: number | null;
  status: ProjectStatus;
  frankenphpMode: FrankenphpMode;
  createdAt: string;
  updatedAt: string;
}

export interface CreateProjectInput {
  name: string;
  path: string;
  domain: string;
  serverType: ServerType;
  phpVersion: string;
  framework: FrameworkType;
  documentRoot: string;
  sslEnabled: boolean;
  databaseName?: string | null;
  databasePort?: number | null;
  frankenphpMode?: FrankenphpMode | null;
}

export interface UpdateProjectPatch {
  name?: string;
  domain?: string;
  serverType?: ServerType;
  phpVersion?: string;
  framework?: FrameworkType;
  documentRoot?: string;
  sslEnabled?: boolean;
  databaseName?: string | null;
  databasePort?: number | null;
  status?: ProjectStatus;
  frankenphpMode?: FrankenphpMode;
}

export interface ScanResult {
  framework: FrameworkType;
  recommendedServer: ServerType;
  serverReason?: string;
  recommendedPhpVersion?: string;
  suggestedDomain: string;
  documentRoot: string;
  documentRootReason?: string;
  detectedFiles: string[];
  warnings: string[];
  missingPhpExtensions: string[];
}
