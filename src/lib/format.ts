/** Presentation helpers shared by the panel components. */

const MINUTE = 60;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** Locale used for all user-facing numbers and dates. */
const LOCALE = "zh-CN";

const numberFormat = new Intl.NumberFormat(LOCALE);

/** Short date for entries older than a week. */
const shortDateFormat = new Intl.DateTimeFormat(LOCALE, {
  month: "numeric",
  day: "numeric",
});

/** Short date including the year, for entries from a previous year. */
const fullDateFormat = new Intl.DateTimeFormat(LOCALE, {
  year: "numeric",
  month: "numeric",
  day: "numeric",
});

/**
 * Human-readable age of a timestamp.
 *
 * Deliberately coarse: a clipboard list is scanned, not read, so exact seconds
 * would be noise. Older entries fall back to `Intl.DateTimeFormat` so the
 * output follows the locale instead of a hand-rolled pattern.
 */
export function relativeTime(unixSeconds: number, now = Date.now() / 1000): string {
  const delta = Math.max(0, now - unixSeconds);
  if (delta < 45) return "刚刚";
  if (delta < HOUR) return `${Math.round(delta / MINUTE)} 分钟前`;
  if (delta < DAY) return `${Math.round(delta / HOUR)} 小时前`;
  if (delta < 7 * DAY) return `${Math.round(delta / DAY)} 天前`;

  const date = new Date(unixSeconds * 1000);
  const sameYear = date.getFullYear() === new Date(now * 1000).getFullYear();
  return sameYear ? shortDateFormat.format(date) : fullDateFormat.format(date);
}

/** Collapse all whitespace runs so multi-line clips stay on one row. */
export function singleLine(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

/** Short label describing what kind of text was copied. */
export function describeContent(text: string): string {
  const trimmed = text.trim();
  if (/^https?:\/\/\S+$/i.test(trimmed)) return "链接";
  if (/^[\w.+-]+@[\w-]+\.[\w.-]+$/.test(trimmed)) return "邮箱";
  if (/^#[0-9a-f]{3,8}$/i.test(trimmed)) return "颜色";
  if (/^[+-]?\d+(\.\d+)?$/.test(trimmed)) return "数字";
  if (trimmed.includes("\n")) return `${trimmed.split("\n").length} 行文本`;
  return "文本";
}

/** Group-separated count, following the locale. */
export function formatCount(value: number): string {
  return numberFormat.format(value);
}

/**
 * Truncate for display without splitting a surrogate pair.
 *
 * The full text is never lost — only the preview is shortened.
 */
export function truncate(text: string, max: number): string {
  const chars = Array.from(text);
  if (chars.length <= max) return text;
  return `${chars.slice(0, max).join("")}…`;
}
