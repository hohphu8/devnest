import type { HTMLAttributes, PropsWithChildren } from "react";
import { cn } from "@/lib/utils";

export function Card({
  children,
  className,
  ...props
}: PropsWithChildren<HTMLAttributes<HTMLElement>>) {
  return (
    <section className={cn("surface-card", className)} {...props}>
      {children}
    </section>
  );
}
