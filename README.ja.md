# Maplay

Rust だけで書かれた、軽量でネイティブな macOS 用の音楽プレーヤーです。

[English](README.md) | 日本語

Maplay は、すでに持っているファイルをそのまま再生します。取り込みもコピーも不要です。
音楽フォルダを指定すると、検索できる高速なライブラリを常に最新に保ちます。音は Core Audio に直接出力し、
ギャップレス再生と、任意で使えるビットパーフェクトモードに対応しています。

> **状態:** 開発初期（0.1.0）です。Apple Silicon の Mac で日常的に使う目的で作っていて、動作確認は
> 1 台の Mac でしか行っていません。荒い部分があります。バイナリの配布はまだなく、ソースからビルドして使います。

## 機能

- **対応フォーマット:** FLAC、ALAC、MP3、AAC（HE-AAC を含む）、WAV、AIFF、Ogg Vorbis
- **ギャップレス再生**（AAC / MP3 のエンコーダ遅延と末尾のパディングも補正）
- **ビットパーフェクトモード:** 出力デバイスのサンプルレートを曲ごとに合わせ、ソフトウェア音量を使いません。
  任意で排他アクセス（hog mode）も使えます。成立しない Bluetooth や AirPlay の出力では選べません。
- **出力デバイスの選択**（デバイスが抜かれたときは自動でデフォルトに切り替え）
- **選んだフォルダからライブラリを作成** — ファイルはコピーも移動もしません。変更は自動で検知し（FSEvents）、再スキャンは差分だけです
- **アルバム・アーティスト・ジャンル・全曲**で閲覧でき、すぐに検索できます。一覧は仮想化していて、数万曲でも軽快です
- **プレイリスト**は通常の `.m3u8` ファイルとして保存し、ライブラリのデータベースとは独立しています
- **キュー**（Up Next）、シャッフル、リピート
- **アルバムアート:** タグの埋め込み画像か `cover.jpg` / `folder.png` を使い、サムネイルをキャッシュします
- **メディアキーと「再生中」表示**に対応
- **ライト・ダーク・システム連動**のテーマ。日本語などの CJK 文字も正しく表示します

## 動作環境

- macOS 14 以降、Apple Silicon（arm64）
- 最近の安定版 Rust ツールチェーン（edition 2024。開発に使ったのは Rust 1.98）

## ビルドと起動

```sh
cargo run --release -p maplay-app
```

アイコンと Info.plist、アドホック署名を含む `Maplay.app` を作って、インストールするには:

```sh
scripts/bundle.sh              # dist/Maplay.app を作成
scripts/bundle.sh --install    # さらに /Applications にコピー
```

署名はアドホック署名です。自分でビルドしたものを自分の Mac で動かすだけなら十分です。ほかの人に配るには、
Developer ID での署名と公証が必要です。

## 使い方

サイドバーの **Add folder…** で音楽フォルダを選びます。曲をダブルクリックすると再生します。

| キー | 動作 |
| --- | --- |
| Space | 再生 / 一時停止 |
| ⌘→ / ⌘← | 次の曲 / 前の曲 |
| ⌘F | 検索欄にフォーカス（Esc で内容を消去） |
| メディアキー | 再生 / 一時停止、次、前 |

曲を右クリックするとプレイリストに追加できます。ツールバーの ⚙ メニューに、ビットパーフェクトモード・排他アクセス・
出力デバイスがあります。その隣のボタンでテーマを切り替えます。

### データの保存場所

すべて `~/Library/Application Support/Maplay/` の下にあります。

| パス | 内容 |
| --- | --- |
| `library.db` | ライブラリのキャッシュ（SQLite）。消しても再スキャンで復元されます |
| `playlists/*.m3u8` | プレイリスト |
| `settings.txt` | テーマ、ビットパーフェクト、排他アクセス、出力デバイス |
| `artwork/` | サムネイルのキャッシュ。消しても問題ありません |

正はファイルのタグです。Maplay が音楽ファイルに書き込むことはありません。

## 開発

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

ワークスペースは 2 つのクレートでできています。

- `core/`（`maplay-core`）— UI 以外のすべて。Core Audio の出力とデバイス管理（`unsafe` はここだけで、安全な API で包んでいます）、
  デコード、再生エンジン、ライブラリのスキャンとクエリ、プレイリスト、アートワーク、設定
- `app/`（`maplay-app`、バイナリ名は `maplay`）— egui / eframe の UI

仕組み: 音声は AudioToolbox（`ExtAudioFile`）でデコードします。Ogg Vorbis だけは
[symphonia](https://github.com/pdeljanov/Symphonia) を使います。デコードスレッドがロックフリーのリングバッファ
（[rtrb](https://github.com/mgeier/rtrb)）に書き込み、Core Audio のレンダーコールバックがそれを読み出します。
このコールバックの中では、ロック・メモリ確保・I/O を一切行いません。ライブラリは、
[lofty](https://github.com/Serial-ATA/lofty-rs) で読んだタグの SQLite キャッシュです。

### テスト用の音源

音源ファイルが必要なテストは、`testdata/` がなければ自動でスキップされます。生成するには（`sox`、`lame`、macOS の
`afconvert` が必要）:

```sh
brew install sox lame
scripts/gen-test-audio.sh
```

実際の出力デバイスを開くテストもありますが、音は必ずミュートです。ビットパーフェクトのテストは、デフォルトデバイスの
サンプルレートを一時的に切り替えて、終了後に元へ戻します。デバイスがなければスキップされます。

### 開発用のスクリーンショット

`MAPLAY_SCREENSHOT=<file.png>` を指定すると、アプリは 1 フレームをそのファイルに描画して終了します。
`MAPLAY_THEME=light|dark`、`MAPLAY_NAV=albums|artists|genres|album`、`MAPLAY_PLAY=<行>`（ミュートで再生）、
`MAPLAY_SELECT=<行>` を組み合わせると、特定の状態を撮影できます。画面のロックを解除しておく必要があります。

### ドキュメント

- [`docs/spec.md`](docs/spec.md) — 設計と守るべきルール（日本語）
- [`docs/gui-mockup.webp`](docs/gui-mockup.webp) — UI が従っているモックアップ

## 予定していないもの

CUE シート、ネットワーク共有、DSD / APE / WMA、タグ編集、EQ、クロスフェード、ReplayGain、歌詞、スクロブル。Opus は将来追加するかもしれません。

## ライセンス

次のいずれかを選んで利用できます。

- Apache License, Version 2.0（[LICENSE-APACHE](LICENSE-APACHE)）
- MIT ライセンス（[LICENSE-MIT](LICENSE-MIT)）

別途明示しない限り、このプロジェクトに対して意図的に提出された貢献は、Apache-2.0 ライセンスの定義に従い、
追加の条件なしに上記のデュアルライセンスで提供されたものとみなします。
