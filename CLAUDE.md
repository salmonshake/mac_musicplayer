## 実装の進め方（指揮役向け）
- 設計・タスク分解はあなたが行い、docs/plan.mdに書く
  （1タスク=1〜2ファイル、パス・型・完了条件を明記）
- 実装は `~/bin/local-worker "指示"` に委譲する。1回の呼び出しで1タスクだけ
- 指示にはplan.mdのタスク番号と完了条件を含める
- 委譲後は必ず git diff とビルド/テスト結果を確認してレビューする
- 問題があれば、具体的な修正指示を付けて再委譲する
- 自分でコードを直接書くのは、同じタスクでlocal-workerが2回失敗したときだけ
  - 例外: 音声エンジンのコア（Core Audio FFI・オーディオスレッド/リングバッファの並行処理・再生エンジン）は指揮役が直接書いてよい（ユーザー合意 2026-09-28。ローカルモデルでは細分化しても通らなかったため）
- タスクが通ったらgit commitする

## 仕様
- 全体仕様・絶対ルールは docs/spec.md、GUI イメージは docs/gui-mockup.webp
- local-worker は終了後に自分で `cargo clippy --workspace --all-targets -D warnings` と `cargo test --workspace` を実行して検証し、
  失敗（またはターン上限）なら自動で Opus（~/bin/ask-opus）に助言を求めて 1 回だけ再試行する。
  出力の最終行 `local-worker: RESULT=ok|failed escalated=yes|no` を必ず確認する（ok でもレビューは省略しない）。
  「2回失敗」の数え方: RESULT=failed を 1 回の失敗とする

## 言語
- ユーザーへの返答・報告は日本語で書く
- アプリの UI 文言は英語（docs/gui-mockup.webp に合わせる）。コードのコメントは日本語でも英語でもよい
