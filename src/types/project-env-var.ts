export interface ProjectEnvVar {
  id: string;
  projectId: string;
  envKey: string;
  envValue: string;
  createdAt: string;
  updatedAt: string;
}

export interface CreateProjectEnvVarInput {
  projectId: string;
  envKey: string;
  envValue: string;
}

export interface UpdateProjectEnvVarInput {
  projectId: string;
  envVarId: string;
  envKey: string;
  envValue: string;
}

export type ProjectEnvComparisonStatus =
  | "match"
  | "onlyTracked"
  | "onlyDisk"
  | "valueMismatch";

export interface ProjectDiskEnvVar {
  key: string;
  value: string;
  sourceLine: number;
}

export interface ProjectEnvComparisonItem {
  key: string;
  trackedValue?: string | null;
  diskValue?: string | null;
  status: ProjectEnvComparisonStatus;
}

export interface ProjectEnvInspection {
  projectId: string;
  envFilePath: string;
  envFileExists: boolean;
  diskReadError?: string | null;
  trackedCount: number;
  diskCount: number;
  diskVars: ProjectDiskEnvVar[];
  comparison: ProjectEnvComparisonItem[];
}
