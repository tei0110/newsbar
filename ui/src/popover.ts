// 小窓: 新着の一覧。判断は Rust 側にあり、ここは表示と操作の受け渡しだけ。
import { listen } from "@tauri-apps/api/event";
import {
  type Article,
  type AppStatus,
  type UnreadCounts,
  el,
  getStatus,
  listArticles,
  markAllRead,
  markRead,
  openSettings,
  openUrl,
  refreshNow,
  unreadCounts,
} from "./api";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

let currentLabel: string | null = null;
let includeRead = false;
let expandedId: number | null = null;
/** この小窓を開いている間に既読にした記事（閉じるまでは ○ で残す） */
const readThisSession = new Set<number>();

async function render() {
  const [articles, counts, status] = await Promise.all([
    listArticles(currentLabel, true),
    unreadCounts(),
    getStatus(),
  ]);
  renderHeader(counts, status);
  renderBanners(status);
  renderTabs(counts);
  renderList(articles.filter((a) => includeRead || !a.read_at || readThisSession.has(a.id)));
}

function renderHeader(counts: UnreadCounts, status: AppStatus) {
  $("count").textContent = `新着 ${counts.total}件`;
  let updated = "";
  if (status.refreshing) {
    updated = "更新中…";
  } else if (status.last_refreshed_at) {
    const d = new Date(status.last_refreshed_at);
    const hh = String(d.getHours()).padStart(2, "0");
    const mm = String(d.getMinutes()).padStart(2, "0");
    updated = `最終更新 ${hh}:${mm}`;
  }
  $("updated").textContent = updated;
  $<HTMLButtonElement>("refresh").disabled = status.refreshing;
}

function renderBanners(status: AppStatus) {
  const box = $("banners");
  box.replaceChildren();
  if (status.summary_error) {
    box.append(el("div", { class: "banner", text: status.summary_error }));
  }
  if (status.feed_error_count > 0) {
    box.append(el("div", { class: "banner", text: "一部の情報源を取得できませんでした（設定で確認）" }));
  }
}

function renderTabs(counts: UnreadCounts) {
  const labels = counts.by_label.map(([label]) => label);
  if (currentLabel && !labels.includes(currentLabel)) labels.push(currentLabel);
  const countOf = (label: string) => counts.by_label.find(([l]) => l === label)?.[1] ?? 0;

  const tab = (label: string | null, text: string) => {
    const button = el("button", { class: label === currentLabel ? "tab active" : "tab", text });
    button.type = "button";
    button.addEventListener("click", () => {
      currentLabel = label;
      expandedId = null;
      void render();
    });
    return button;
  };
  $("tabs").replaceChildren(
    tab(null, "すべて"),
    ...labels.map((label) => tab(label, `${label} ${countOf(label)}`)),
  );
}

function renderList(articles: Article[]) {
  const list = $("list");
  if (articles.length === 0) {
    list.replaceChildren(el("p", { class: "empty", text: "新着はありません" }));
    return;
  }
  list.replaceChildren(...articles.map(renderItem));
}

function renderItem(a: Article): HTMLElement {
  const unread = !a.read_at;
  const title = a.title ?? a.original_title;
  const item = el("article", { class: `item${unread ? " unread" : ""}${a.id === expandedId ? " open" : ""}` }, [
    el("div", { class: "item-title" }, [
      el("span", { class: "dot", text: unread ? "●" : "○" }),
      el("span", { text: title }),
    ]),
    oneLine(a),
    el("div", { class: "meta", text: `${a.source_name}・${a.age_label}` }),
  ]);
  if (a.id === expandedId) item.append(details(a));
  item.addEventListener("click", () => void toggle(a));
  return item;
}

function oneLine(a: Article): HTMLElement | null {
  if (a.summary_status === "pending") return el("div", { class: "one-line muted", text: "要約待ち" });
  if (a.summary_status === "done" && a.one_line) return el("div", { class: "one-line", text: a.one_line });
  return null;
}

function details(a: Article): HTMLElement {
  const open = el("button", { class: "open-link", text: "元記事を開く ↗" });
  open.type = "button";
  open.addEventListener("click", (e) => {
    e.stopPropagation();
    void openUrl(a.url);
  });
  return el("div", { class: "details" }, [
    el("p", { text: detailText(a), class: a.detail ? undefined : "muted" }),
    a.title ? el("p", { class: "original muted", text: a.original_title }) : null,
    open,
  ]);
}

/** 広げたときの本文。要約がまだのものと、要約できなかったものを言い分ける */
function detailText(a: Article): string {
  if (a.detail) return a.detail;
  return a.summary_status === "pending" ? "要約待ち" : "要約できませんでした";
}

async function toggle(a: Article) {
  expandedId = expandedId === a.id ? null : a.id;
  if (!a.read_at) {
    readThisSession.add(a.id);
    await markRead(a.id);
  }
  await render();
}

$("refresh").addEventListener("click", () => {
  void refreshNow().catch(() => undefined);
});
$("settings").addEventListener("click", () => void openSettings());
$("mark-all").addEventListener("click", async () => {
  await markAllRead(currentLabel);
  await render();
});
$<HTMLInputElement>("include-read").addEventListener("change", (e) => {
  includeRead = (e.target as HTMLInputElement).checked;
  void render();
});

void listen("popover-shown", () => {
  readThisSession.clear();
  expandedId = null;
  void render();
});
void listen("articles-updated", () => void render());
void render();
