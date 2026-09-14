import { Eraser, Loader2, PauseCircle, Pin } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { formatCount } from "@/lib/format";

interface PanelFooterProps {
  total: number;
  pinned: number;
  paused: boolean;
  capturing: boolean;
  canClear: boolean;
  onClear: (keepPinned: boolean) => void;
  onTogglePause: (paused: boolean) => void;
}

/** Status line plus the destructive bulk action. */
export function PanelFooter({
  total,
  pinned,
  paused,
  capturing,
  canClear,
  onClear,
  onTogglePause,
}: PanelFooterProps) {
  return (
    <footer className="flex items-center gap-2 border-t border-border/70 px-3 py-2 text-[0.7rem] text-muted-foreground">
      <span aria-live="polite" className="flex items-center gap-1.5">
        {capturing ? (
          <Loader2 className="size-3 animate-spin" aria-hidden="true" />
        ) : paused ? (
          <PauseCircle className="size-3" aria-hidden="true" />
        ) : null}
        {paused ? "已暂停记录" : `${formatCount(total)} 条记录`}
      </span>

      {pinned > 0 && (
        <span className="flex items-center gap-1">
          <Pin className="size-2.5" aria-hidden="true" />
          {formatCount(pinned)} 条置顶
        </span>
      )}

      <span className="ml-auto flex items-center gap-2">
        <Button
          variant="ghost"
          size="xs"
          onClick={() => onTogglePause(!paused)}
          aria-pressed={paused}
        >
          {paused ? "继续记录" : "暂停记录"}
        </Button>

        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button variant="ghost" size="xs" disabled={!canClear}>
              <Eraser className="size-3" aria-hidden="true" />
              清空
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>清空剪贴板历史？</AlertDialogTitle>
              <AlertDialogDescription>
                将删除除置顶条目以外的全部记录。此操作无法撤销。
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>取消</AlertDialogCancel>
              <AlertDialogAction onClick={() => onClear(true)}>
                清空未置顶
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </span>
    </footer>
  );
}
