import {
  forwardRef,
  type ButtonHTMLAttributes,
  type PropsWithChildren,
  type ReactNode,
} from "react";
import { cn } from "@/lib/utils";

type ButtonVariant = "primary" | "secondary" | "ghost";
type ButtonSize = "md" | "sm" | "icon";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  busy?: boolean;
  busyLabel?: string;
  spinner?: ReactNode;
}

export const Button = forwardRef<HTMLButtonElement, PropsWithChildren<ButtonProps>>(
  function Button(
    {
      busy = false,
      busyLabel,
      children,
      className,
      size = "md",
      spinner,
      variant = "secondary",
      ...props
    },
    ref,
  ) {
    const busyContent =
      size === "icon" && !busyLabel ? (
        <span aria-hidden="true" className="button-spinner" />
      ) : (
        <>
          {spinner ?? <span aria-hidden="true" className="button-spinner" />}
          {busyLabel ? <span className="button-copy">{busyLabel}</span> : null}
        </>
      );

    return (
      <button
        className={cn("button", className)}
        data-busy={busy}
        data-size={size}
        data-variant={variant}
        disabled={busy || props.disabled}
        ref={ref}
        type={props.type ?? "button"}
        {...props}
      >
        {busy ? busyContent : children}
      </button>
    );
  },
);
