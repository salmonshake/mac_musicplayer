# 実装計画

仕様は [spec.md](spec.md)。各タスクは local-worker に 1 回 1 タスクで委譲する。

## 共通ルール（全タスク）
- cargo は `PATH="/opt/homebrew/opt/rustup/bin:$PATH"` が必要な場合がある
- crate バージョンは下記「依存バージョン」から変えない。記載のない crate を追加しない
- 製品コード（`src/` 配下、tests/examples 以外）で `unwrap()` / `expect()` / `panic!` を使わない
- `unsafe` は `core/src/coreaudio/` 内だけ。各 `unsafe` ブロックに `// SAFETY:` コメントを書く
- OSStatus は必ず `coreaudio::error::check()` で検査する
- 完了条件のコマンドがすべて成功してから終了報告する

## 依存バージョン（確認済み 2026-09-25）
| crate | version | features |
|---|---|---|
| objc2-core-audio-types | 0.3.2 | default |
| objc2-audio-toolbox | 0.3.2 | default-features = false, `std`, `AUComponent`, `AudioComponent`, `AudioOutputUnit`, `AudioUnitProperties`, `ExtendedAudioFile`, `objc2-core-audio-types`, `objc2-core-foundation` |
| objc2-core-foundation | 0.3.2 | default |
| rtrb | 0.4.0 | default |
| thiserror | 2.0.21 | default |

参照した API（objc2-audio-toolbox 0.3.2 のソースで確認済み）:
- `ExtAudioFileOpenURL(&CFURL, NonNull<ExtAudioFileRef>) -> OSStatus`（要 feature `objc2-core-foundation`）
- `ExtAudioFileSetProperty(ExtAudioFileRef, ExtAudioFilePropertyID, u32, NonNull<c_void>) -> OSStatus`
- `ExtAudioFileGetProperty(ExtAudioFileRef, ExtAudioFilePropertyID, NonNull<u32>, NonNull<c_void>) -> OSStatus`
- `ExtAudioFileRead(ExtAudioFileRef, NonNull<u32>, NonNull<AudioBufferList>) -> OSStatus`
- `ExtAudioFileDispose(ExtAudioFileRef) -> OSStatus`
- 定数: `kExtAudioFileProperty_FileDataFormat` / `_ClientDataFormat` / `_FileLengthFrames`
- `AudioComponentFindNext(AudioComponent, NonNull<AudioComponentDescription>) -> AudioComponent`
- `AudioComponentInstanceNew(AudioComponent, NonNull<AudioComponentInstance>) -> OSStatus`
- `AudioUnitSetProperty(AudioUnit, id, scope, element, *const c_void, u32) -> OSStatus`
- `AudioUnitInitialize` / `AudioOutputUnitStart` / `AudioOutputUnitStop` / `AudioUnitUninitialize` / `AudioComponentInstanceDispose`
- `AURenderCallback = Option<unsafe extern "C-unwind" fn(NonNull<c_void>, NonNull<AudioUnitRenderActionFlags>, NonNull<AudioTimeStamp>, u32, u32, *mut AudioBufferList) -> OSStatus>`
- `AURenderCallbackStruct { inputProc, inputProcRefCon: *mut c_void }`
- 定数: `kAudioUnitType_Output`, `kAudioUnitSubType_DefaultOutput`, `kAudioUnitManufacturer_Apple`, `kAudioUnitProperty_StreamFormat`, `kAudioUnitProperty_SetRenderCallback`
- `objc2_core_foundation::CFURL::from_file_path(path) -> Option<CFRetained<CFURL>>`
- `AudioStreamBasicDescription { mSampleRate: f64, mFormatID, mFormatFlags, mBytesPerPacket, mFramesPerPacket, mBytesPerFrame, mChannelsPerFrame, mBitsPerChannel, mReserved }`
- `AudioBufferList { mNumberBuffers: u32, mBuffers: [AudioBuffer; 1] }` / `AudioBuffer { mNumberChannels, mDataByteSize, mData: *mut c_void }`
- 定数: `kAudioFormatLinearPCM`, `kAudioFormatFlagsNativeFloatPacked`

---

## 段階1: CLI で FLAC を 1 曲再生

内部 PCM 形式は **f32 インターリーブ**（`kAudioFormatFlagsNativeFloatPacked`）に統一する。
段階1ではファイルのサンプルレートをそのまま出力 AU の入力フォーマットにし、DefaultOutput AU の内部 SRC に任せる。

データの流れ:
`ExtAudioFile(デコード, f32) → デコードスレッド → rtrb::Producer<f32> → rtrb::Consumer<f32> → レンダーコールバック → DefaultOutput AU`

### Task 1.1 ワークスペース骨格
- ファイル: `Cargo.toml`（workspace ルート）, `core/Cargo.toml`, `core/src/lib.rs`
- 内容:
  - ルート `Cargo.toml`: `[workspace] resolver = "3"`, `members = ["core"]`,
    `[workspace.package] edition = "2024", license = "MIT OR Apache-2.0"`
  - `core/Cargo.toml`: package 名 `mmp-core`（`core` は std と衝突するため不可）、lib 名 `mmp_core`。依存は上表のとおり
  - `core/src/lib.rs`: `pub mod coreaudio;` はまだ書かない。空の lib（doc コメント 1 行のみ）
- 完了条件: `cargo build -p mmp-core` が成功、`Cargo.lock` が生成される

### Task 1.2 テスト音源生成スクリプト
- ファイル: `scripts/gen-test-audio.sh`
- 内容: sox で `testdata/` に以下を生成（`testdata/` は `.gitignore` 済みの拡張子のみ出力）
  - `sine440_44k16_stereo.flac`: 440Hz サイン, 44100Hz, 16bit, 2ch, 3.0 秒, 振幅 -6dBFS
  - `sine1k_96k24_stereo.flac`: 1kHz サイン, 96000Hz, 24bit, 2ch, 2.0 秒
  - sox が無ければ「brew install sox」を案内して exit 1
- 完了条件: `bash scripts/gen-test-audio.sh` 後、`afinfo testdata/sine440_44k16_stereo.flac` がサンプルレート 44100・2ch・約 3.0 秒を表示

### Task 1.3 Core Audio エラー型
- ファイル: `core/src/coreaudio/mod.rs`, `core/src/coreaudio/error.rs`（+ `lib.rs` に `pub mod coreaudio;`）
- 型:
  ```rust
  #[derive(Debug, thiserror::Error)]
  pub enum CoreAudioError {
      #[error("{context} failed: OSStatus {status} ({fourcc})")]
      OsStatus { context: &'static str, status: i32, fourcc: String },
      #[error("invalid path: {0}")]
      InvalidPath(std::path::PathBuf),
      #[error("audio component not found: {0}")]
      ComponentNotFound(&'static str),
      #[error("unsupported format: {0}")]
      Unsupported(String),
  }
  pub type Result<T> = std::result::Result<T, CoreAudioError>;
  pub fn check(status: i32, context: &'static str) -> Result<()>;
  ```
  - `fourcc`: status の 4 バイトがすべて印字可能 ASCII なら `'wht?'` 形式、そうでなければ空文字
- 完了条件: `cargo test -p mmp-core` 成功。テスト: `check(0, ..)` が Ok、`check(-50, ..)` が Err、`0x77687421` ('wht?') で fourcc が `'wht?'`

### Task 1.4 ExtAudioFile 安全ラッパー
- ファイル: `core/src/coreaudio/ext_audio_file.rs`（+ `mod.rs` に追加）
- 型:
  ```rust
  pub struct StreamInfo { pub sample_rate: f64, pub channels: u32 }
  pub struct ExtAudioFile { raw: ExtAudioFileRef, client_channels: u32 }
  impl ExtAudioFile {
      pub fn open(path: &Path) -> Result<Self>;          // ファイル不在・非対応は Err
      pub fn file_info(&self) -> Result<StreamInfo>;     // FileDataFormat
      pub fn length_frames(&self) -> Result<i64>;        // FileLengthFrames
      /// クライアント形式を f32 インターリーブ・ファイルと同じレート/ch に設定
      pub fn set_client_f32_interleaved(&mut self, info: &StreamInfo) -> Result<()>;
      /// buf に最大 buf.len()/channels フレーム読む。戻り値は読んだフレーム数。0 = EOF
      pub fn read(&mut self, buf: &mut [f32]) -> Result<usize>;
  }
  impl Drop for ExtAudioFile  // ExtAudioFileDispose（戻り値は無視してよい）
  unsafe impl Send for ExtAudioFile {}
  ```
- 完了条件: `cargo test -p mmp-core` 成功。テスト（`core/tests/ext_audio_file.rs`、testdata が無ければ skip してメッセージ出力）:
  - 44.1k FLAC: `file_info` = 44100Hz/2ch、全 read のフレーム合計 == `length_frames()` == 132300
  - 存在しないパスで `open` が Err（panic しない）
  - 空ファイル（テスト内で一時作成）で `open` が Err

### Task 1.5 出力 AudioUnit ラッパー
- ファイル: `core/src/coreaudio/output.rs`（+ `mod.rs` に追加）
- 型:
  ```rust
  pub struct OutputUnit { unit: AudioUnit, state: *mut RenderState }
  struct RenderState { consumer: rtrb::Consumer<f32>, channels: usize }
  impl OutputUnit {
      /// DefaultOutput を作成し、入力スコープ element 0 に f32 インターリーブ形式を設定、
      /// レンダーコールバックを登録して Initialize する（Start はしない）
      pub fn new(info: &StreamInfo, consumer: rtrb::Consumer<f32>) -> Result<Self>;
      pub fn start(&mut self) -> Result<()>;
      pub fn stop(&mut self) -> Result<()>;
  }
  impl Drop  // Stop → Uninitialize → InstanceDispose → Box::from_raw(state) の順
  ```
- レンダーコールバックの制約（厳守）:
  - ロック・アロケーション・I/O・println/log 禁止
  - `consumer.read_chunk(n)` などで最大 `frames * channels` サンプルを出力バッファにコピー
  - 足りない分は 0.0 で埋める（アンダーラン時は無音）
  - 戻り値は常に 0
- 完了条件: `cargo build -p mmp-core` と `cargo test -p mmp-core` 成功。
  テスト: 容量 4096 のリングを作り `new` → `start` → 200ms sleep → `stop` → drop が Err/クラッシュなし（`#[ignore]` は付けない。出力デバイスが無い環境なら Err を許容しメッセージ出力）

### Task 1.6 1 曲再生関数
- ファイル: `core/src/playback.rs`（+ `lib.rs` に `pub mod playback;`）
- 型:
  ```rust
  #[derive(Debug, thiserror::Error)]
  pub enum PlaybackError { #[error(transparent)] CoreAudio(#[from] CoreAudioError) }
  /// ブロッキング。ファイルを最後まで再生し、リングが空になってから戻る
  pub fn play_file_blocking(path: &Path) -> Result<(), PlaybackError>;
  ```
- 処理: open → file_info → set_client_f32_interleaved → リング容量 = sample_rate × channels × 0.5 秒 →
  OutputUnit::new/start → ループで read（1 回 4096 フレーム）し producer に書き込み（空きが無ければ 5ms sleep）→
  EOF 後 `producer.slots() == 容量` になるまで待つ → さらに 50ms 待って stop
- 完了条件: `cargo build -p mmp-core` 成功、clippy 警告なし（`cargo clippy -p mmp-core -- -D warnings`）

### Task 1.7 CLI サンプル
- ファイル: `core/examples/play.rs`
- 内容: `cargo run -p mmp-core --example play -- <path>`。引数不足は使い方を表示して exit 2、エラーは stderr に表示して exit 1
- 完了条件: `cargo run -p mmp-core --example play -- testdata/sine440_44k16_stereo.flac` が約 3 秒で exit 0（音が鳴るかは指揮役/ユーザーが耳で確認）。存在しないパスで exit 1
