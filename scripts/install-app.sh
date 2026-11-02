#!/bin/bash
# .app をビルドして /Applications/newsbar.app を入れ替え、起動し直す。
#
# 使い方: npm run install-app
#
# ビルドや確認に失敗したときは、/Applications の今のアプリには手を付けずに止まる。
# 置き場所は変わらないので、「ログイン時に起動」の登録はそのまま使える。
set -euo pipefail
cd "$(dirname "$0")/.."

APP_NAME="newsbar.app"
BUILT="target/release/bundle/macos/$APP_NAME"
INSTALLED="/Applications/$APP_NAME"

echo "==> ビルドしています（数分かかります）"
npm run tauri build -- --bundles app

echo "==> Sandbox の設定を確かめています"
entitlements=$(codesign -d --entitlements - "$BUILT" 2>/dev/null)
for key in com.apple.security.app-sandbox com.apple.security.network.client; do
  if ! grep -q "$key" <<<"$entitlements"; then
    echo "エラー: $key が入っていません。入れ替えをやめます" >&2
    exit 1
  fi
done

echo "==> 動いている newsbar を終了しています"
pkill -f "$APP_NAME/Contents/MacOS/newsbar" || true
# 終了を待つ（最大5秒）
for _ in 1 2 3 4 5 6 7 8 9 10; do
  pgrep -f "$APP_NAME/Contents/MacOS/newsbar" >/dev/null || break
  sleep 0.5
done

echo "==> $INSTALLED を入れ替えています"
# ditto は既存のフォルダに上書きで混ぜるので、先に古いほうを消す
rm -rf "$INSTALLED"
ditto "$BUILT" "$INSTALLED"

echo "==> 起動しています"
open "$INSTALLED"
echo "完了しました。キーチェーンの確認が出たら「常に許可」を選んでください"
