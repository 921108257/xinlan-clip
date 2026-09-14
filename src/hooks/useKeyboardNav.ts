import { useEffect } from "react";

interface KeyboardNavOptions {
  itemCount: number;
  activeIndex: number;
  setActiveIndex: (index: number) => void;
  onSelect: (index: number) => void;
  onEscape: () => void;
  /** When true, single-letter shortcuts are suppressed. */
  disabled?: boolean;
}

/**
 * Global keyboard navigation for the panel.
 *
 * The search box deliberately keeps focus at all times so the user can type
 * immediately after the panel appears; arrow keys therefore have to be handled
 * at the window level rather than on a focused list. This mirrors how launcher
 * style pickers behave.
 */
export function useKeyboardNav({
  itemCount,
  activeIndex,
  setActiveIndex,
  onSelect,
  onEscape,
  disabled = false,
}: KeyboardNavOptions) {
  useEffect(() => {
    if (disabled) return;

    const onKeyDown = (event: KeyboardEvent) => {
      // Radix dialogs already own Enter/Escape/Tab, and the modal layer means
      // the panel behind it must not act on those keys.
      if (document.body.dataset.modalOpen === "true") return;

      const target = event.target as HTMLElement | null;
      const typing = isTextEntry(target);
      const meta = event.ctrlKey || event.metaKey;

      switch (event.key) {
        case "ArrowDown": {
          if (itemCount === 0) return;
          event.preventDefault();
          const next = activeIndex < 0 ? 0 : Math.min(activeIndex + 1, itemCount - 1);
          setActiveIndex(next);
          break;
        }

        case "ArrowUp": {
          if (itemCount === 0) return;
          event.preventDefault();
          const next = activeIndex <= 0 ? 0 : activeIndex - 1;
          setActiveIndex(next);
          break;
        }

        case "Home": {
          if (itemCount === 0 || typing) return;
          event.preventDefault();
          setActiveIndex(0);
          break;
        }

        case "End": {
          if (itemCount === 0 || typing) return;
          event.preventDefault();
          setActiveIndex(itemCount - 1);
          break;
        }

        case "Enter": {
          if (itemCount === 0) return;
          event.preventDefault();
          onSelect(activeIndex < 0 ? 0 : activeIndex);
          break;
        }

        case "Escape": {
          event.preventDefault();
          onEscape();
          break;
        }

        default:
          // Ctrl+number jumps straight to an entry, like the ten-item
          // shortcuts in most clipboard managers.
          if (meta && /^[1-9]$/.test(event.key)) {
            const target = Number(event.key) - 1;
            if (target < itemCount) {
              event.preventDefault();
              setActiveIndex(target);
            }
          }
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    disabled,
    itemCount,
    activeIndex,
    setActiveIndex,
    onSelect,
    onEscape,
  ]);
}

/** Whether an element consumes text input, so nav keys should be left alone. */
function isTextEntry(element: HTMLElement | null): boolean {
  if (!element) return false;
  const tag = element.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA") {
    // In an empty search box, Home/End would be a no-op anyway; treat it as
    // non-typing so list navigation still works.
    if (tag === "INPUT") {
      const input = element as HTMLInputElement;
      return input.value.length > 0;
    }
    return true;
  }
  return element.isContentEditable;
}
