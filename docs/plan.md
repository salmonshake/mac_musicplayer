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
- 完了条件: `cargo test -p mmp-core` 成功。テスト: `check(0, ..)` が Ok、`check(-50, ..)` が Err、`0x7768743f` ('wht?') で fourcc が `'wht?'`

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

### Task 1.5a 後片付け（1.5 レビュー指摘）
- ファイル: `core/src/coreaudio/output.rs`, `core/tests/output_unit.rs`
- 内容:
  - `RenderState` と `consumer` フィールドを非公開にする
  - `output.rs` のすべての `unsafe` ブロックの直前に `// SAFETY:` コメントを付ける
  - テスト: `OutputUnit::new` が Err なら eprintln して return（出力デバイスの無い環境で失敗させない）。テスト内の余分な `mod tests` ラッパーを外す
- 完了条件: `cargo test -p mmp-core` と `cargo clippy -p mmp-core --all-targets -- -D warnings` が成功し、`output.rs` の `unsafe` がすべて `// SAFETY:` 付き

---

## 段階2: 対応フォーマット拡充

方針:
- FLAC / ALAC / MP3 / AAC / HE-AAC / WAV / AIFF は ExtAudioFile で読む（AudioToolbox が対応済み）
- Ogg Vorbis だけ symphonia で読む
- 両者を `Decoder` トレイトで抽象化し、`playback` は拡張子に応じた `decode::open()` を使う
- ギャップレス（priming/padding の除去）は段階3。段階2では長さの厳密一致は求めない

### 依存の追加（段階2、確認済み 2026-09-25）
| crate | version | features |
|---|---|---|
| objc2-audio-toolbox | 0.3.2 | 既存 features に `AudioFile` を追加 |
| symphonia | 0.6.1 | default-features = false, `ogg`, `vorbis` |

参照した API:
- HE-AAC: `ExtAudioFileGetProperty(kExtAudioFileProperty_AudioFile)` で `AudioFileID`（`*mut OpaqueAudioFileID`）を取得 →
  `AudioFileGetPropertyInfo(id, kAudioFilePropertyFormatList, *mut u32 size, *mut u32 writable)` →
  `AudioFileGetProperty(id, kAudioFilePropertyFormatList, NonNull<u32>, NonNull<c_void>)` で `[AudioFormatListItem]`（`mASBD`, `mChannelLayoutTag`）。
  先頭要素が再生すべき最高品質の形式（afinfo で確認: HE-AAC は `[0] 44100Hz aach`, `[1] 22050Hz aac`）。
  FileDataFormat は基本層の 22050Hz を返すので、そのまま使うと SBR が失われる
- symphonia 0.6.1（examples/basic-interleaved.rs を正とする）:
  `MediaSourceStream::new(Box::new(File), Default::default())`,
  `symphonia::default::get_probe().probe(&Hint, mss, FormatOptions, MetadataOptions) -> Result<Box<dyn FormatReader>>`,
  `format.default_track(TrackType::Audio) -> Option<&Track>`, `track.codec_params.as_ref()?.audio()?`（`AudioCodecParameters { sample_rate: Option<u32>, channels: Option<Channels>, .. }`, `Channels::count()`），
  `symphonia::default::get_codecs().make_audio_decoder(params, &AudioDecoderOptions) -> Result<Box<dyn AudioDecoder>>`,
  `format.next_packet() -> Result<Option<Packet>>`, `decoder.decode(&packet) -> Result<GenericAudioBufferRef>`,
  `buf.samples_interleaved()`, `buf.copy_to_slice_interleaved(&mut [f32])`,
  `symphonia::core::errors::Error::{IoError, DecodeError, SeekError, Unsupported, LimitError, ResetRequired}`

### Task 2.1 テスト音源の追加
- ファイル: `scripts/gen-test-audio.sh`
- 内容: 既存 2 ファイルに加え、sox で `testdata/_src_44k16.wav`（440Hz, 44100Hz, 16bit, 2ch, 3.0 秒, gain -6）を作り、そこから以下を生成
  - `sine440_44k16.wav`（sox でコピー）, `sine440_44k16.aiff`（sox）, `sine440.mp3`（sox）, `sine440.ogg`（sox, Vorbis）
  - `sine440_alac.m4a`: `afconvert -f m4af -d alac`
  - `sine440_aac.m4a`: `afconvert -f m4af -d aac -b 192000`
  - `sine440_heaac.m4a`: `afconvert -f m4af -d aach -b 64000`
- 完了条件: スクリプト実行後、上記 7 ファイルが存在し、`afinfo` で 2ch・約 3 秒（HE-AAC の Data format は 22050Hz 表示でよい）

### Task 2.2 HE-AAC の正しいサンプルレート
- ファイル: `core/Cargo.toml`（features に `AudioFile`）, `core/src/coreaudio/ext_audio_file.rs`, `core/tests/ext_audio_file.rs`
- 内容: `file_info()` を「フォーマットリストの先頭要素の `mASBD` を使い、取得できない・空なら従来どおり FileDataFormat」に変更
- 完了条件: テスト追加 — `sine440_heaac.m4a` の `file_info().sample_rate == 44100.0`、`sine440_aac.m4a` と既存 FLAC も 44100。全テスト・clippy 成功

### Task 2.3 Decoder トレイトと ExtAudioFile 実装
- ファイル: `core/src/decode/mod.rs`, `core/src/decode/ext.rs`（+ `lib.rs` に `pub mod decode;`）
- 型:
  ```rust
  pub trait Decoder: Send {
      fn info(&self) -> StreamInfo;
      /// f32 インターリーブで最大 buf.len()/channels フレーム。戻り値はフレーム数、0 = EOF
      fn read(&mut self, buf: &mut [f32]) -> Result<usize, DecodeError>;
  }
  #[derive(Debug, thiserror::Error)]
  pub enum DecodeError {
      #[error(transparent)] CoreAudio(#[from] CoreAudioError),
      #[error("io: {0}")] Io(#[from] std::io::Error),
      #[error("decode: {0}")] Codec(String),
      #[error("no audio track")] NoAudioTrack,
      #[error("unsupported: {0}")] Unsupported(String),
  }
  /// 拡張子（小文字化）で振り分け。"ogg" | "oga" は段階2.4 まで Unsupported、それ以外は ExtDecoder
  pub fn open(path: &Path) -> Result<Box<dyn Decoder>, DecodeError>;
  ```
  - `ExtDecoder { file: ExtAudioFile, info: StreamInfo }`: open 時に `file_info` → `set_client_f32_interleaved`
- 完了条件: build・clippy・既存テスト成功

### Task 2.4 Ogg Vorbis デコーダ（symphonia）
- ファイル: `core/Cargo.toml`（symphonia 追加）, `core/src/decode/vorbis.rs`（+ `decode/mod.rs` の振り分け）
- 型: `VorbisDecoder { format: Box<dyn FormatReader>, decoder: Box<dyn AudioDecoder>, track_id: u32, info: StreamInfo, pending: Vec<f32>, pending_pos: usize }`
- read: `pending` の残りを先に buf へコピー。空なら `next_packet` → `None` は EOF(0)、他トラックはスキップ、`decode` → `pending.resize(samples_interleaved, 0.0)` → `copy_to_slice_interleaved`。`Error::DecodeError` はそのパケットをスキップ、その他のエラーは `DecodeError::Codec(e.to_string())`
- `sample_rate` / `channels` が取れなければ `Unsupported`
- 完了条件: build・clippy・既存テスト成功

### Task 2.5 再生を Decoder 経由に切り替え + 全フォーマットテスト
- ファイル: `core/src/playback.rs`, `core/tests/formats.rs`
- 内容: `play_file_blocking` を `decode::open` 経由に変更（`PlaybackError` に `Decode(#[from] DecodeError)` 追加）
- テスト: 段階1・2 の全テスト音源（FLAC 2, WAV, AIFF, MP3, Ogg, ALAC, AAC, HE-AAC）を `decode::open` で開き、2ch・期待レート、read 合計フレームが 期待値 ±3000 フレーム（MP3/AAC の priming 分の誤差を許容。厳密化は段階3）。テスト音源が無ければ skip
- 完了条件: 全テスト・clippy 成功。`play` サンプルで ogg と heaac が約 3 秒で exit 0

---

## 段階3: ギャップレス再生

調査結果（2026-09-26、実ファイルで計測）:
- ExtAudioFile は AAC/HE-AAC（m4a の priming/remainder 情報）と MP3 の LAME タグ（encoder delay/padding）を読み、
  **デコード時点で正確にトリムする**（`lame -b 192` / `-V 2` とも 132300 フレーム丁度。afinfo: `132300 valid frames + 576 priming + 756 remainder`）
- symphonia の Vorbis も 132300 丁度
- sox で作った MP3 は Xing/LAME タグが無く、原理的にトリム不能（+1332）。テスト音源は lame で作り直す
- よって段階3でトリム処理の自前実装は不要。やることは「テスト厳密化」と「複数曲を途切れなく流す再生ループ」

再生ループの方針:
- 1 本のデコードループが曲を順に処理し、同じリングバッファ・同じ OutputUnit に書き続ける（曲間で止めない）
- 次曲のデコーダは現在曲の再生開始時に先に open しておく（open の遅延で途切れないように）
- 次曲の `StreamInfo`（sample_rate, channels）が現在と異なる場合のみ、リングが空になるまで待って OutputUnit を作り直す（この場合の隙間は許容）
- 開けない・読めない曲はスキップして次へ進み、失敗リストとして返す（落ちない）

### Task 3.1 テスト音源: lame 版 MP3 とギャップレス用ペア
- ファイル: `scripts/gen-test-audio.sh`
- 内容:
  - 既存の `sox testdata/_src_44k16.wav testdata/sine440.mp3` 行を `lame --quiet -b 192 testdata/_src_44k16.wav testdata/sine440.mp3` に置き換え（lame が無ければ brew install lame を案内して exit 1）
  - `testdata/_src6s_44k16.wav`: `sox -n -r 44100 -b 16 -c 2 ... synth 6.0 sine 440 gain -6`
  - 2 分割: `sox _src6s_44k16.wav testdata/_gap_a.wav trim 0 132300s` と `sox _src6s_44k16.wav testdata/_gap_b.wav trim 132300s`
  - 各パートを変換: `gap_a.flac`/`gap_b.flac`（sox）, `gap_a.mp3`/`gap_b.mp3`（lame -b 192）, `gap_a.m4a`/`gap_b.m4a`（afconvert -f m4af -d aac -b 192000）, `gap_a.ogg`/`gap_b.ogg`（sox）
- 完了条件: 生成物が揃い、`afinfo testdata/sine440.mp3` に `132300 valid frames` が出る

### Task 3.2 テスト厳密化とつなぎ目テスト
- ファイル: `core/tests/formats.rs`, `core/tests/gapless.rs`（新規）
- formats.rs: 許容誤差 3000 → 0（全フォーマットでフレーム数完全一致）
- gapless.rs: 各フォーマット（flac, mp3, m4a, ogg）について `gap_a.*` → `gap_b.*` の順に `decode::open` して全サンプルを 1 本の Vec<f32> に連結し、
  - 総フレーム数 == 264600
  - つなぎ目（フレーム 132300）前後 ±2048 フレームで、隣接サンプルの差の絶対値の最大が 0.1 未満（440Hz・-6dB サインの隣接差は最大約 0.031。隙間やクリックがあると大きくなる）
  - FLAC のみ追加: `_src6s_44k16.wav` をデコードした参照と全サンプルで差の最大が 1e-4 未満
- 完了条件: 全テスト・clippy 成功

### Task 3.3 複数曲のギャップレス再生ループ
- ファイル: `core/src/playback.rs`
- 型:
  ```rust
  pub struct TrackFailure { pub path: PathBuf, pub error: PlaybackError }
  /// paths を順に途切れなく再生する。出力デバイスを開けない等の致命的エラーのみ Err。
  /// 開けない/読めない曲はスキップして TrackFailure に積む
  pub fn play_files_blocking(paths: &[PathBuf]) -> Result<Vec<TrackFailure>, PlaybackError>;
  ```
  - 既存 `play_file_blocking(path)` は `play_files_blocking(&[path.to_path_buf()])` を呼び、失敗が 1 件あればそのエラーを返す薄いラッパーにする
- 完了条件: build・clippy・全テスト成功

### Task 3.4 CLI を複数ファイル対応に
- ファイル: `core/examples/play.rs`
- 内容: `play <file>...`（1 個以上）。`play_files_blocking` を使い、スキップした曲は stderr に表示。致命的エラーは exit 1、引数なしは exit 2
- 完了条件: `play testdata/gap_a.flac testdata/gap_b.flac` が約 6 秒で exit 0（つなぎ目の音切れの有無は耳で確認）
