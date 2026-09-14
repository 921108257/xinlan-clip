import { memo, useState } from "react";
import { Pin, PinOff, Trash2 } from "lucide-react";

import { entryDomId } from "@/components/EntryList";
import { cn } from "@/lib/utils";
import { describeContent, relativeTime, singleLine, truncate } from "@/lib/format";
import type { Entry } from "@/lib/types";

interface EntryItemProps {
  entry: Entry;
  index: number;
  active: boolean;
  onActivate: (index: number) => void;
  onSelect: (index: number) => void;
  onTogglePin: (entry: Entry) => void;
  onDelete: (entry: Entry) => void;
}

/** How many characters of a clip are shown before truncation. */
const PREVIEW_LIMIT = 220;

/**
 * One row of clipboard history.
 *
 * The whole row is the paste target, while pin and delete are revealed on hover
 * or keyboard focus so they never compete with the primary action.
 */
export const EntryItem = memo(function EntryItem({
  entry,
  index,
  active,
  onActivate,
  onSelect,
  onTogglePin,
  onDelete,
}: EntryItemProps) {
  const [hovered, setHovered] = useState(false);
  const preview = truncate(singleLine(entry.text), PREVIEW_LIMIT);

  return (
    <div
      id={entryDomId(entry.id)}
      role="option"
      aria-selected={active}
      data-entry-index={index}
      onMouseEnter={() => {
        setHovered(true);
        onActivate(index);
      }}
      onMouseLeave={() => setHovered(false)}
      onClick={(event) => {
        // Let the row buttons handle their own clicks.
        if ((event.target as HTMLElement).closest("[data-entry-action]")) return;
        onSelect(index);
      }}
      className={cn(
        "group/entry relative w-full cursor-pointer rounded-lg border px-3 py-2.5 text-left transition-colors",
        "border-transparent",
        active
          ? "border-ring/40 bg-accent"
          : "hover:border-border/60 hover:bg-accent/50",
      )}
    >
      <div className="flex items-start gap-2">
        <p
          className={cn(
            "min-w-0 flex-1 text-[0.82rem] leading-relaxed break-words whitespace-pre-wrap",
            active ? "text-accent-foreground" : "text-foreground/90",
          )}
        >
          {preview}
        </p>

        <div
          className={cn(
            "flex shrink-0 items-center gap-0.5 transition-opacity",
            hovered || active ? "opacity-100" : "opacity-0",
          )}
        >
          <button
            type="button"
            data-entry-action="pin"
            aria-label={entry.pinned ? "取消置顶" : "置顶此条"}
            title={entry.pinned ? "取消置顶" : "置顶此条"}
            onClick={(event) => {
              event.stopPropagation();
              onTogglePin(entry);
            }}
            className="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-background/70 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          >
            {entry.pinned ? (
              <PinOff className="size-3.5" aria-hidden="true" />
            ) : (
              <Pin className="size-3.5" aria-hidden="true" />
            )}
          </button>

          <button
            type="button"
            data-entry-action="delete"
            aria-label="删除此条记录"
            title="删除此条记录"
            onClick={(event) => {
              event.stopPropagation();
              onDelete(entry);
            }}
            className="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-destructive/15 hover:text-destructive focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          >
            <Trash2 className="size-3.5" aria-hidden="true" />
          </button>
        </div>
      </div>

      <div className="mt-1.5 flex items-center gap-2 text-[0.68rem] text-muted-foreground">
        {entry.pinned && (
          <span className="inline-flex items-center gap-1 rounded-full bg-primary/15 px-1.5 py-0.5 font-medium text-primary">
            <Pin className="size-2.5" aria-hidden="true" />
            已置顶
          </span>
        )}
        <span className="tabular-nums">{relativeTime(entry.lastUsedAt)}</span>
        <span aria-hidden="true">·</span>
        <span>{describeContent(entry.text)}</span>
        {entry.useCount > 1 && (
          <>
            <span aria-hidden="true">·</span>
            <span className="tabular-nums">用过 {entry.useCount} 次</span>
          </>
        )}
        {index < 9 && active && (
          <span className="ml-auto rounded border border-border px-1 tabular-nums">
            Ctrl+{index + 1}
          </span>
        )}
      </div>
    </div>
  );
});
