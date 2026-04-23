import type { SVGProps } from "react";
import { cn } from "@/lib/utils";

export type IconName =
  | "activity"
  | "alert"
  | "check"
  | "clock"
  | "database"
  | "diagnostics"
  | "eye"
  | "folder"
  | "folderOpen"
  | "home"
  | "logs"
  | "play"
  | "plus"
  | "recipes"
  | "refresh"
  | "reliability"
  | "search"
  | "server"
  | "settings"
  | "sidebar"
  | "stop";

interface IconProps extends Omit<SVGProps<SVGSVGElement>, "children"> {
  name: IconName;
}

function BaseIcon({
  children,
  className,
  ...props
}: SVGProps<SVGSVGElement>) {
  return (
    <svg
      aria-hidden="true"
      className={cn("icon", className)}
      fill="none"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={1.8}
      viewBox="0 0 24 24"
      {...props}
    >
      {children}
    </svg>
  );
}

export function Icon({ name, ...props }: IconProps) {
  switch (name) {
    case "activity":
      return (
        <BaseIcon {...props}>
          <path d="M4 12h4l2.5-5 3 10L16 12h4" />
        </BaseIcon>
      );
    case "alert":
      return (
        <BaseIcon {...props}>
          <path d="M12 3 3.5 18a1 1 0 0 0 .87 1.5h15.26a1 1 0 0 0 .87-1.5L12 3Z" />
          <path d="M12 9v4" />
          <path d="M12 17h.01" />
        </BaseIcon>
      );
    case "check":
      return (
        <BaseIcon {...props}>
          <path d="M20 6 9 17l-5-5" />
        </BaseIcon>
      );
    case "clock":
      return (
        <BaseIcon {...props}>
          <circle cx="12" cy="12" r="8.5" />
          <path d="M12 7.5v5l3 2" />
        </BaseIcon>
      );
    case "database":
      return (
        <BaseIcon {...props}>
          <ellipse cx="12" cy="6" rx="7" ry="3" />
          <path d="M5 6v6c0 1.66 3.13 3 7 3s7-1.34 7-3V6" />
          <path d="M5 12v6c0 1.66 3.13 3 7 3s7-1.34 7-3v-6" />
        </BaseIcon>
      );
    case "diagnostics":
      return (
        <BaseIcon {...props}>
          <path d="M12 3v3" />
          <path d="m16.95 6.05-2.12 2.12" />
          <path d="M21 12h-3" />
          <path d="m16.95 17.95-2.12-2.12" />
          <path d="M12 21v-3" />
          <path d="m9.17 15.05-.7-2.12-2.12-.7 2.12-.7.7-2.12.7 2.12 2.12.7-2.12.7-.7 2.12Z" />
        </BaseIcon>
      );
    case "eye":
      return (
        <BaseIcon {...props}>
          <path d="M2.5 12s3.5-6 9.5-6 9.5 6 9.5 6-3.5 6-9.5 6-9.5-6-9.5-6Z" />
          <circle cx="12" cy="12" r="3" />
        </BaseIcon>
      );
    case "folder":
      return (
        <BaseIcon {...props}>
          <path d="M3 7.5A2.5 2.5 0 0 1 5.5 5H10l2 2h6.5A2.5 2.5 0 0 1 21 9.5v8A2.5 2.5 0 0 1 18.5 20h-13A2.5 2.5 0 0 1 3 17.5v-10Z" />
        </BaseIcon>
      );
    case "folderOpen":
      return (
        <BaseIcon {...props}>
          <path d="M3 7.5A2.5 2.5 0 0 1 5.5 5H10l2 2h6.5A2.5 2.5 0 0 1 21 9.5V11" />
          <path d="M3 11.5h18l-2.2 6.62A2.5 2.5 0 0 1 16.43 20H5.57a2.5 2.5 0 0 1-2.37-1.88L3 11.5Z" />
        </BaseIcon>
      );
    case "home":
      return (
        <BaseIcon {...props}>
          <path d="m4 10 8-6 8 6" />
          <path d="M6 9.5V20h12V9.5" />
        </BaseIcon>
      );
    case "logs":
      return (
        <BaseIcon {...props}>
          <path d="M5 6h14" />
          <path d="M5 12h14" />
          <path d="M5 18h9" />
          <path d="M3 6h.01" />
          <path d="M3 12h.01" />
          <path d="M3 18h.01" />
        </BaseIcon>
      );
    case "play":
      return (
        <BaseIcon {...props}>
          <path d="m9 7 8 5-8 5V7Z" />
        </BaseIcon>
      );
    case "plus":
      return (
        <BaseIcon {...props}>
          <path d="M12 5v14" />
          <path d="M5 12h14" />
        </BaseIcon>
      );
    case "recipes":
      return (
        <BaseIcon {...props}>
          <path d="m12 3 2.7 5.48L21 9.38l-4.5 4.38 1.06 6.2L12 17.3 6.44 19.96l1.06-6.2L3 9.38l6.3-.9L12 3Z" />
        </BaseIcon>
      );
    case "refresh":
      return (
        <BaseIcon {...props}>
          <path d="M20 11a8 8 0 0 0-14.9-4" />
          <path d="M4 4v4h4" />
          <path d="M4 13a8 8 0 0 0 14.9 4" />
          <path d="M20 20v-4h-4" />
        </BaseIcon>
      );
    case "reliability":
      return (
        <BaseIcon {...props}>
          <path d="M12 3 5 6v5c0 5 3.4 8.93 7 10 3.6-1.07 7-5 7-10V6l-7-3Z" />
          <path d="M9.5 12.5 11 14l3.5-4" />
        </BaseIcon>
      );
    case "search":
      return (
        <BaseIcon {...props}>
          <circle cx="11" cy="11" r="6" />
          <path d="m20 20-4.2-4.2" />
        </BaseIcon>
      );
    case "server":
      return (
        <BaseIcon {...props}>
          <rect x="4" y="4" width="16" height="6" rx="2" />
          <rect x="4" y="14" width="16" height="6" rx="2" />
          <path d="M8 7h.01" />
          <path d="M8 17h.01" />
          <path d="M16 7h2" />
          <path d="M16 17h2" />
        </BaseIcon>
      );
    case "settings":
      return (
        <BaseIcon {...props}>
          <circle cx="12" cy="12" r="3.25" />
          <path d="M12 2.75v2.1" />
          <path d="M12 19.15v2.1" />
          <path d="m5.46 5.46 1.48 1.48" />
          <path d="m17.06 17.06 1.48 1.48" />
          <path d="M2.75 12h2.1" />
          <path d="M19.15 12h2.1" />
          <path d="m5.46 18.54 1.48-1.48" />
          <path d="m17.06 6.94 1.48-1.48" />
        </BaseIcon>
      );
    case "sidebar":
      return (
        <BaseIcon {...props}>
          <rect x="4" y="4.5" width="16" height="15" rx="2" />
          <path d="M9 4.5v15" />
          <path d="M12.5 9.5 10 12l2.5 2.5" />
        </BaseIcon>
      );
    case "stop":
      return (
        <BaseIcon {...props}>
          <rect x="6.5" y="6.5" width="11" height="11" rx="2" />
        </BaseIcon>
      );
  }
}
