// Rust のコマンドを呼ぶための薄いラッパー。型は src-tauri/src/commands.rs・state.rs と対応させる。
import { invoke } from "@tauri-apps/api/core";

export type SummaryStatus = "pending" | "done" | "skipped" | "failed";

export interface Article {
  id: number;
  source_id: number;
  source_name: string;
  label: string;
  url: string;
  original_title: string;
  published_at: string;
  fetched_at: string;
  title: string | null;
  one_line: string | null;
  detail: string | null;
  summary_status: SummaryStatus;
  read_at: string | null;
  age_label: string;
}

export interface UnreadCounts {
  total: number;
  by_label: [string, number][];
}

export interface AppStatus {
  last_refreshed_at: string | null;
  feed_error_count: number;
  summary_error: string | null;
  refreshing: boolean;
}

export interface Source {
  id: number;
  name: string;
  feed_url: string;
  label: string;
  enabled: boolean;
  last_fetched_at: string | null;
  last_error: string | null;
}

export interface Recommended {
  name: string;
  feed_url: string;
  label: string;
  added: boolean;
}

export interface Settings {
  refresh_interval_minutes: number;
  model: string;
  language: "ja" | "en";
}

export interface RefreshReport {
  new_articles: number;
  summarized: number;
  feed_errors: [number, string][];
  summary_error: string | null;
}

export const listArticles = (label: string | null, includeRead: boolean) =>
  invoke<Article[]>("list_articles", { label, includeRead });
export const unreadCounts = () => invoke<UnreadCounts>("unread_counts");
export const markRead = (id: number) => invoke<void>("mark_read", { id });
export const markAllRead = (label: string | null) => invoke<number>("mark_all_read", { label });
export const openUrl = (url: string) => invoke<void>("open_url", { url });
export const refreshNow = () => invoke<RefreshReport>("refresh_now");
export const getStatus = () => invoke<AppStatus>("get_status");
export const listSources = () => invoke<Source[]>("list_sources");
export const addSource = (name: string, feedUrl: string, label: string) =>
  invoke<Source>("add_source", { name, feedUrl, label });
export const deleteSource = (id: number) => invoke<void>("delete_source", { id });
export const setSourceEnabled = (id: number, enabled: boolean) =>
  invoke<void>("set_source_enabled", { id, enabled });
export const listRecommended = () => invoke<Recommended[]>("list_recommended");
export const addRecommended = (feedUrl: string) => invoke<Source>("add_recommended", { feedUrl });
export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<void>("save_settings", { settings });
export const setApiKey = (key: string) => invoke<void>("set_api_key", { key });
export const hasApiKey = () => invoke<boolean>("has_api_key");
export const getAutostart = () => invoke<boolean>("get_autostart");
export const setAutostart = (enabled: boolean) => invoke<void>("set_autostart", { enabled });
export const openSettings = () => invoke<void>("open_settings");
export const openReader = () => invoke<void>("open_reader");

/** 小さな DOM 組み立て用の関数。文字列は必ず textContent で入れる（フィード由来の文字列を HTML として扱わない） */
export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: { class?: string; text?: string; title?: string } = {},
  children: (Node | null)[] = [],
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (attrs.class) node.className = attrs.class;
  if (attrs.text !== undefined) node.textContent = attrs.text;
  if (attrs.title) node.title = attrs.title;
  for (const child of children) if (child) node.append(child);
  return node;
}
