import { useEffect, useRef } from "react";
import { ViewTransition } from "react";
import { ClipboardList, SearchX } from "lucide-react";

import { EntryItem } from "@/components/EntryItem";
import { ScrollArea } from "@/components/ui/scroll-area";
import type { Entry } from "@/lib/types";

interface EntryListProps {
  entries: Entry[];
  activeIndex: number;
  loading: boolean;
  search: string;
  onActivate: (index: number) => void;
  onSelect: (index: number) => void;
  onTogglePin: (entry: Entry) => void;
  onDelete: (entry: Entry) => void;
}

/**
 * The scrollable history list.
 *
 * Each row is wrapped in a `ViewTransition` keyed by entry id so that deleting,
 * pinning, or reordering slides the remaining rows into place instead of
 * snapping. Driving these updates through `startTransition` is what activates
 * them; see `App.tsx`.
 */
export function EntryList({
  entries,
  activeIndex,
  loading,
  search,
  onActivate,
  onSelect,
  onTogglePin,
  onDelete,
}: EntryListProps) {
  const viewportRef = useRef<HTMLDivElement>(null);

  // Follow the keyboard cursor as it moves through a long list.
  useEffect(() => {
    const viewport = viewportRef.current?.querySelector<HTMLElement>(
      "[data-slot=scroll-area-viewport]",
    );
    if (!viewport) return;
    const active = viewport.querySelector<HTMLElement>(
      `[data-entry-index="${activeIndex}"]`,
    );
    active?.scrollIntoView({ block: "nearest" });
  }, [activeIndex, entries]);

  if (!loading && entries.length === 0) {
    return search.trim() ? (
      <EmptyState
        icon={<SearchX className="size-6" aria-hidden="true" />}
        title="没有匹配的记录"
        detail={`没有找到包含“${search.trim()}”的剪贴板内容。`}
      />
    ) : (
      <EmptyState
        icon={<ClipboardList className="size-6" aria-hidden="true" />}
        title="还没有剪贴板记录"
        detail="复制任意文本后，本面板一打开就会把它记录下来。"
      />
    );
  }

  return (
    <ScrollArea ref={viewportRef} className="min-h-0 flex-1">
      {/*
        Keyboard focus deliberately stays in the search box so the user can type
        the moment the panel appears. The list is therefore a listbox driven by
        `aria-activedescendant`, with the arrow/Enter handling done globally in
        `useKeyboardNav` — the accessible equivalent of a roving tabindex.
      */}
      <div
        role="listbox"
        aria-label="剪贴板历史"
        aria-activedescendant={
          entries[activeIndex] ? entryDomId(entries[activeIndex].id) : undefined
        }
        className="flex flex-col gap-0.5 p-2"
      >
        {entries.map((entry, index) => (
          <ViewTransition key={entry.id} default="none" update="vt-item">
            <EntryItem
              entry={entry}
              index={index}
              active={index === activeIndex}
              onActivate={onActivate}
              onSelect={onSelect}
              onTogglePin={onTogglePin}
              onDelete={onDelete}
            />
          </ViewTransition>
        ))}
      </div>
    </ScrollArea>
  );
}

/** DOM id used to link a listbox option to `aria-activedescendant`. */
export function entryDomId(id: number): string {
  return `clip-entry-${id}`;
}

function EmptyState({
  icon,
  title,
  detail,
}: {
  icon: React.ReactNode;
  title: string;
  detail: string;
}) {
  return (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-10 text-center">
      <div className="rounded-full bg-muted p-3 text-muted-foreground">{icon}</div>
      <p className="text-sm font-medium">{title}</p>
      <p className="text-xs leading-relaxed text-muted-foreground">{detail}</p>
    </div>
  );
}
