import { useEffect, useState } from "react";
import { ArrowLeft, CheckCircle2, Keyboard, Loader2, TriangleAlert } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import * as ipc from "@/lib/ipc";
import { cn } from "@/lib/utils";
import type { Settings } from "@/lib/types";

interface SettingsViewProps {
  settings: Settings | null;
  onBack: () => void;
  onChanged: () => void;
}

/** Retention presets; arbitrary values are not worth a slider here. */
const RETENTION_CHOICES = [100, 200, 500, 1000, 2000];

export function SettingsView({ settings, onBack, onChanged }: SettingsViewProps) {
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<{ tone: "ok" | "error"; text: string } | null>(null);
  const [binding, setBinding] = useState(settings?.hotkeyBinding ?? "<Control><Alt>v");

  useEffect(() => {
    if (settings?.hotkeyBinding) setBinding(settings.hotkeyBinding);
  }, [settings?.hotkeyBinding]);

  if (!settings) {
    return (
      <div className="flex flex-1 items-center justify-center text-muted-foreground">
        <Loader2 className="size-4 animate-spin" aria-hidden="true" />
      </div>
    );
  }

  const run = async (key: string, action: () => Promise<void>) => {
    setBusy(key);
    setMessage(null);
    try {
      await action();
      onChanged();
    } catch (cause) {
      setMessage({ tone: "error", text: String(cause) });
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-2 border-b border-border/70 px-2 py-2">
        <Button variant="ghost" size="icon-sm" onClick={onBack} aria-label="返回记录列表">
          <ArrowLeft className="size-4" aria-hidden="true" />
        </Button>
        <h2 className="text-sm font-medium">设置</h2>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
        <Section
          title="粘贴"
          description="点击记录后，把内容放回剪贴板并自动按下 Ctrl+V。"
        >
          <Row label="选择后自动粘贴">
            <Switch
              checked={settings.autoPaste}
              disabled={busy === "autoPaste"}
              onCheckedChange={(checked) =>
                run("autoPaste", () => ipc.updateSettings({ autoPaste: checked }))
              }
              aria-label="选择后自动粘贴"
            />
          </Row>

          <PortalNote settings={settings} />
        </Section>

        <Separator className="my-3" />

        <Section
          title="唤起方式"
          description="GNOME 不允许应用自行抢占全局快捷键，因此这里代为登记一条 GNOME 自定义快捷键。"
        >
          <Row label="当前绑定">
            <code className="rounded bg-muted px-1.5 py-0.5 text-[0.7rem]">
              {prettyBinding(settings.hotkeyBinding)}
            </code>
          </Row>

          <Row label="绑定 Ctrl+Alt+V">
            {settings.hotkeyInstalled ? (
              <Button
                variant="outline"
                size="sm"
                disabled={busy === "hotkey"}
                onClick={() =>
                  run("hotkey", async () => {
                    await ipc.removeHotkey();
                    setMessage({ tone: "ok", text: "已移除快捷键。" });
                  })
                }
              >
                移除
              </Button>
            ) : (
              <Button
                size="sm"
                disabled={busy === "hotkey"}
                onClick={() =>
                  run("hotkey", async () => {
                    await ipc.installHotkey(binding);
                    setMessage({
                      tone: "ok",
                      text: "已登记快捷键。若不起作用，请注销后重新登录一次。",
                    });
                  })
                }
              >
                <Keyboard className="size-3.5" aria-hidden="true" />
                登记
              </Button>
            )}
          </Row>
        </Section>

        <Separator className="my-3" />

        <Section
          title="存储"
          description="超出上限时，最久未使用的非置顶记录会被自动清理。"
        >
          <Row label="最多保留">
            <Select
              value={String(settings.maxItems)}
              onValueChange={(value) =>
                run("retention", () => ipc.updateSettings({ maxItems: Number(value) }))
              }
            >
              <SelectTrigger size="sm" className="w-28" aria-label="最多保留的记录数">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {RETENTION_CHOICES.map((choice) => (
                  <SelectItem key={choice} value={String(choice)}>
                    {choice} 条
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Row>
        </Section>

        {message && (
          <p
            role="status"
            className={cn(
              "mt-3 flex items-start gap-1.5 text-[0.7rem] leading-relaxed",
              message.tone === "ok" ? "text-primary" : "text-destructive",
            )}
          >
            {message.tone === "ok" ? (
              <CheckCircle2 className="mt-0.5 size-3 shrink-0" aria-hidden="true" />
            ) : (
              <TriangleAlert className="mt-0.5 size-3 shrink-0" aria-hidden="true" />
            )}
            <span>{message.text}</span>
          </p>
        )}

        <p className="mt-4 text-[0.68rem] leading-relaxed text-muted-foreground">
          本应用把历史记录保存在本机 SQLite 数据库
          <code className="mx-1 rounded bg-muted px-1 py-0.5">
            ~/.local/share/com.xinlan.clip/clipboard.db
          </code>
          中，不会上传到任何地方。
        </p>
      </div>
    </div>
  );
}

function Section({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: React.ReactNode;
}) {
  return (
    <section className="flex flex-col gap-2">
      <div>
        <h3 className="text-xs font-medium">{title}</h3>
        <p className="mt-0.5 text-[0.68rem] leading-relaxed text-muted-foreground">
          {description}
        </p>
      </div>
      <div className="flex flex-col gap-1.5">{children}</div>
    </section>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-lg bg-muted/40 px-2.5 py-2">
      <span className="text-xs">{label}</span>
      {children}
    </div>
  );
}

function PortalNote({ settings }: { settings: Settings }) {
  if (settings.portal.status === "ready") {
    return (
      <p className="flex items-start gap-1.5 text-[0.68rem] leading-relaxed text-muted-foreground">
        <CheckCircle2 className="mt-0.5 size-3 shrink-0" aria-hidden="true" />
        <span>
          已建立输入会话。点击条目会把内容放回剪贴板；部分 GNOME
          版本接受模拟按键请求但并不真正发送按键，此时按 Ctrl+V 即可。若始终无效，可关闭上面的开关。
        </span>
      </p>
    );
  }

  if (settings.portal.status === "denied") {
    return (
      <p className="flex items-start gap-1.5 text-[0.68rem] leading-relaxed text-warn">
        <TriangleAlert className="mt-0.5 size-3 shrink-0" aria-hidden="true" />
        <span>系统拒绝了输入权限，点击记录只会复制，需要手动粘贴。</span>
      </p>
    );
  }

  return (
    <p className="flex items-start gap-1.5 text-[0.68rem] leading-relaxed text-muted-foreground">
      <TriangleAlert className="mt-0.5 size-3 shrink-0" aria-hidden="true" />
      <span>尚未建立输入会话，暂时只能复制。</span>
    </p>
  );
}

/** Turn `<Control><Alt>v` into something readable. */
function prettyBinding(binding: string): string {
  return binding
    .replace(/<Control>/gi, "Ctrl+")
    .replace(/<Primary>/gi, "Ctrl+")
    .replace(/<Alt>/gi, "Alt+")
    .replace(/<Shift>/gi, "Shift+")
    .replace(/<Super>/gi, "Super+");
}
