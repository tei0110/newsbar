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
  openReader,
  openSettings,
  openUrl,
  refreshNow,
  unreadCounts,
} from "./api";
import { detailText, displayTitle, followFontScale, oneLine, renderBanners, renderTabs, updatedLabel } from "./view";

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
  renderBanners($("banners"), status);
  renderTabs($("tabs"), counts, currentLabel, (label) => {
    currentLabel = label;
    expandedId = null;
    void render();
  });
  renderList(articles.filter((a) => includeRead || !a.read_at || readThisSession.has(a.id)));
}

function renderHeader(counts: UnreadCounts, status: AppStatus) {
  $("count").textContent = `新着 ${counts.total}件`;
  $("updated").textContent = updatedLabel(status);
  $<HTMLButtonElement>("refresh").disabled = status.refreshing;
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
  const item = el("article", { class: `item${unread ? " unread" : ""}${a.id === expandedId ? " open" : ""}` }, [
    el("div", { class: "item-title" }, [
      el("span", { class: "dot", text: unread ? "●" : "○" }),
      el("span", { text: displayTitle(a) }),
    ]),
    oneLine(a),
    el("div", { class: "meta", text: `${a.source_name}・${a.age_label}` }),
  ]);
  if (a.id === expandedId) item.append(details(a));
  item.addEventListener("click", () => void toggle(a));
  return item;
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
$("reader").addEventListener("click", () => void openReader());
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
followFontScale((s) => s.popover_font_scale);
