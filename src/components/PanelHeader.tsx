import { Clipboard, Search, Settings, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

interface PanelHeaderProps {
  search: string;
  onSearchChange: (value: string) => void;
  searchRef: React.RefObject<HTMLInputElement | null>;
  onOpenSettings: () => void;
  onClose: () => void;
  /** Shown while a clipboard read is in flight. */
  capturing: boolean;
}

/**
 * Draggable header with the search field.
 *
 * `data-tauri-drag-region` lets the user move the frameless window, which on
 * Wayland is otherwise only possible through the compositor.
 */
export function PanelHeader({
  search,
  onSearchChange,
  searchRef,
  onOpenSettings,
  onClose,
  capturing,
}: PanelHeaderProps) {
  return (
    <header
      data-tauri-drag-region
      className="flex items-center gap-2 border-b border-border/70 px-3 py-2.5"
    >
      {/* The panel has no visible title bar, so this names the region. */}
      <h1 className="sr-only">xinlan-clip 剪贴板历史</h1>

      <div
        data-tauri-drag-region
        className="flex items-center gap-2 pl-0.5 text-muted-foreground"
      >
        <Clipboard
          className={cn("size-4", capturing && "text-primary")}
          aria-hidden="true"
        />
      </div>

      <div className="relative min-w-0 flex-1">
        <Search
          className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
          aria-hidden="true"
        />
        <Input
          ref={searchRef}
          name="clipboard-search"
          type="search"
          value={search}
          onChange={(event) => onSearchChange(event.target.value)}
          placeholder="搜索剪贴板历史…"
          aria-label="搜索剪贴板历史"
          autoComplete="off"
          spellCheck={false}
          data-search-input
          className="h-8 bg-transparent pl-8 text-sm"
        />
      </div>

      <Button
        variant="ghost"
        size="icon-sm"
        onClick={onOpenSettings}
        aria-label="打开设置"
        title="设置"
      >
        <Settings className="size-3.5" aria-hidden="true" />
      </Button>

      <Button
        variant="ghost"
        size="icon-sm"
        onClick={onClose}
        aria-label="隐藏面板"
        title="隐藏（Esc）"
      >
        <X className="size-3.5" aria-hidden="true" />
      </Button>
    </header>
  );
}
