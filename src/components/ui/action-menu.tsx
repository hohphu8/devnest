import {
  createContext,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type CSSProperties,
  type PropsWithChildren,
} from "react";
import { createPortal } from "react-dom";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";

const ActionMenuContext = createContext<{ closeMenu: () => void } | null>(null);

interface ActionMenuProps extends PropsWithChildren {
  className?: string;
  disabled?: boolean;
  label?: string;
}

interface ActionMenuItemProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  tone?: "default" | "danger";
}

export function ActionMenu({
  children,
  className,
  disabled = false,
  label = "Actions",
}: ActionMenuProps) {
  const [open, setOpen] = useState(false);
  const [panelStyle, setPanelStyle] = useState<CSSProperties>({
    visibility: "hidden",
  });
  const panelId = useId();
  const rootRef = useRef<HTMLDivElement | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const panelRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) {
      return;
    }

    function handlePointerDown(event: MouseEvent) {
      const target = event.target as Node;
      const clickedTrigger = rootRef.current?.contains(target);
      const clickedPanel = panelRef.current?.contains(target);

      if (!clickedTrigger && !clickedPanel) {
        setOpen(false);
      }
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setOpen(false);
      }
    }

    document.addEventListener("mousedown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);

    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  useEffect(() => {
    if (disabled) {
      setOpen(false);
    }
  }, [disabled]);

  useLayoutEffect(() => {
    if (!open) {
      return;
    }

    function updatePosition() {
      if (!triggerRef.current || !panelRef.current) {
        return;
      }

      const triggerRect = triggerRef.current.getBoundingClientRect();
      const panelRect = panelRef.current.getBoundingClientRect();
      const viewportWidth = window.innerWidth;
      const viewportHeight = window.innerHeight;
      const gap = 8;
      const margin = 12;
      const belowSpace = viewportHeight - triggerRect.bottom - margin;
      const aboveSpace = triggerRect.top - margin;
      const openUpward = belowSpace < panelRect.height + gap && aboveSpace > belowSpace;
      const top = openUpward
        ? Math.max(margin, triggerRect.top - panelRect.height - gap)
        : Math.min(viewportHeight - panelRect.height - margin, triggerRect.bottom + gap);
      const left = Math.min(
        Math.max(margin, triggerRect.right - panelRect.width),
        viewportWidth - panelRect.width - margin,
      );

      setPanelStyle({
        top,
        left,
        visibility: "visible",
      });
    }

    updatePosition();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);

    return () => {
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
    };
  }, [open]);

  return (
    <div className={cn("action-menu", className)} ref={rootRef}>
      <Button
        aria-controls={open ? panelId : undefined}
        aria-expanded={open}
        aria-haspopup="menu"
        disabled={disabled}
        onClick={() => setOpen((current) => !current)}
        ref={triggerRef}
        size="sm"
      >
        {label}
      </Button>
      {open && typeof document !== "undefined"
        ? createPortal(
            <ActionMenuContext.Provider value={{ closeMenu: () => setOpen(false) }}>
              <div
                className="action-menu-panel"
                id={panelId}
                ref={panelRef}
                role="menu"
                style={panelStyle}
              >
                {children}
              </div>
            </ActionMenuContext.Provider>,
            document.body,
          )
        : null}
    </div>
  );
}

export function ActionMenuItem({
  children,
  className,
  disabled,
  onClick,
  tone = "default",
  ...props
}: PropsWithChildren<ActionMenuItemProps>) {
  const context = useContext(ActionMenuContext);

  return (
    <button
      {...props}
      className={cn(
        "action-menu-item",
        tone === "danger" ? "action-menu-item-danger" : null,
        className,
      )}
      disabled={disabled}
      onClick={(event) => {
        onClick?.(event);

        if (!event.defaultPrevented && !disabled) {
          context?.closeMenu();
        }
      }}
      role="menuitem"
      type={props.type ?? "button"}
    >
      {children}
    </button>
  );
}
