// 「広げて読む」画面: 左に一覧、右に選んだ記事の詳しい要約（2ペイン）。
// 溜まった記事を1件ずつ読み進めるための画面。判断は Rust 側にあり、ここは表示と操作の受け渡しだけ。
import { listen } from "@tauri-apps/api/event";
import {
  type Article,
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
import { detailText, displayTitle, followFontScale, oneLine, renderBanners, renderTabs, updatedLabel } from "./view";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

let currentLabel: string | null = null;
let includeRead = false;
let selectedId: number | null = null;
/** 一覧に今出している記事（キー操作で前後に移るため） */
let visible: Article[] = [];
/** この画面を開いている間に既読にした記事（閉じるまでは ○ で一覧に残す） */
const readThisSession = new Set<number>();

async function render() {
  const [articles, counts, status] = await Promise.all([
    listArticles(currentLabel, true),
    unreadCounts(),
    getStatus(),
  ]);
  $("count").textContent = `新着 ${counts.total}件`;
  $("updated").textContent = updatedLabel(status);
  $<HTMLButtonElement>("refresh").disabled = status.refreshing;
  renderBanners($("banners"), status);
  renderTabs($("tabs"), counts, currentLabel, (label) => {
    currentLabel = label;
    selectedId = null;
    void render();
  });

  visible = articles.filter((a) => includeRead || !a.read_at || readThisSession.has(a.id));
  renderList();
  const selected = visible.find((a) => a.id === selectedId);
  renderDetail(selected ?? null);

  // まだ何も選んでいない（開いた直後・タブを切り替えた直後）なら、いちばん上の未読を選ぶ
  if (!selected && visible.length > 0) {
    const first = visible.find((a) => !a.read_at) ?? visible[0];
    await select(first);
  }
}

function renderList() {
  const list = $("list");
  if (visible.length === 0) {
    list.replaceChildren(el("p", { class: "empty", text: "新着はありません" }));
    return;
  }
  list.replaceChildren(...visible.map(renderItem));
  list.querySelector(".item.open")?.scrollIntoView({ block: "nearest" });
}

function renderItem(a: Article): HTMLElement {
  const unread = !a.read_at;
  const item = el("article", { class: `item${unread ? " unread" : ""}${a.id === selectedId ? " open" : ""}` }, [
    el("div", { class: "item-title" }, [
      el("span", { class: "dot", text: unread ? "●" : "○" }),
      el("span", { text: displayTitle(a) }),
    ]),
    oneLine(a),
    el("div", { class: "meta", text: `${a.source_name}・${a.age_label}` }),
  ]);
  item.addEventListener("click", () => void select(a));
  return item;
}

function renderDetail(a: Article | null) {
  const box = $("detail");
  if (!a) {
    box.replaceChildren(el("p", { class: "empty", text: "記事を選ぶと、ここに詳しい要約が出ます" }));
    return;
  }
  const open = el("button", { class: "open-link", text: "元記事を開く ↗" });
  open.type = "button";
  open.addEventListener("click", () => void openUrl(a.url));
  const parts: (HTMLElement | null)[] = [
    el("div", { class: "meta", text: `${a.label} ｜ ${a.source_name}・${a.age_label}` }),
    el("h1", { text: displayTitle(a) }),
    a.summary_status === "done" && a.one_line ? el("p", { class: "lead", text: a.one_line }) : null,
    el("p", { class: a.detail ? "body" : "body muted", text: detailText(a) }),
    a.title ? el("p", { class: "original muted", text: a.original_title }) : null,
    open,
  ];
  box.replaceChildren(...parts.filter((p): p is HTMLElement => p !== null));
}

/** 記事を選んで右に出す。未読なら既読にする（右に出したら既読） */
async function select(a: Article) {
  selectedId = a.id;
  if (!a.read_at) {
    readThisSession.add(a.id);
    await markRead(a.id);
  }
  await render();
}

/** 一覧の中で前後の記事に移る */
function move(step: number) {
  if (visible.length === 0) return;
  const index = visible.findIndex((a) => a.id === selectedId);
  const next = visible[Math.min(Math.max(index + step, 0), visible.length - 1)];
  if (next && next.id !== selectedId) void select(next);
}

document.addEventListener("keydown", (e) => {
  if (e.metaKey || e.ctrlKey || e.altKey) return;
  if (e.key === "ArrowDown" || e.key === "j") {
    e.preventDefault();
    move(1);
  } else if (e.key === "ArrowUp" || e.key === "k") {
    e.preventDefault();
    move(-1);
  } else if (e.key === "Enter") {
    const a = visible.find((x) => x.id === selectedId);
    if (a) {
      e.preventDefault();
      void openUrl(a.url);
    }
  }
});

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

void listen("articles-updated", () => void render());
void render();
followFontScale((s) => s.reader_font_scale);
