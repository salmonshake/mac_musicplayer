# Project: Maplay — Lightweight macOS Music Player

GUI イメージ: [gui-mockup.webp](gui-mockup.webp)

## 前提
- 対象: macOS 14+ / Apple Silicon (aarch64-apple-darwin) のみ
- 言語: Rust のみ（Swift/Xcode プロジェクトは使わない）
- 公開: GitHub（MIT OR Apache-2.0）。音源ファイルはコミットしない
  - テスト用音源は sox/ffmpeg で生成するスクリプトのみコミットする
- crate のバージョンは Cargo.toml を正とする。勝手にバージョンを上げない
- API が不明なときは推測しない
  - Rust crate → docs.rs の該当バージョンを確認
  - Core Audio / AudioToolbox → Apple 公式ドキュメント（C API）を確認

## 構成
- workspace/core : 再生・ライブラリ・DB・タグ（UI 非依存、cargo test 可能に保つ）
- workspace/app  : egui/eframe UI

### 再生
- 出力: Core Audio（AudioUnit/HAL）を objc2-core-audio / objc2-audio-toolbox で直接使用。cpal は使わない
- デコード: AudioToolbox（ExtAudioFile）を主。Ogg Vorbis のみ symphonia
- デコードスレッド → オーディオコールバック間は rtrb（ロックフリーリングバッファ）
- ギャップレス: 次曲を先読みし、AAC/MP3 のエンコーダ遅延（priming）・末尾パディングを正しく除去する

### Core Audio FFI
- unsafe は core/src/coreaudio/ 内の安全ラッパーに閉じ込め、他モジュールから直接呼ばない
- OSStatus は必ずチェックし、独自エラー型に変換する
- デバイスの抜き差し・デフォルト出力変更を監視し、ストリームを安全に再構築する

### その他
- タグ: lofty（読み取り専用。タグ書き込みは実装しない）
- DB: rusqlite（キャッシュ扱い。いつ消しても再スキャンで完全に復元できること）
- 監視: notify（FSEvents）
- メディアキー / Now Playing: souvlaki

## データ方針
- ファイルのタグを正とし、DB はキャッシュ
- プレイリストは DB キャッシュとは別に永続化する（再スキャン・DB 削除で消えないこと）
- 保存先: ~/Library/Application Support/Maplay/

## 絶対ルール
- オーディオコールバック内でロック・アロケーション・I/O・ログ出力をしない
- unwrap()/expect() を製品コードで使わない（壊れたファイル・タグ無し・読めない形式でも落ちない）
- 日本語表示のため CJK フォントを必ず読み込む
- 数万曲前提:
  - 一覧は仮想化（表示行のみ描画）
  - スキャンはバックグラウンド・差分（mtime + サイズで判定）
  - DB に適切なインデックスを張る
  - アートワークはサムネイル生成 + ディスクキャッシュ、メモリは LRU で上限あり
  - 監視フォルダ（外付けドライブ等）が見えなくなっても落ちない
- 再生中の UI 再描画は間引く（シークバー更新は 250ms〜1s 間隔）
- 1 回の変更は 1 機能に絞る。スコープ外の機能を勝手に追加しない

## 音量モード
- 通常モード: ソフトウェア音量あり。必要に応じてデバイスのサンプルレートにリサンプル
- ビットパーフェクトモード:
  - 曲ごとに出力デバイスのサンプルレートを曲に追従させる
  - ソフトウェア音量は 100% 固定（UI のスライダーは無効化）
  - hog mode（排他）は設定で選択可能にする
  - Bluetooth など成立しないデバイスではこのモードを選択不可にする

## ライブラリ
- 指定フォルダ監視式（ファイルのコピー・移動はしない）
- アルバムのグループ化キー: Album Artist + Album（Album Artist が空なら Artist で代用）
- アルバム内の並び: DISCNUMBER → TRACKNUMBER

## スコープ（MVP）
- 再生: FLAC / ALAC / MP3 / AAC（HE-AAC 含む）/ WAV / AIFF / Ogg Vorbis、ギャップレス
- 音量モード: 通常 / ビットパーフェクト
- キュー / シャッフル / リピート
- 出力デバイス切り替え
- ライト/ダークテーマ切り替え
- プレイリスト（作成・編集・削除）
- アルバム一覧 / 曲一覧 / 検索
- ライブラリ: 指定フォルダ監視式

## やらない（明示的にスコープ外）
- CUE シート
- NAS / SMB
- DSD / APE / WMA
- タグ編集
- EQ / クロスフェード / ReplayGain
- 歌詞 / スクロブル
- 日本語ソート（読み仮名）は保留
- Opus は MVP 後に追加検討（libopus の C 依存が入るため）

## 開発の進め方
以下の順で段階的に実装し、各段階で動作確認・コミットしてから次へ進む。
1. CLI で FLAC を 1 曲再生（AudioToolbox デコード + Core Audio 出力）
2. 対応フォーマット拡充（Ogg Vorbis は symphonia）
3. ギャップレス再生
4. 出力デバイス列挙・切り替え、デバイス抜き差し対応
5. フォルダスキャン・タグ読み取り・DB 登録・差分スキャン・監視
6. egui でサイドバー + アルバム一覧 + 曲一覧（仮想化）+ 検索
7. プレイヤーバー・シーク・キュー / シャッフル / リピート
8. プレイリスト（永続化込み）
9. メディアキー / Now Playing（souvlaki）
10. アートワークキャッシュ
11. テーマ切り替え
12. ビットパーフェクトモード
