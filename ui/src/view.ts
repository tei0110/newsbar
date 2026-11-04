// 小窓と「広げて読む」画面で共通の表示部品。判断は Rust 側にあり、ここは見せ方だけ。
import { type AppStatus, type Article, type UnreadCounts, el } from "./api";

/** 一覧に出すタイトル。空文字の要約タイトル（過去に保存されたもの）も、元のタイトルで代わりに出す */
export function displayTitle(a: Article): string {
  return a.title || a.original_title;
}

/** ヘッダーの「最終更新 HH:MM」または「更新中…」 */
export function updatedLabel(status: AppStatus): string {
  if (status.refreshing) return "更新中…";
  if (!status.last_refreshed_at) return "";
  const d = new Date(status.last_refreshed_at);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  return `最終更新 ${hh}:${mm}`;
}

/** 要約のエラーや、取得できなかった情報源があることを知らせる帯 */
export function renderBanners(box: HTMLElement, status: AppStatus) {
  box.replaceChildren();
  if (status.summary_error) {
    box.append(el("div", { class: "banner", text: status.summary_error }));
  }
  if (status.feed_error_count > 0) {
    box.append(el("div", { class: "banner", text: "一部の情報源を取得できませんでした（設定で確認）" }));
  }
}

/** 「すべて」と分野ごとのタブ（未読件数つき） */
export function renderTabs(
  box: HTMLElement,
  counts: UnreadCounts,
  current: string | null,
  onSelect: (label: string | null) => void,
) {
  const labels = counts.by_label.map(([label]) => label);
  if (current && !labels.includes(current)) labels.push(current);
  const countOf = (label: string) => counts.by_label.find(([l]) => l === label)?.[1] ?? 0;

  const tab = (label: string | null, text: string) => {
    const button = el("button", { class: label === current ? "tab active" : "tab", text });
    button.type = "button";
    button.addEventListener("click", () => onSelect(label));
    return button;
  };
  box.replaceChildren(tab(null, "すべて"), ...labels.map((label) => tab(label, `${label} ${countOf(label)}`)));
}

/** 一覧の一言要約（要約待ちならその旨、要約できなかったものは出さない） */
export function oneLine(a: Article): HTMLElement | null {
  if (a.summary_status === "pending") return el("div", { class: "one-line muted", text: "要約待ち" });
  if (a.summary_status === "done" && a.one_line) return el("div", { class: "one-line", text: a.one_line });
  return null;
}

/** 詳しい要約の本文。要約がまだのものと、要約できなかったものを言い分ける */
export function detailText(a: Article): string {
  if (a.detail) return a.detail;
  return a.summary_status === "pending" ? "要約待ち" : "要約できませんでした";
}
