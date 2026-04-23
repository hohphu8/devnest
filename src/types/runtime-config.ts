import type { RuntimeType } from "@/types/runtime";

export type RuntimeConfigFieldKind = "toggle" | "number" | "size" | "text" | "select";

export interface RuntimeConfigFieldOption {
  value: string;
  label: string;
}

export interface RuntimeConfigField {
  key: string;
  label: string;
  description?: string | null;
  kind: RuntimeConfigFieldKind;
  placeholder?: string | null;
  options: RuntimeConfigFieldOption[];
}

export interface RuntimeConfigSection {
  id: string;
  title: string;
  description?: string | null;
  fields: RuntimeConfigField[];
}

export interface RuntimeConfigSchema {
  runtimeId: string;
  runtimeType: RuntimeType;
  runtimeVersion: string;
  configPath: string;
  supportsEditor: boolean;
  openFileOnly: boolean;
  sections: RuntimeConfigSection[];
}

export interface RuntimeConfigValues {
  runtimeId: string;
  runtimeType: RuntimeType;
  runtimeVersion: string;
  configPath: string;
  values: Record<string, string>;
  updatedAt: string;
}
