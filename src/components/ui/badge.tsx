import type { HTMLAttributes, PropsWithChildren } from "react";
import { cn } from "@/lib/utils";

export function Badge({
  children,
  className,
  ...props
}: PropsWithChildren<HTMLAttributes<HTMLSpanElement>>) {
  return (
    <span className={cn("badge", className)} {...props}>
      {children}
    </span>
  );
}
