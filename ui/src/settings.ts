// 設定画面: 情報源の管理・おすすめ・自動更新の間隔。判断は Rust 側にあり、ここは表示と操作の受け渡しだけ。
import { listen } from "@tauri-apps/api/event";
import {
  type FetchStats,
  type Recommended,
  type Settings,
  type Source,
  addRecommended,
  addSource,
  deleteSource,
  el,
  fetchStats,
  getAutostart,
  getSettings,
  hasApiKey,
  listRecommended,
  listSources,
  saveSettings,
  setApiKey,
  setAutostart,
  setSourceEnabled,
} from "./api";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

async function renderSources() {
  const [sources, stats] = await Promise.all([listSources(), fetchStats()]);
  const box = $("sources");
  if (sources.length === 0) {
    box.replaceChildren(el("div", { class: "row muted", text: "情報源がありません" }));
    return;
  }
  box.replaceChildren(...sources.map((s) => sourceRow(s, stats.find((x) => x.source_id === s.id))));
}

function sourceRow(s: Source, stats: FetchStats | undefined): HTMLElement {
  const toggle = document.createElement("input");
  toggle.type = "checkbox";
  toggle.checked = s.enabled;
  toggle.title = s.enabled ? "有効" : "無効";
  toggle.addEventListener("change", async () => {
    await setSourceEnabled(s.id, toggle.checked);
    await renderSources();
  });

  const remove = el("button", { text: "削除" });
  remove.type = "button";
  remove.addEventListener("click", async () => {
    await deleteSource(s.id);
    await Promise.all([renderSources(), renderRecommended()]);
  });

  let fetched: HTMLElement | null = null;
  if (s.last_error) {
    fetched = el("div", { class: "error", text: `取得できませんでした: ${s.last_error}` });
  } else if (s.last_fetched_at) {
    fetched = el("div", { class: "url", text: `最終取得 ${new Date(s.last_fetched_at).toLocaleString("ja-JP")}` });
  }

  return el("div", { class: "row" }, [
    toggle,
    el("div", { class: "grow" }, [
      el("div", { class: "name", text: s.name }),
      el("div", { class: "url", text: s.feed_url, title: s.feed_url }),
      fetched,
      historyLine(stats),
    ]),
    el("span", { class: "badge", text: s.label }),
    remove,
  ]);
}

/** 直近7日の取得の結果。例: 「直近7日: 失敗 3回／120回（最後の失敗 11/06 10:30 HTTP 502）」 */
function historyLine(stats: FetchStats | undefined): HTMLElement | null {
  if (!stats || stats.total === 0) return null;
  if (stats.failures === 0) {
    return el("div", { class: "url", text: `直近7日: ${stats.total}回すべて取得できました` });
  }
  let last = "";
  if (stats.last_failure_at) {
    const at = new Date(stats.last_failure_at).toLocaleString("ja-JP", {
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
    last = `（最後の失敗 ${at} ${stats.last_failure_error ?? ""}）`;
  }
  return el("div", { class: "url", text: `直近7日: 失敗 ${stats.failures}回／${stats.total}回${last}` });
}

async function renderRecommended() {
  const list = await listRecommended();
  $("recommended").replaceChildren(...list.map(recommendedRow));
}

function recommendedRow(r: Recommended): HTMLElement {
  let action: HTMLElement;
  if (r.added) {
    action = el("span", { class: "badge", text: "追加済み" });
  } else {
    const button = el("button", { text: "追加" });
    button.type = "button";
    button.addEventListener("click", async () => {
      button.disabled = true;
      await addRecommended(r.feed_url);
      await Promise.all([renderSources(), renderRecommended()]);
    });
    action = button;
  }
  return el("div", { class: "row" }, [
    el("div", { class: "grow" }, [
      el("div", { class: "name", text: r.name }),
      el("div", { class: "url", text: r.feed_url, title: r.feed_url }),
    ]),
    el("span", { class: "badge", text: r.label }),
    action,
  ]);
}

$("add-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  const error = $("add-error");
  const button = $<HTMLButtonElement>("add-button");
  error.hidden = true;
  button.disabled = true;
  button.textContent = "確認中…";
  try {
    await addSource(
      $<HTMLInputElement>("add-name").value,
      $<HTMLInputElement>("add-url").value,
      $<HTMLSelectElement>("add-label").value,
    );
    $<HTMLFormElement>("add-form").reset();
    await Promise.all([renderSources(), renderRecommended()]);
  } catch (err) {
    error.textContent = String(err);
    error.hidden = false;
  } finally {
    button.disabled = false;
    button.textContent = "追加";
  }
});

/** 画面の入力欄から、保存する設定を組み立てる */
function currentSettings(): Settings {
  return {
    refresh_interval_minutes: Number($<HTMLSelectElement>("interval").value),
    model: $<HTMLInputElement>("model").value,
    language: $<HTMLSelectElement>("language").value as Settings["language"],
    popover_font_scale: Number($<HTMLSelectElement>("popover-font-scale").value),
    reader_font_scale: Number($<HTMLSelectElement>("reader-font-scale").value),
  };
}

async function save() {
  const error = $("settings-error");
  try {
    await saveSettings(currentSettings());
    error.hidden = true;
  } catch (err) {
    error.textContent = String(err);
    error.hidden = false;
  }
}

$("interval").addEventListener("change", () => void save());
$("language").addEventListener("change", () => void save());
$("model").addEventListener("change", () => void save());
$("popover-font-scale").addEventListener("change", () => void save());
$("reader-font-scale").addEventListener("change", () => void save());

async function renderKeyStatus() {
  $("key-status").textContent = (await hasApiKey()) ? "保存済み（キーチェーン）" : "未設定";
}

$("key-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  const input = $<HTMLInputElement>("api-key");
  const error = $("settings-error");
  try {
    await setApiKey(input.value);
    input.value = "";
    error.hidden = true;
  } catch (err) {
    error.textContent = String(err);
    error.hidden = false;
  }
  await renderKeyStatus();
});

// スイッチの見た目は、操作のあと macOS の登録状態に合わせ直す（失敗や許可待ちでも実際の状態を見せる）
async function renderAutostart() {
  $<HTMLInputElement>("autostart").checked = await getAutostart();
}

$("autostart").addEventListener("change", async () => {
  const error = $("autostart-error");
  try {
    await setAutostart($<HTMLInputElement>("autostart").checked);
    error.hidden = true;
  } catch (err) {
    error.textContent = String(err);
    error.hidden = false;
  }
  await renderAutostart();
});

async function init() {
  const settings = await getSettings();
  $<HTMLSelectElement>("interval").value = String(settings.refresh_interval_minutes);
  $<HTMLInputElement>("model").value = settings.model;
  $<HTMLSelectElement>("language").value = settings.language;
  $<HTMLSelectElement>("popover-font-scale").value = String(settings.popover_font_scale);
  $<HTMLSelectElement>("reader-font-scale").value = String(settings.reader_font_scale);
  await Promise.all([renderSources(), renderRecommended(), renderKeyStatus(), renderAutostart()]);
}

// 更新が終わったら、最終取得やエラーの表示を新しくする
void listen("articles-updated", () => void renderSources());
void init();
