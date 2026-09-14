import { Info, ShieldAlert } from "lucide-react";

import type { PortalState } from "@/lib/types";

interface HintBannerProps {
  portal: PortalState;
  hotkeyInstalled: boolean;
}

/**
 * Explains the two platform limits that shape this app.
 *
 * Both are properties of GNOME 46 on Wayland rather than bugs, and both are
 * surprising enough that a silent panel would look broken:
 *
 * 1. There is no clipboard-change notification for background clients, so
 *    history only grows while the panel is open.
 * 2. The app cannot register its own global hotkey, because GNOME does not
 *    implement the GlobalShortcuts portal.
 */
export function HintBanner({ portal, hotkeyInstalled }: HintBannerProps) {
  const messages: { tone: "info" | "warn"; text: string }[] = [];

  if (!hotkeyInstalled) {
    messages.push({
      tone: "info",
      text: "尚未注册全局快捷键。在“设置”中一键绑定 Ctrl+Alt+V 后，才能随时唤起本面板。",
    });
  }

  if (portal.status === "denied") {
    messages.push({
      tone: "warn",
      text: "未获得模拟按键权限，点击条目只会复制到剪贴板，需要手动粘贴。",
    });
  } else if (portal.status === "ready") {
    // Honest wording: the portal accepts the keystroke, but whether the desktop
    // delivers it is outside the app's control (GNOME 46 accepts these calls
    // without delivering them), so neither "it works" nor "it is broken" is
    // accurate in advance.
    messages.push({
      tone: "info",
      text: "点击条目会把内容放回剪贴板；若没有自动粘贴，按 Ctrl+V 即可。",
    });
  } else {
    messages.push({
      tone: "warn",
      text: "输入会话尚未就绪，暂时只能复制而不能自动粘贴。",
    });
  }

  if (messages.length === 0) return null;

  return (
    <div className="flex flex-col gap-1 border-b border-border/70 bg-muted/40 px-3 py-2">
      {messages.map((message) => (
        <p
          key={message.text}
          className={
            message.tone === "warn"
              ? "flex items-start gap-1.5 text-[0.7rem] leading-relaxed text-warn"
              : "flex items-start gap-1.5 text-[0.7rem] leading-relaxed text-muted-foreground"
          }
        >
          {message.tone === "warn" ? (
            <ShieldAlert className="mt-0.5 size-3 shrink-0" aria-hidden="true" />
          ) : (
            <Info className="mt-0.5 size-3 shrink-0" aria-hidden="true" />
          )}
          <span>{message.text}</span>
        </p>
      ))}
    </div>
  );
}
