import { useToastStore } from "@/app/store/toast-store";

export function ToastViewport() {
  const items = useToastStore((state) => state.items);
  const dismiss = useToastStore((state) => state.dismiss);

  if (items.length === 0) {
    return null;
  }

  return (
    <div aria-atomic="true" aria-live="polite" className="toast-viewport">
      {items.map((toast) => (
        <div className="toast-card" data-tone={toast.tone} key={toast.id} role="status">
          <div className="toast-copy">
            {toast.title ? <strong>{toast.title}</strong> : null}
            <span>{toast.message}</span>
          </div>
          <button
            aria-label="Dismiss notification"
            className="toast-dismiss"
            onClick={() => dismiss(toast.id)}
            type="button"
          >
            Close
          </button>
        </div>
      ))}
    </div>
  );
}
