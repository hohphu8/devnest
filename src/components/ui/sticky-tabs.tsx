import { useRef } from "react";

export interface StickyTabItem<T extends string = string> {
  id: T;
  label: string;
  meta?: string;
}

interface StickyTabsProps<T extends string> {
  activeTab: T;
  ariaLabel: string;
  items: ReadonlyArray<StickyTabItem<T>>;
  onSelect: (tab: T) => void;
  namespace?: string;
}

export function StickyTabs<T extends string>({
  activeTab,
  ariaLabel,
  items,
  onSelect,
  namespace = "workspace",
}: StickyTabsProps<T>) {
  const tabRefs = useRef<Array<HTMLButtonElement | null>>([]);

  function focusTab(index: number) {
    const nextButton = tabRefs.current[index];
    if (nextButton) {
      nextButton.focus();
      onSelect(items[index]!.id);
    }
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLButtonElement>, index: number) {
    if (items.length === 0) {
      return;
    }

    if (event.key === "ArrowRight" || event.key === "ArrowDown") {
      event.preventDefault();
      focusTab((index + 1) % items.length);
      return;
    }

    if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
      event.preventDefault();
      focusTab((index - 1 + items.length) % items.length);
      return;
    }

    if (event.key === "Home") {
      event.preventDefault();
      focusTab(0);
      return;
    }

    if (event.key === "End") {
      event.preventDefault();
      focusTab(items.length - 1);
    }
  }

  return (
    <div aria-label={ariaLabel} className="workspace-tabs sticky" role="tablist">
      {items.map((tab, index) => (
        <button
          aria-controls={`${namespace}-panel-${tab.id}`}
          aria-selected={activeTab === tab.id}
          className="workspace-tab"
          data-active={activeTab === tab.id}
          id={`${namespace}-tab-${tab.id}`}
          key={tab.id}
          onKeyDown={(event) => handleKeyDown(event, index)}
          onClick={() => onSelect(tab.id)}
          ref={(node) => {
            tabRefs.current[index] = node;
          }}
          role="tab"
          tabIndex={activeTab === tab.id ? 0 : -1}
          type="button"
        >
          <strong>{tab.label}</strong>
          {tab.meta ? <span>{tab.meta}</span> : null}
        </button>
      ))}
    </div>
  );
}
