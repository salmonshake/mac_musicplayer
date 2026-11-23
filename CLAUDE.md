# Maplay — 開発ガイド（AI エージェント向け）

macOS 14+ / Apple Silicon 向けの軽量な音楽プレーヤー。Rust のみで書く。

## 参照
- 設計と守るべきルール: `docs/spec.md`（日本語）
- UI のイメージ: `docs/gui-mockup.webp`
- 使い方・データの保存場所・開発手順: `README.md`

## 構成
- `core/`（`maplay-core`）— UI 非依存。Core Audio 出力・デバイス、デコード、再生エンジン、ライブラリ（SQLite）、プレイリスト、アートワーク、設定。`cargo test` できる状態を保つ
- `app/`（`maplay-app`、バイナリ名 `maplay`）— egui/eframe の UI

## 検証コマンド
変更したら次をすべて通す。clippy の出力は `error` 行と `Finished` 行の両方を確認する（`tail` だけだと失敗を見落とす）。

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## コードのルール
- オーディオコールバック内でロック・アロケーション・I/O・ログ出力をしない
- 製品コード（tests / examples 以外）で `unwrap()` / `expect()` / `panic!` を使わない。壊れたファイル・タグ無し・読めない形式でも落ちない
- `unsafe` は `core/src/coreaudio/` の安全ラッパー内だけ。各 `unsafe` ブロックに `// SAFETY:` コメントを書く
- OSStatus は必ず `coreaudio::error::check()` で検査する
- Core Audio や crate の API は推測しない。docs.rs / Apple のドキュメントで確認する
- crate のバージョンは Cargo.toml を正とし、勝手に上げない。依存を足すときは理由を説明する
- 数万曲を前提にする（一覧は仮想化、スキャンは差分、重い処理はバックグラウンド）
- 1 回の変更は 1 機能に絞る。スコープ外の機能を足さない
- UI の文言は英語。コードのコメントは日本語でも英語でもよい
- 配色は WCAG AA（文字 4.5:1 以上）を満たし、色だけで状態を伝えない

## テストの書き方
- 音源が必要なテストは、`testdata/` に無ければスキップする（生成は `scripts/gen-test-audio.sh`）
- 実機の出力デバイスを使うテストは、必ずミュート（音量 0 か無音の音源）にする。デバイスが無い環境ではスキップする
- タイミングに依存するテストは、結果を上書きせず蓄積して判定する（不安定になりやすい）

## 画面の確認
環境変数で 1 フレームを PNG に保存して終了できる（画面のロックを解除しておく）。

```sh
MAPLAY_SCREENSHOT=/tmp/shot.png MAPLAY_THEME=light MAPLAY_NAV=album MAPLAY_PLAY=2 MAPLAY_SELECT=1 \
  MAPLAY_WINDOW=1440x900 cargo run -q -p maplay-app
```

文字化け（フォントに無いグリフ）、はみ出し、ライト/ダーク両方のコントラストを見る。

## その他
- 作業用のメモや個人のツール・ラッパースクリプトなど、製品に属さないものはこのリポジトリに入れない
- 音源ファイルはコミットしない（`.gitignore` 済み）。テスト音源は生成スクリプトで作る

## 言語
- ユーザーへの返答・報告は日本語で書く
