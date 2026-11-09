// 小窓と「広げて読む」画面で共通の表示部品。判断は Rust 側にあり、ここは見せ方だけ。
import { listen } from "@tauri-apps/api/event";
import { type AppStatus, type Article, type Settings, type UnreadCounts, el, getSettings } from "./api";

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

/**
 * 文字の大きさを設定に合わせる。開いたときと、設定を保存したとき（"settings-updated"）に反映する。
 * `pick` で小窓用・「広げて読む」画面用のどちらの値を使うかを選ぶ
 */
export function followFontScale(pick: (s: Settings) => number) {
  const apply = (s: Settings) =>
    document.documentElement.style.setProperty("--font-scale", String(pick(s) / 100));
  void getSettings().then(apply).catch(() => undefined);
  void listen<Settings>("settings-updated", (e) => apply(e.payload));
}

/**
 * 検索欄（#search と ×ボタン #search-clear）を動かす。入力が落ち着いたら `onChange` に検索語を渡す。
 * 日本語の変換中は待ち、確定してから絞り込む。Esc か × で検索をやめる
 */
export function setupSearch(onChange: (query: string) => void) {
  const input = document.getElementById("search") as HTMLInputElement;
  const clear = document.getElementById("search-clear") as HTMLButtonElement;
  let timer: number | undefined;
  const fire = () => {
    window.clearTimeout(timer);
    clear.hidden = input.value === "";
    onChange(input.value.trim());
  };
  const reset = () => {
    input.value = "";
    fire();
  };
  input.addEventListener("input", (e) => {
    if ((e as InputEvent).isComposing) return;
    window.clearTimeout(timer);
    timer = window.setTimeout(fire, 250);
  });
  input.addEventListener("compositionend", fire);
  input.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && !e.isComposing) {
      e.preventDefault();
      reset();
    }
  });
  clear.addEventListener("click", () => {
    reset();
    input.focus();
  });
}

/** 絞り込み中であることと件数（例: 「"CVE" で絞り込み中 3件」）。検索していなければ隠す */
export function renderSearchStatus(box: HTMLElement, query: string, count: number) {
  box.hidden = query === "";
  box.textContent = query === "" ? "" : `"${query}" で絞り込み中 ${count}件`;
}

/** 一覧が空のときの文言 */
export function emptyListText(query: string): string {
  return query === "" ? "新着はありません" : "見つかりませんでした";
}
