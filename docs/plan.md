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

---

## 段階4: 出力デバイス列挙・切り替え、抜き差し対応

方針:
- デバイス未指定: 従来どおり `kAudioUnitSubType_DefaultOutput`（システムのデフォルト出力変更に AU 自体が追従する）
- デバイス指定: `kAudioUnitSubType_HALOutput` + `kAudioOutputUnitProperty_CurrentDevice` で固定
- システムオブジェクトの `Devices` / `DefaultOutputDevice` 変化を監視し、指定デバイスが消えたらデフォルトにフォールバックして出力を作り直す（リング内の最大 0.5 秒は失われてよい）
- リスナーのコールバックは Core Audio の通知スレッドで呼ばれる（レンダースレッドではない）。中では `mpsc::Sender::send` だけ行う

### 依存の追加（段階4、確認済み 2026-09-26）
| crate | version | features |
|---|---|---|
| objc2-core-audio | 0.3.2 | default-features = false, `std`, `AudioHardware`, `objc2-core-audio-types`, `objc2-core-foundation` |

参照した API（objc2-core-audio 0.3.2）:
- `AudioObjectID = u32`, `kAudioObjectSystemObject: c_int = 1`（u32 にキャスト）, `kAudioObjectUnknown = 0`
- `AudioObjectPropertyAddress { mSelector, mScope, mElement }`
- `AudioObjectGetPropertyDataSize(id, NonNull<Address>, 0, null, NonNull<u32>) -> OSStatus`
- `AudioObjectGetPropertyData(id, NonNull<Address>, 0, null, NonNull<u32> io_size, NonNull<c_void>) -> OSStatus`
- `AudioObjectAddPropertyListener(id, NonNull<Address>, AudioObjectPropertyListenerProc, *mut c_void) -> OSStatus` / `AudioObjectRemovePropertyListener`（同じ引数）
- `AudioObjectPropertyListenerProc = Option<unsafe extern "C-unwind" fn(AudioObjectID, u32, NonNull<AudioObjectPropertyAddress>, *mut c_void) -> OSStatus>`
- セレクタ: `kAudioHardwarePropertyDevices`, `kAudioHardwarePropertyDefaultOutputDevice`, `kAudioDevicePropertyStreamConfiguration`（出力 scope、`AudioBufferList` 可変長。各 `mBuffers[i].mNumberChannels` の合計がチャンネル数）, `kAudioObjectPropertyName` / `kAudioDevicePropertyDeviceUID`（`CFStringRef` を返す。呼び出し側が所有 → `CFRetained::from_raw` で受け、`to_string()`）, `kAudioDevicePropertyTransportType`（u32）, `kAudioDevicePropertyNominalSampleRate`（f64）, `kAudioDevicePropertyDeviceIsAlive`（u32）
- スコープ/要素: `kAudioObjectPropertyScopeGlobal`, `kAudioObjectPropertyScopeOutput`, `kAudioObjectPropertyElementMain`
- `kAudioDeviceTransportTypeBluetooth` / `kAudioDeviceTransportTypeBluetoothLE`（段階12 のビットパーフェクト判定で使う）
- AudioToolbox: `kAudioOutputUnitProperty_CurrentDevice`（scope Global, element 0, 値は AudioObjectID）

### Task 4.1 デバイス列挙
- ファイル: `core/Cargo.toml`, `core/src/coreaudio/device.rs`（+ `mod.rs`）, `core/tests/device.rs`
- 型:
  ```rust
  pub type DeviceId = u32;
  #[derive(Debug, Clone, PartialEq)]
  pub struct DeviceInfo { pub id: DeviceId, pub uid: String, pub name: String,
      pub output_channels: u32, pub nominal_sample_rate: f64, pub transport_type: u32 }
  impl DeviceInfo { pub fn is_bluetooth(&self) -> bool; }
  /// 出力チャンネルが 1 以上のデバイスのみ
  pub fn output_devices() -> Result<Vec<DeviceInfo>>;
  pub fn default_output_device() -> Result<DeviceId>;
  pub fn find_output_device_by_uid(uid: &str) -> Result<Option<DeviceInfo>>;
  pub fn is_alive(id: DeviceId) -> bool;   // 取得失敗は false
  ```
  - 個々のデバイスで名前等の取得に失敗しても列挙全体を失敗させず、そのデバイスをスキップ
- 完了条件: テスト — `output_devices()` が 1 件以上（0 件なら skip）で全件 output_channels > 0・uid 非空、`default_output_device()` の ID が一覧に含まれる、`find_output_device_by_uid` が一覧の先頭の uid で見つかり、存在しない uid で None、`is_alive(0)` が false。全テスト・clippy 成功

### Task 4.2 OutputUnit のデバイス指定
- ファイル: `core/src/coreaudio/output.rs`, `core/src/playback.rs`, `core/tests/output_unit.rs`
- 内容: `OutputUnit::new(info, consumer, device: Option<DeviceId>)`。`Some(id)` なら HALOutput を探し、`configure` の前に `kAudioOutputUnitProperty_CurrentDevice` を設定。`None` は従来どおり DefaultOutput。既存呼び出しは `None` を渡す
- 完了条件: テスト追加 — `default_output_device()` の ID を明示指定して new/start/stop 成功。全テスト・clippy 成功

### Task 4.3 デバイス変化の監視
- ファイル: `core/src/coreaudio/device_watch.rs`（+ `mod.rs`）, `core/tests/device_watch.rs`
- 型:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum DeviceEvent { DevicesChanged, DefaultOutputChanged }
  pub struct DeviceWatcher { sender: *mut std::sync::mpsc::Sender<DeviceEvent> }
  impl DeviceWatcher { pub fn new() -> Result<(Self, std::sync::mpsc::Receiver<DeviceEvent>)>; }
  impl Drop  // 両リスナーを Remove してから Box::from_raw(sender)
  ```
  - システムオブジェクトの `Devices` と `DefaultOutputDevice`（scope Global, element Main）に同じコールバックを登録し、`in_addresses` の各 `mSelector` を見てイベントを送る。送信失敗は無視
  - 2 つ目の登録に失敗したら 1 つ目を Remove し、Box を解放して Err
- 完了条件: テスト — new → drop がエラー・クラッシュなし。全テスト・clippy 成功（実際の抜き差しは手動確認）

### Task 4.4 再生ループへの組み込み
- ファイル: `core/src/playback.rs`
- 内容: `pub struct PlaybackOptions { pub device_uid: Option<String> }`（Default 実装）と
  `pub struct PlaybackReport { pub failures: Vec<TrackFailure>, pub device_fallback: bool }`。
  `play_files_blocking(paths, &PlaybackOptions) -> Result<PlaybackReport, PlaybackError>` に変更（`play_file_blocking` はデフォルト値で呼ぶ）
  - 開始時: uid 指定があれば `find_output_device_by_uid`。見つからなければデフォルトで再生し `device_fallback = true`（ライブラリ内で print しない）
  - `DeviceWatcher` を再生中保持し、`stream` のループ内で `try_recv` を確認。`DevicesChanged` かつ指定デバイスが `is_alive` でない → デバイス指定を捨ててデフォルトで出力を作り直す（新しいリングを作り、以降はそこへ書く）。この場合も `device_fallback = true`
- 完了条件: build・clippy・全テスト成功

### Task 4.5 CLI のデバイス対応
- ファイル: `core/examples/play.rs`
- 内容: `play --list-devices`（`* ` 印でデフォルト、`uid  name  ch  rate  [BT]` を 1 行ずつ）、`play --device <uid> <file>...`
- 内容（追記）: `device_fallback` が true なら stderr に「指定デバイスが見つからない/外れたためデフォルト出力で再生」と表示
- 完了条件: `--list-devices` が一覧を表示して exit 0、存在しない uid 指定でもデフォルトで再生して exit 0

---

## 段階5: ライブラリ（スキャン・タグ・DB・差分・監視）

方針:
- ファイルのタグが正。DB（SQLite）はキャッシュで、消しても再スキャンで完全に復元できる（プレイリストは段階8で別ファイル）
- モジュール: `core/src/library/{mod.rs, error.rs, tags.rs, db.rs, scan.rs, scanner.rs, watch.rs}`
- 差分判定は (mtime ナノ秒, サイズ)。変化したファイルだけタグを読み直す
- 監視フォルダが存在しない（外付けドライブが外れた等）ときは、そのフォルダの曲を **削除しない**（`folder_missing` として報告）
- スキャンと監視はバックグラウンドスレッド。各スレッドは自分の `Connection` を持つ（SQLite は WAL モード）
- 対象拡張子（小文字比較）: flac, m4a, mp3, aac, wav, aif, aiff, ogg, oga
- アルバムのグループ化キー: `album_artist`（空なら `artist`）+ `album`。並び: disc_no → track_no
- タグが無い・壊れている: 落とさない。title はファイル名（拡張子なし）、他は空/None
- DB の保存先は呼び出し側が決める（アプリ名未定のため。段階6 で `~/Library/Application Support/<アプリ名>/` を決める）

### 依存の追加（段階5、確認済み 2026-09-26）
| crate | version | features |
|---|---|---|
| lofty | 0.25.4 | default |
| rusqlite | 0.40.2 | `bundled` |
| notify | 8.2.0（=8.2.0 で固定。9.x は rc のため使わない） | default（macos_fsevent） |
| walkdir | 2.5.0 | default |
| tempfile | 3.27.0 | **dev-dependencies のみ** |

参照した API:
- lofty 0.25.4: `lofty::read_from_path(path) -> Result<TaggedFile, _>`, `use lofty::prelude::*`（`AudioFile, TaggedFileExt, Accessor, ItemKey`）,
  `tagged.primary_tag().or_else(|| tagged.first_tag())`, `Accessor`: `title() / artist() / album() / genre() -> Option<Cow<str>>`, `track() / disk() -> Option<u32>`, `date() -> Option<Timestamp>`（`Timestamp { year: u16, .. }`）,
  `tag.get_string(ItemKey::AlbumArtist) -> Option<&str>`, `tagged.properties()`: `duration() -> Duration`, `sample_rate() / audio_bitrate() -> Option<u32>`, `bit_depth() / channels() -> Option<u8>`
- rusqlite 0.40.2: `Connection::open(path) / open_in_memory()`, `execute_batch`, `execute(sql, params![..])`, `query_row`, `prepare` + `query_map`, `transaction()`, `pragma_update(None, "journal_mode", "WAL")`, `OptionalExtension::optional()`
- notify 8.2.0: `notify::recommended_watcher(tx)`（`std::sync::mpsc::Sender<notify::Result<Event>>` が EventHandler）, `watcher.watch(&path, RecursiveMode::Recursive)`, `Event { kind, paths }`

### Task 5.1 タグ付きテスト音源
- ファイル: `scripts/gen-test-audio.sh`
- 追記: `testdata/lib/` 以下に次を生成（`_src_44k16.wav` から、各 1 秒に trim してよい）
  - `testdata/lib/AlbumA/01.flac`: sox で `--add-comment` により TITLE=一曲目, ARTIST=テスト歌手, ALBUMARTIST=テスト歌手, ALBUM=テストアルバム, TRACKNUMBER=1, DISCNUMBER=1, DATE=2024, GENRE=Rock
  - `testdata/lib/AlbumA/02.mp3`: lame `--tt 二曲目 --ta テスト歌手 --tl テストアルバム --tn 2 --ty 2024 --tg Rock --id3v2-only`（Album Artist は無し → Artist で代用される想定）
  - `testdata/lib/AlbumB/track.ogg`: sox `--add-comment` で TITLE=Ogg Song, ARTIST=Other, ALBUM=Album B, TRACKNUMBER=3
  - `testdata/lib/notag.flac`: タグ無し
  - `testdata/lib/broken.mp3`: `printf 'not audio' >` で作る壊れたファイル
  - `testdata/lib/readme.txt`: 対象外拡張子
  - `testdata/long_60s.flac`: 60 秒・440Hz・gain -12（手動テスト用）
- 完了条件: 生成物が揃う（`.gitignore` 済みの拡張子。`readme.txt` は testdata/ ごと ignore されていなければ `.gitignore` に `testdata/lib/readme.txt` を追加）

### Task 5.2 タグ読み取り
- ファイル: `core/Cargo.toml`（依存追加）, `core/src/library/mod.rs`, `core/src/library/error.rs`, `core/src/library/tags.rs`（+ `lib.rs` に `pub mod library;`）, `core/tests/tags.rs`
- 型:
  ```rust
  #[derive(Debug, thiserror::Error)]
  pub enum LibraryError { #[error(transparent)] Db(#[from] rusqlite::Error), #[error(transparent)] Io(#[from] std::io::Error),
      #[error("tag: {0}")] Tag(String), #[error("watch: {0}")] Watch(String) }
  #[derive(Debug, Clone, Default, PartialEq)]
  pub struct TrackMeta { pub title: String, pub artist: String, pub album: String, pub album_artist: String, pub genre: String,
      pub track_no: Option<u32>, pub disc_no: Option<u32>, pub year: Option<u32>,
      pub duration_ms: u64, pub sample_rate: Option<u32>, pub bit_depth: Option<u8>, pub channels: Option<u8>, pub bitrate_kbps: Option<u32> }
  impl TrackMeta { /// album_artist が空なら artist
      pub fn effective_album_artist(&self) -> &str; }
  pub fn read_meta(path: &Path) -> Result<TrackMeta, LibraryError>;
  ```
  - lofty が読めない → `Err(LibraryError::Tag(..))`。タグが無いだけなら Ok で title = ファイル名（拡張子なし）
- 完了条件: テスト — AlbumA/01.flac の全項目（日本語含む）、02.mp3 の album_artist が空で effective が「テスト歌手」、notag.flac の title が "notag"、broken.mp3 が Err（panic しない）、duration_ms が 1000±50

### Task 5.3 DB
- ファイル: `core/src/library/db.rs`（テストはファイル内 `#[cfg(test)]` で `open_in_memory`）
- スキーマ（`PRAGMA user_version = 1`）:
  - `folders(id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE)`
  - `tracks(id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE, folder_id INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE, mtime_ns INTEGER NOT NULL, size INTEGER NOT NULL, title TEXT NOT NULL, artist TEXT NOT NULL, album TEXT NOT NULL, album_artist TEXT NOT NULL, album_key TEXT NOT NULL, genre TEXT NOT NULL, track_no INTEGER, disc_no INTEGER, year INTEGER, duration_ms INTEGER NOT NULL, sample_rate INTEGER, bit_depth INTEGER, channels INTEGER, bitrate_kbps INTEGER)`
  - インデックス: `tracks(folder_id)`, `tracks(album_key, disc_no, track_no)`, `tracks(artist)`, `tracks(title)`, `tracks(genre)`
  - `album_key = effective_album_artist + "\u{1f}" + album`
- 型:
  ```rust
  pub struct Library { conn: rusqlite::Connection }
  #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct FileState { pub mtime_ns: i64, pub size: i64 }
  impl Library {
      pub fn open(path: &Path) -> Result<Self, LibraryError>;   // WAL, foreign_keys=ON, スキーマ作成
      pub fn open_in_memory() -> Result<Self, LibraryError>;
      pub fn add_folder(&self, path: &Path) -> Result<i64, LibraryError>;   // 既存なら既存 id
      pub fn remove_folder(&self, path: &Path) -> Result<(), LibraryError>; // 配下の tracks も消える
      pub fn folders(&self) -> Result<Vec<(i64, PathBuf)>, LibraryError>;
      pub fn file_states(&self, folder_id: i64) -> Result<HashMap<PathBuf, FileState>, LibraryError>;
      pub fn upsert_track(&self, folder_id: i64, path: &Path, state: FileState, meta: &TrackMeta) -> Result<(), LibraryError>;
      pub fn remove_track(&self, path: &Path) -> Result<(), LibraryError>;
      pub fn track_count(&self) -> Result<u64, LibraryError>;
      pub fn transaction<T>(&mut self, f: impl FnOnce(&Library) -> Result<T, LibraryError>) -> Result<T, LibraryError>; // BEGIN..COMMIT、Err なら ROLLBACK
  }
  ```
- 完了条件: テスト — フォルダ追加の冪等性、upsert 2 回で行数 1・内容更新、file_states の往復、remove_folder で曲も消える、同じ DB ファイルを開き直してもスキーマ作成が失敗しない（tempfile 使用）

### Task 5.4 差分スキャン
- ファイル: `core/src/library/scan.rs`, `core/tests/scan.rs`
- 型:
  ```rust
  #[derive(Debug, Default, Clone, PartialEq, Eq)]
  pub struct ScanStats { pub added: u32, pub updated: u32, pub removed: u32, pub unchanged: u32, pub errors: u32, pub folder_missing: bool }
  pub fn is_audio_path(path: &Path) -> bool;
  /// folder を走査して DB を差分更新。1 トランザクション
  pub fn scan_folder(lib: &mut Library, folder: &Path) -> Result<ScanStats, LibraryError>;
  ```
  - folder が存在しない/ディレクトリでない → DB に触らず `folder_missing: true`
  - タグ読み取り失敗のファイル: `errors += 1`、DB には入れない（既存行があれば残す）
  - 走査中に見つからなかった既存行は削除（`removed`）
- 完了条件: テスト（testdata/lib を tempdir にコピーして使う）— 初回 added=4・errors=1（broken.mp3）、2 回目 unchanged=4・added=0、1 ファイルを書き換え（内容を別ファイルで上書き）→ updated=1、1 ファイル削除 → removed=1、存在しないフォルダ → folder_missing かつ曲数不変

### Task 5.5 バックグラウンドスキャナ
- ファイル: `core/src/library/scanner.rs`, `core/tests/scanner.rs`
- 型:
  ```rust
  pub enum ScanEvent { Started { folder: PathBuf }, Finished { folder: PathBuf, stats: ScanStats }, Failed { folder: PathBuf, error: String }, AllDone }
  pub struct ScanHandle { pub events: Receiver<ScanEvent>, join: JoinHandle<()> }
  impl ScanHandle { pub fn wait(self); }
  /// db_path を別スレッドで開き、Library の全登録フォルダを順にスキャン
  pub fn scan_all_in_background(db_path: PathBuf) -> ScanHandle;
  ```
- 完了条件: テスト — tempdir の DB にフォルダ登録 → `scan_all_in_background` → AllDone まで受信、Finished の stats が期待どおり、別 Connection から track_count が 4

### Task 5.6 フォルダ監視
- ファイル: `core/src/library/watch.rs`, `core/tests/watch.rs`
- 型:
  ```rust
  pub struct LibraryWatcher { .. }   // notify watcher と処理スレッドを保持。Drop で停止
  impl LibraryWatcher {
      /// 登録フォルダを監視し、変化があったフォルダを「最後のイベントから debounce 経過後」に scan_folder。
      /// 結果は ScanEvent で送る。存在しないフォルダは監視登録をスキップ（落ちない）
      pub fn start(db_path: PathBuf, debounce: Duration) -> Result<(Self, Receiver<ScanEvent>), LibraryError>;
  }
  ```
- 完了条件: テスト — tempdir にフォルダ登録・初回スキャン後、watcher を debounce 300ms で開始し、新しい flac をコピー → 10 秒以内に Finished（added=1）を受信

### Task 5.7 CLI サンプル
- ファイル: `core/examples/lib.rs`
- 内容: `lib <db> add <folder>` / `lib <db> scan`（バックグラウンドスキャンの進捗を表示）/ `lib <db> stats`（フォルダ一覧と曲数）/ `lib <db> watch`（Ctrl+C まで監視し、結果を表示）
- 完了条件: testdata/lib で add → scan → stats が期待どおり

---

## 段階6: egui UI（サイドバー + アルバム一覧 + 曲一覧（仮想化）+ 検索）

方針:
- 新クレート `app/`（package `mmp-app`、bin 名 `macmusicplay`）。UI 非依存の問い合わせ・パスは core 側に置く
- 表示用データは起動時・スキャン完了時に DB から `Vec` に読み込み、表の描画は egui_extras `TableBuilder` の `body.rows()` で**見えている行だけ**描く
- 検索は SQL の `LIKE`（title / artist / album / album_artist）。数万曲なら十分速い。入力のたびではなく 200ms 入力が止まってから再検索
- 起動時にバックグラウンドで全フォルダを差分スキャンし、同時に `LibraryWatcher` で監視。`ScanEvent::Finished` を受けたら一覧を読み直す
- 再生は段階7。段階6では選択まで
- 日本語フォント: 起動時にシステムフォントを読み込む（同梱しない）。候補順: `/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc`, `/System/Library/Fonts/Hiragino Sans GB.ttc`, `/Library/Fonts/Arial Unicode.ttf`。見つからなくても落ちない
- 画面構成は docs/gui-mockup.webp（左: LIBRARY / GENRES / フォルダ、右: 一覧。上: 検索欄）

### 依存の追加（段階6、確認済み 2026-09-26）
| crate | version | 用途 |
|---|---|---|
| eframe | 0.36.2 | ウィンドウ・egui 実行（app のみ） |
| egui_extras | 0.36.2 | TableBuilder（app のみ） |
| rfd | 0.17.2 | フォルダ選択ダイアログ（app のみ） |

参照した API（egui/eframe 0.36.2 — 0.35 以前と異なる点に注意）:
- `eframe::run_native(app_name, NativeOptions, Box::new(|cc| Ok(Box::new(MyApp::new(cc)))))`
- **`eframe::App` の必須メソッドは `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame)`**（旧 `update(ctx, frame)` ではない）。
  描画しない処理は任意の `fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame)` に書く
- パネル: `egui::Panel::left("id")` / `Panel::top("id")` … `.show(ui, |ui| ..)`（`show_inside` は非推奨名）。中央は `egui::CentralPanel::default().show(ui, |ui| ..)`
- フォント: `ctx.add_font(epaint::text::FontInsert { name, data: FontData::from_owned(bytes)（.index で ttc 内の番号）, families: vec![InsertFontFamily { family: FontFamily::Proportional, priority: FontPriority::Lowest }, (Monospace も同様)] })`
  - `FontData { font, index: u32, tweak }`
- 表: `TableBuilder::new(ui).striped(true).sense(Sense::click()).column(Column::initial(w).resizable(true))…column(Column::remainder()).header(h, |mut header| { header.col(|ui| ..); }).body(|body| body.rows(row_h, n, |mut row| { let i = row.index(); row.set_selected(..); row.col(|ui| ..); if row.response().clicked() {..} }))`
- 再描画: `ctx.request_repaint_after(Duration)`
- rfd: `rfd::FileDialog::new().set_title(..).pick_folder() -> Option<PathBuf>`（メインスレッドで呼ぶ。egui の ui() 内で可）

### Task 6.1 ライブラリ問い合わせ API
- ファイル: `core/src/library/query.rs`（+ `mod.rs`）
- 型:
  ```rust
  #[derive(Debug, Clone, PartialEq)]
  pub struct TrackRow { pub id: i64, pub path: PathBuf, pub title: String, pub artist: String, pub album: String,
      pub album_artist: String, pub album_key: String, pub genre: String, pub track_no: Option<u32>, pub disc_no: Option<u32>,
      pub year: Option<u32>, pub duration_ms: u64, pub sample_rate: Option<u32>, pub bit_depth: Option<u8>, pub bitrate_kbps: Option<u32> }
  impl TrackRow { /// 例: "24-bit/96kHz FLAC", "320kbps MP3", "44.1kHz AAC"（拡張子を大文字化。m4a は bit_depth があれば ALAC、無ければ AAC）
      pub fn format_label(&self) -> String; }
  #[derive(Debug, Clone, PartialEq)]
  pub struct AlbumRow { pub album_key: String, pub album: String, pub album_artist: String, pub year: Option<u32>, pub track_count: u32, pub duration_ms: u64 }
  #[derive(Debug, Clone, Default, PartialEq)]
  pub struct TrackFilter { pub album_key: Option<String>, pub artist: Option<String>, pub genre: Option<String>, pub search: Option<String> }
  impl Library {
      pub fn list_tracks(&self, f: &TrackFilter) -> Result<Vec<TrackRow>, LibraryError>;
      pub fn list_albums(&self, search: Option<&str>) -> Result<Vec<AlbumRow>, LibraryError>;
      pub fn list_artists(&self) -> Result<Vec<(String, u32)>, LibraryError>;   // (artist, 曲数)
      pub fn list_genres(&self) -> Result<Vec<(String, u32)>, LibraryError>;    // 空文字は除く
  }
  ```
  - 並び: album_key 指定時は `disc_no, track_no, title`。それ以外は `album_artist(空なら artist), album, disc_no, track_no`（`COLLATE NOCASE`、NULL は最後）
  - アルバム: `GROUP BY album_key`、album_artist は effective（`CASE WHEN album_artist = '' THEN artist ELSE album_artist END` の MAX）、year は MAX、並びは album_artist, album
  - 検索: `%q%` の LIKE（`ESCAPE '\'` で `%` `_` `\` をエスケープ）。title / artist / album / album_artist の OR。空白区切りの複数語は AND
- 完了条件: `#[cfg(test)]` で open_in_memory + upsert したデータに対し — 並び順、album_key 絞り込み、検索（日本語・大文字小文字・`%` を含む語・複数語 AND）、list_albums の件数と track_count、genre の空除外、format_label の各形式。全テスト・clippy 成功

### Task 6.2 データ保存先
- ファイル: `core/src/paths.rs`（+ `lib.rs`）
- 内容: `pub const APP_NAME: &str = "MacMusicPlay"; pub fn data_dir() -> Result<PathBuf, std::io::Error>`（`$HOME/Library/Application Support/MacMusicPlay` を create_dir_all して返す。HOME 未設定は Err）、`pub fn library_db_path() -> Result<PathBuf, std::io::Error>`（data_dir/library.db）
- 完了条件: テスト（HOME を一時ディレクトリに差し替えて作成されることを確認。テストは `serial` 不要なよう 1 関数にまとめる）、clippy 成功

### Task 6.3 app クレート骨格と日本語フォント
- ファイル: ルート `Cargo.toml`（members に "app"）, `app/Cargo.toml`, `app/src/main.rs`, `app/src/fonts.rs`
- 内容: ウィンドウ（タイトル MacMusicPlay、1200x760、最小 800x500）。`fonts::install_cjk_fonts(&egui::Context) -> Option<PathBuf>`（読み込んだフォントのパス）。main は CentralPanel に「MacMusicPlay」と「日本語表示テスト：一曲目」を表示するだけ
- 完了条件: `cargo build -p mmp-app`、clippy 成功（起動して日本語が表示されるかは指揮役/ユーザーが確認）

### Task 6.4 アプリ状態とバックグラウンドスキャン
- ファイル: `app/src/state.rs`, `app/src/main.rs`
- 型:
  ```rust
  pub enum Nav { AllTracks, Albums, Album(String /*album_key*/), Artists, Artist(String), Genres, Genre(String) }
  pub struct AppState { pub lib: Library, pub db_path: PathBuf, pub nav: Nav, pub search: String, search_changed_at: Option<Instant>,
      pub tracks: Vec<TrackRow>, pub albums: Vec<AlbumRow>, pub artists: Vec<(String,u32)>, pub genres: Vec<(String,u32)>, pub folders: Vec<(i64, PathBuf)>,
      pub selected_track: Option<usize>, pub scanning: bool, pub status: String,
      scan_rx: Option<Receiver<ScanEvent>>, watch: Option<(LibraryWatcher, Receiver<ScanEvent>)> }
  impl AppState {
      pub fn open() -> Result<Self, String>;       // library_db_path → Library::open → reload → scan_all_in_background 開始
      pub fn reload(&mut self);                     // nav と search に応じて tracks/albums/…を読み直す（エラーは status に）
      pub fn set_nav(&mut self, nav: Nav); pub fn set_search(&mut self, s: String);   // search は 200ms 後に reload
      pub fn add_folder(&mut self, p: PathBuf); pub fn remove_folder(&mut self, p: &Path);  // 追加後はスキャン開始
      pub fn poll(&mut self, ctx: &egui::Context);  // scan/watch イベント処理・遅延検索。scanning 中は request_repaint_after(250ms)
  }
  ```
  - スキャン完了（AllDone）後に watcher を（作り直して）開始。フォルダ追加・削除時も作り直す
- 完了条件: build・clippy 成功。main の `logic()` で `poll`、`ui()` はまだ簡易表示（status と曲数）

### Task 6.5 サイドバー
- ファイル: `app/src/ui/mod.rs`, `app/src/ui/sidebar.rs`
- 内容: `Panel::left` に LIBRARY（全曲 / アーティスト / アルバム / ジャンル）、GENRES（list_genres 上位を列挙し、クリックで Nav::Genre）、FOLDERS（登録フォルダ名 + 「−」で削除、「フォルダを追加…」で rfd）。選択中の項目はハイライト。スキャン中は下部に「スキャン中…」
- 完了条件: build・clippy 成功

### Task 6.6 曲一覧（仮想化テーブル）
- ファイル: `app/src/ui/tracks.rs`
- 内容: 列 `# / タイトル / 長さ / アーティスト / アルバム / ジャンル / フォーマット`。`body.rows(row_h, state.tracks.len(), ..)` で可視行のみ描画。クリックで選択（`set_selected`）。長さは `m:ss`。空のときは「曲がありません。フォルダを追加してください」
- 完了条件: build・clippy 成功

### Task 6.7 アルバム一覧・アルバム詳細・アーティスト一覧
- ファイル: `app/src/ui/albums.rs`
- 内容: Nav::Albums は `AlbumRow` の仮想化テーブル（アルバム / アーティスト / 年 / 曲数 / 長さ）、クリックで Nav::Album(key)。Nav::Album は上部にアルバム名・アーティスト・「年 • n 曲 • 合計時間」、下に tracks テーブル（6.6 の関数を再利用）。Nav::Artists は (artist, 曲数) のテーブルでクリックで Nav::Artist
- 完了条件: build・clippy 成功

### Task 6.8 検索欄と全体の組み立て
- ファイル: `app/src/ui/toolbar.rs`, `app/src/main.rs`
- 内容: `Panel::top` に検索欄（プレースホルダ「検索」、⌘F でフォーカス、Esc でクリア）と、右端に曲数/ステータス表示。main の `ui()` で toolbar → sidebar → CentralPanel（nav に応じて tracks/albums/artists）を組み立てる
- 追加: `app/src/state.rs` 先頭の `#![allow(dead_code)]`（6.4 で一時的に付けたもの）を削除し、未使用の項目があれば使うか削除する
- 完了条件: build・clippy 成功。testdata/lib を追加した状態で起動し、全曲・アルバム・ジャンル・検索（「テスト」「ogg」）が機能すること（指揮役/ユーザーが確認）

### 段階6 追加調整（ユーザーフィードバック 2026-09-26: 文字に対して余白が足りない / UI は英語 / docs/gui-mockup.webp に寄せる）
方針:
- UI 文言はすべて英語。サイドバー項目はモックどおり小文字（`all tracks` / `artists` / `albums` / `genres`）、セクション見出しは大文字（`LIBRARY` / `GENRES` / `FOLDERS`）
- 余白: 本文 14pt、見出し 26pt、小 11pt。`item_spacing = (10, 8)`、`button_padding = (10, 5)`、`interact_size.y = 26`。表の行高 34、ヘッダー 30。サイドバー幅 230・内側余白 14、項目の高さ 30。中央パネルの内側余白 16〜24
- プレイヤーバー・PLAYLISTS・アートワーク・テーマ切り替えは各段階（7/8/10/11）でモックに合わせて作る
- egui 0.36: 全テーマのスタイル変更は `ctx.all_styles_mut(|s: &mut egui::Style| { s.text_styles.insert(TextStyle::Body, FontId::proportional(14.0)); s.spacing.item_spacing = vec2(..); … })`、パネルの余白は `.frame(egui::Frame::side_top_panel(&ui.style()).inner_margin(14.0))` / `egui::Frame::central_panel(..)`（無ければ grep で確認）

### Task 6.9 スタイル（文字サイズ・余白）
- ファイル: `app/src/style.rs`（新規）, `app/src/main.rs`（`mod style;` と creator で `style::apply(&cc.egui_ctx)` を呼ぶ）
- 内容: 上記の文字サイズ・spacing を `all_styles_mut` で設定。`pub const ROW_HEIGHT: f32 = 34.0; pub const HEADER_HEIGHT: f32 = 30.0; pub const SIDEBAR_WIDTH: f32 = 230.0; pub const PANEL_MARGIN: f32 = 14.0;` を公開
- 完了条件: build・clippy 成功

### Task 6.10 サイドバー・ツールバー・状態メッセージの英語化と余白
- ファイル: `app/src/ui/sidebar.rs`, `app/src/ui/toolbar.rs`, `app/src/state.rs`（status 文言のみ）
- 文言: `LIBRARY` / `all tracks` / `artists` / `albums` / `genres`、`GENRES`、`FOLDERS`、`Add folder…`、`Remove from library (files are not deleted)`、`Choose a music folder`、`Scanning…`、検索 `Search`、ステータス `{n} tracks` / `{n} albums` / `{n} artists` / `{n} genres`、state の `Scanning: {folder}` / `Could not add folder: {e}` / `Could not remove folder: {e}` / `Load error: {e}` / `Could not start watching: {e}`
- サイドバー: 幅 `SIDEBAR_WIDTH`（`default_size` 等の名前は grep で確認）、`PANEL_MARGIN` の内側余白。セクション見出しの前に 16 の空き、項目は `selectable_label` を `ui.add_sized([ui.available_width(), 30.0], ..)` で横幅いっぱい・高さ 30。GENRES は `Rock  12` のように件数を weak 色で
- ツールバー: 上下 8・左右 16 の余白、検索欄の幅 320
- 完了条件: build・clippy・test 成功、`grep -rn '[ぁ-んァ-ン一-龥]' app/src/ui app/src/state.rs` がコメント以外で 0 件

### Task 6.11 曲一覧・アルバム画面の英語化と余白
- ファイル: `app/src/ui/tracks.rs`, `app/src/ui/albums.rs`
- 文言: 列 `#` / `Title` / `Length` / `Artist` / `Album` / `Genres` / `Bitrate/Format`、アルバム一覧 `Album` / `Artist` / `Year` / `Tracks` / `Length`、アーティスト `Artist` / `Tracks`、ジャンル `Genre` / `Tracks`、空表示 `No tracks yet. Add a music folder from the sidebar.` / `No albums` / `No artists` / `No genres`、戻る `← Albums`、アルバム見出し下 `{year} • {n} tracks • {m:ss or h:mm:ss}`
- 余白: 行高 `ROW_HEIGHT`、ヘッダー `HEADER_HEIGHT`、ヘッダー文字は weak 色、`#` と `Length` は右寄せ。中央パネル内側余白 16。アルバム詳細の見出しブロックは上下 16、見出し 26pt・アーティスト 16pt
- `tracks::show(ui, state, show_album_column: bool)` に変更し、アルバム詳細では Album 列を隠す（モックどおり）
- 完了条件: build・clippy・test 成功、6.10 と同じ grep が 0 件

### Task 6.12 開発用スクリーンショット（リモート確認用）
- ファイル: `app/src/screenshot.rs`（新規）, `app/src/main.rs`
- 内容: 環境変数 `MMP_SCREENSHOT=<出力 .png パス>` があるときだけ有効。`MMP_NAV=all|albums|album|artists|genres`（album は albums の先頭を開く）で初期画面を選ぶ。
  スキャンが終わってから（最大 600 フレーム待つ）30 フレーム後に `ctx.send_viewport_cmd(ViewportCommand::Screenshot(Default::default()))`、
  `Event::Screenshot { image, .. }` を受けたら 24bit BMP を書き（依存追加なし）、`sips -s format png <bmp> --out <png>` で変換し、`ViewportCommand::Close` で終了
- 完了条件: build・clippy・test 成功、`MMP_SCREENSHOT=/tmp/x.png ./target/debug/macmusicplay` で PNG ができて終了する

### Task 6.13 サイドバーの見た目をモックに寄せる
- ファイル: `app/src/ui/sidebar.rs`
- 内容: nav 項目・ジャンル項目を **左寄せ・横幅いっぱい・高さ 30** のボタンにする:
  `egui::Button::new(()).left_text(label).frame_when_inactive(false).corner_radius(6.0).min_size(egui::vec2(ui.available_width(), 30.0))`、
  選択中だけ `.fill(ui.visuals().widgets.inactive.weak_bg_fill)`（控えめなグレー。鮮やかな selection 色は使わない）。
  ジャンル行は `left_text(g)` と `right_text(RichText::new(n.to_string()).weak())`。`Add folder…` は `Add folder...`（ヒラギノの「…」は中央点になるため）
- 完了条件: build・clippy 成功。スクショで左寄せ・グレー選択を確認

### Task 6.14 表・ツールバーの細部
- ファイル: `app/src/ui/tracks.rs`, `app/src/ui/albums.rs`, `app/src/ui/toolbar.rs`
- 内容:
  - すべての表の列を `resizable(false)` にし、縦の区切り線を無くす（モックに線は無い）
  - 空の値の代替表示（weak 色）: アルバム名 `Unknown Album`、アーティスト `Unknown Artist`（album 一覧・album 詳細見出し・tracks の Artist/Album 列）
  - 件数の単複: `pub fn count(n: usize, one: &str, many: &str) -> String`（tracks.rs に置く。`1 track` / `2 tracks`）を album 詳細見出しとツールバーで使う
  - ツールバーの件数: Nav::Album / Artist / Genre / AllTracks は `{n} tracks`、Albums は albums、Artists は artists、Genres は genres
- 完了条件: build・clippy・test 成功（count のテスト: 0 tracks / 1 track / 2 tracks）

---

## 段階7: プレイヤーバー・シーク・キュー / シャッフル / リピート

方針:
- 再生エンジンを core に新設（`core/src/player/`）。専用スレッドが デコーダ → リング → OutputUnit を回し、UI は `Player` ハンドル経由でコマンドを送る。状態は `Arc<Mutex<PlayerStatus>>`（エンジンスレッドと UI スレッドだけが触る。**オーディオコールバックは触らない**）
- オーディオコールバックが使う共有値はすべてアトミック（`RenderShared { volume_bits: AtomicU32, flush: AtomicBool, frames_played: AtomicU64 }`）。ロック・アロケーション・I/O なし
  - 音量: コールバック内でサンプルに乗算（段階12 のビットパーフェクトでは 1.0 固定）
  - flush: シーク・曲変更時にエンジンが立てる。コールバックは `consumer.slots()` 分を `read_chunk(n)?.commit_all()` で捨てて無音を出し、フラグを下ろす
  - frames_played: コールバックが出力したフレーム数（無音も含まない＝リングから取れた分だけ）を加算。エンジンは曲の境界フレームを記録して「現在曲の再生位置」を計算する
- ギャップレスは段階3 と同じく、同じ形式なら同じリングへ続けて書く。形式が変わるときだけ出力を作り直す
- キュー: 元の並び `items` と再生順 `order`（シャッフル時は現在曲を先頭に固定して残りを並べ替え。乱数は依存を増やさず xorshift64）。リピート `Off / All / One`
- 前の曲: 再生位置が 3 秒超なら曲頭へ、以下なら前の曲へ
- UI: モック下部のプレイヤーバー（左: 曲名/アーティスト、中央: shuffle / prev / play-pause / next / repeat と経過・シークバー・全体時間、右: 音量 % とスライダー）。再生中の再描画は 250ms 間隔（spec の 250ms〜1s）。曲行ダブルクリックで「表示中の一覧を先頭からキューに入れ、その曲から再生」。再生中の行に `PLAYING` バッジ
- アイコン: egui 同梱の絵文字フォントの記号（⏮ ⏯ ⏭ 🔀 🔁 🔂 🔊 等）。ヒラギノに無い字形は自動で egui 既定フォントにフォールバックする（表示を スクショで確認）

参照した API:
- `ExtAudioFileSeek(ExtAudioFileRef, i64 /*フレーム*/) -> OSStatus`（クライアント形式のフレーム単位）
- symphonia 0.6.1: `format.seek(SeekMode::Accurate, SeekTo::Timestamp { ts: Timestamp::new(i64), track_id }) -> Result<SeekedTo { required_ts, actual_ts, .. }>`（Accurate は必ず手前に着地 → `required - actual` フレームを読み捨てる）、`decoder.reset()`。Vorbis の ts はトラックの time_base（通常 1/sample_rate）単位 — 実装時に `track.time_base` を確認
- rtrb: `consumer.slots()`, `consumer.read_chunk(n) -> Result<ReadChunk>`, `chunk.commit_all()`

### Task 7.1 デコーダのシーク
- ファイル: `core/src/coreaudio/ext_audio_file.rs`, `core/src/decode/{mod.rs, ext.rs, vorbis.rs}`, `core/tests/seek.rs`
- 内容: `ExtAudioFile::seek(&mut self, frame: i64) -> Result<()>`。`Decoder` トレイトに `fn seek(&mut self, frame: u64) -> Result<(), DecodeError>`（範囲外は末尾に丸める）。Vorbis は Accurate シーク後に不足分を読み捨て、`pending` を空にする
- 完了条件: テスト — 3 秒の FLAC / MP3 / AAC / Ogg で 1.5 秒（66150 フレーム）へシーク後、残り全部を読んだフレーム数が 66150（完全一致）。範囲外（10 秒）へのシークで panic せず、以後 read が 0

### Task 7.2 OutputUnit の音量・flush・再生フレーム数
- ファイル: `core/src/coreaudio/output.rs`, `core/tests/output_unit.rs`
- 内容: `pub struct RenderShared { volume_bits: AtomicU32, flush: AtomicBool, frames_played: AtomicU64 }`（`pub fn volume()/set_volume(f32)/request_flush()/frames_played()`）を `Arc` で RenderState と共有。`OutputUnit::shared(&self) -> Arc<RenderShared>`。コールバック本体を `fn render_into(out: &mut [f32], channels: usize, consumer: &mut Consumer<f32>, shared: &RenderShared)` に切り出し、CoreAudio 無しでテスト可能にする
- 完了条件: `render_into` の単体テスト — 音量 0.5 で値が半分、flush で残りが捨てられ出力が無音・フラグが下りる、frames_played がリングから取れたフレーム数だけ増える（不足分の無音は数えない）。全テスト・clippy 成功

### Task 7.3 キュー（純粋ロジック）
- ファイル: `core/src/player/mod.rs`, `core/src/player/queue.rs`（+ `lib.rs`）
- 型:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)] pub enum RepeatMode { #[default] Off, All, One }
  pub struct Queue { items: Vec<PathBuf>, order: Vec<usize>, pos: usize, shuffle: bool, repeat: RepeatMode, rng: u64 }
  impl Queue {
      pub fn new(items: Vec<PathBuf>, start: usize, shuffle: bool, repeat: RepeatMode, seed: u64) -> Self;
      pub fn current(&self) -> Option<(usize /*items の添字*/, &Path)>;
      /// 曲の自然終了時。One は同じ曲、All は末尾→先頭、Off は末尾で None
      pub fn advance_on_end(&mut self) -> Option<(usize, &Path)>;
      /// ユーザー操作の次へ。One でも次の曲へ進む（All/One は末尾→先頭、Off は末尾で None）
      pub fn next(&mut self) -> Option<(usize, &Path)>;
      pub fn prev(&mut self) -> Option<(usize, &Path)>;       // 先頭では先頭のまま
      pub fn peek_after_current(&self) -> Option<(usize, &Path)>;  // ギャップレス先読み用（advance_on_end と同じ規則、状態は変えない）
      pub fn jump_to_item(&mut self, item: usize) -> Option<(usize, &Path)>;
      pub fn set_shuffle(&mut self, on: bool);  // 現在曲は維持
      pub fn set_repeat(&mut self, r: RepeatMode);
      pub fn items(&self) -> &[PathBuf]; pub fn upcoming(&self) -> Vec<usize>;  // 現在曲の次からの items 添字
  }
  ```
- 完了条件: テスト — 順送り・戻り・末尾の各リピートの挙動、シャッフルで全曲 1 回ずつ・現在曲が先頭・同じ seed で同じ順、シャッフル解除で元の並びの現在位置に戻る、peek と advance の一致、空キュー

### Task 7.4 再生エンジン
- ファイル: `core/src/player/engine.rs`, `core/tests/player.rs`
- 型:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum PlayState { Stopped, Playing, Paused }
  #[derive(Debug, Clone)] pub struct PlayerStatus { pub state: PlayState, pub current_item: Option<usize>, pub path: Option<PathBuf>,
      pub position_ms: u64, pub duration_ms: u64, pub volume: f32, pub shuffle: bool, pub repeat: RepeatMode, pub error: Option<String>, pub upcoming: Vec<usize> }
  pub struct Player { tx: Sender<Command>, status: Arc<Mutex<PlayerStatus>>, join: Option<JoinHandle<()>> }
  impl Player {
      pub fn new(device_uid: Option<String>) -> Self;
      pub fn play_queue(&self, items: Vec<PathBuf>, start: usize); pub fn toggle_pause(&self); pub fn stop(&self);
      pub fn next(&self); pub fn prev(&self); pub fn seek_ms(&self, ms: u64); pub fn set_volume(&self, v: f32);
      pub fn set_shuffle(&self, on: bool); pub fn set_repeat(&self, r: RepeatMode); pub fn jump_to_item(&self, i: usize);
      pub fn status(&self) -> PlayerStatus;   // Mutex が poison なら既定値
  }
  impl Drop for Player  // Quit コマンド送信 → join
  ```
  - エンジンは `recv_timeout(10ms)` でコマンドを受けつつ、リングに空きがあればデコードして書く。曲の終わりで `peek_after_current` の曲を開いて続けて書く（形式が同じなら）。境界フレームを記録し、`frames_played` が境界を越えたら `advance_on_end` して status を更新
  - Pause は `OutputUnit::stop`、Resume は `start`。シーク・next・prev・jump・play_queue は flush してから新しい位置でデコードし直す
  - 開けない曲はスキップして status.error に記録（落ちない）。デバイス関連は段階4 の Session と同じ扱い（指定デバイス消失でデフォルトに作り直し）
- 完了条件: テスト（音量 0 で実行）— gap_a/gap_b を play_queue → 1 秒以内に Playing、position が進む、seek_ms(2000) 後の position が 2000±300、next で current_item が 1、両曲終了後 Stopped、存在しないパスを含むキューでも落ちず次の曲へ進む。全テスト・clippy 成功

### Task 7.5 アプリ状態への組み込み
- ファイル: `app/src/state.rs`
- 内容: `AppState` に `pub player: Player`、`pub now: PlayerStatus`（poll で毎回コピー）、`pub queue_items: Vec<TrackRow>`（キューに入れた時点の一覧）。`pub fn play_from(&mut self, index: usize)`（表示中の tracks をキューに入れてその曲から）。再生中は `request_repaint_after(250ms)`
- 完了条件: build・clippy 成功

### Task 7.6 プレイヤーバー
- ファイル: `app/src/ui/player_bar.rs`, `app/src/main.rs`
- 内容: `Panel::bottom("player")`（高さ 84、余白 16）。左: 44x44 の角丸プレースホルダ（アートワークは段階10）＋曲名（強調）とアーティスト（weak）。中央: `🔀 ⏮ ▶/⏸ ⏭ 🔁/🔂` のボタン（シャッフル・リピートは有効時に selection 色）、その下に `経過 m:ss` ─ シークバー（`egui::Slider` ではなく自前の横長 `ProgressBar` + ドラッグ/クリックで seek）─ `全体 m:ss`。右: `{n}%` と音量スライダー（0〜100）。何も再生していないときはボタンを無効化
- 完了条件: build・clippy 成功。スクショで配置を確認

### Task 7.7 曲一覧からの再生・再生中表示・キーボード
- ファイル: `app/src/ui/tracks.rs`, `app/src/main.rs`
- 内容: 行ダブルクリックで `state.play_from(i)`。再生中の曲（`now.path` と行の path が一致）の行を selection 色で塗り、タイトルの後ろに小さな `PLAYING` バッジ。Space で再生/一時停止（テキスト入力中は無効）、⌘→ / ⌘← で次/前
- 完了条件: build・clippy 成功。実際に再生・一時停止・シーク・次/前・シャッフル・リピートが動く（指揮役/ユーザーが確認）

### Task 7.8 キューの表示
- ファイル: `app/src/ui/queue.rs`, `app/src/main.rs`
- 内容: プレイヤーバー右端の `☰` で右側パネル（幅 300）を開閉。`Up Next` 見出しと `now.upcoming` の曲（タイトル / アーティスト）。クリックでその曲へジャンプ
- 完了条件: build・clippy 成功

### 段階7 タスク細分化（2026-09-28）
方針: 1 タスク = 1 ファイル・関数 1〜2 個・テスト 1〜数個。先に骨組み（型・関数シグネチャ・doc）を作り、以降のタスクで中身を埋める。
上の Task 7.3 / 7.4 はこの細分化版で置き換える（仕様は上の記述どおり）。

- **7.3a** `core/src/player/{mod.rs,queue.rs}`（+ lib.rs）: `RepeatMode`、`Queue` 構造体、`new` / `current` / `next` / `prev` / `items`（シャッフル・リピートはまだ考慮しない＝order は 0..n、next は末尾で None、prev は先頭で先頭のまま）。テスト: 順送り・戻り・末尾・空キュー
- **7.3b** `queue.rs`: xorshift64 と `set_shuffle`（現在曲を先頭に固定し残りを並べ替え／解除で元の並びの現在位置へ）。`new` の shuffle 引数も反映。テスト: 全曲1回ずつ・現在曲が先頭・同 seed で同順・解除で復帰
- **7.3c** `queue.rs`: `set_repeat` / `advance_on_end` / `peek_after_current` / `jump_to_item` / `upcoming`、`next` の All/One 対応。テスト: 各リピートの末尾挙動・peek と advance の一致・jump
- **7.4a** `core/src/player/engine.rs`（骨組み）: `PlayState` / `PlayerStatus` / `enum Command` / `Player`（各メソッドは Command を送るだけ、`status()`、Drop で Quit → join）。エンジンスレッドは Quit を受けたら終わるだけ。テスト: new → status が Stopped → drop が固まらない
- **7.4b** `engine.rs`: `PlayQueue` で Queue を作り、現在曲を decode::open → OutputUnit（音量は status.volume）→ リングへ書き続け、曲の終わりでリングが空になるのを待って Stopped。テスト（音量 0）: gap_a 1 曲で Playing → 約 3 秒後 Stopped
- **7.4c** `engine.rs`: `TogglePause`（stop/start と state 切替）/ `Stop`（出力破棄・Stopped）/ `SetVolume`（shared.set_volume と status）。テスト: 一時停止中は frames_played が増えない・再開で増える・stop で Stopped
- **7.4d** `engine.rs`: 再生位置。曲の開始時の frames_played を記録し `position_ms = (frames_played - start) * 1000 / rate`、`duration_ms` は DB を使わずデコード前に分からないので 0 のまま可（UI 側で TrackRow の長さを使う）。status を 50ms ごとに更新。テスト: 1 秒後 position が 700〜1300
- **7.4e** `engine.rs`: `Seek` / `Next` / `Prev` / `JumpTo`。共通手順 = `request_flush()` → `flush_pending()` が false になるまで待つ（最大 200ms、一時停止中は待たずに flush フラグだけ立てて再開時に効かせる）→ デコーダを開き直す/seek → start 位置を更新。テスト: seek 2000ms 後 position 2000±300、next で current_item が 1
- **7.4f** `engine.rs`: 曲の終わりで `peek_after_current` の曲を開き、形式が同じなら同じリングへ続けて書き（ギャップレス）、境界フレームを記録。frames_played が境界を越えたら `advance_on_end` して status 更新。違う形式ならリングが空になってから出力を作り直す。テスト: gap_a,gap_b で current_item が 0→1 と変わり、合計約 6 秒で Stopped
- **7.4g** `engine.rs`: 開けない曲はスキップして status.error に記録、デバイス指定と消失時のデフォルトへのフォールバック（段階4 の Session と同じ）。テスト: 存在しないパスを挟んだキューで落ちずに次へ

### Task 7.9 プレイヤーバーの見た目をモックに寄せる
- ファイル: `app/src/ui/player_bar.rs`
- 内容: 3 領域の固定レイアウト（左 320: アートワーク+曲情報 / 右 260: 音量+キュー / 中央: 残り）。中央はボタン行とシークバーを水平中央に。
  ボタンは枠なし（`Button::frame(false)`）の大きめアイコン（20pt、再生/一時停止は 26pt）、シャッフル・リピート有効時は `selection.bg_fill` 色。パネル高 100
- egui 0.36: `ui.scope_builder(egui::UiBuilder::new().max_rect(rect).layout(layout), |ui| ..)`
- 完了条件: build・clippy 成功、スクショで中央配置を確認

---

## 段階8: プレイリスト（作成・編集・削除、永続化込み）

方針:
- 保存先は DB とは別: `~/Library/Application Support/MacMusicPlay/playlists/<名前>.m3u8`（1 プレイリスト = 1 ファイル、UTF-8、絶対パス）。DB 削除・再スキャンで消えない。依存追加なし
- 書き込みは一時ファイル（同じディレクトリの `.<名前>.m3u8.tmp`）に書いてから rename（途中で落ちても壊れない）
- 名前はファイル名にそのまま使うので、`/` `:` と先頭の `.` と制御文字を禁止（`_` に置換しない。エラーで返す）。重複はエラー
- プレイリスト内の曲は DB の TrackRow に解決して表示。DB に無いパス（外付けが外れている等）は一覧から省くが、m3u8 からは消さない
- 並び順はファイルの順。同じ曲の重複は許す（位置で扱う）
- UI（モック）: サイドバーに `PLAYLISTS` セクション（名前一覧・`+` で新規・右クリックで Rename / Delete）。曲行の右クリックに `Add to Playlist ▸ <名前>`。プレイリスト表示中は右クリック `Remove from Playlist` と `Move Up` / `Move Down`
- "recently added" 等のスマートプレイリストは MVP 外（後で検討）

細分化（1 タスク = 1 ファイル・関数 1〜2 個・テスト）:
- **8.1** `core/src/playlist/{mod.rs,m3u.rs}`（+ lib.rs）: `pub fn parse_m3u(text: &str, base: &Path) -> Vec<PathBuf>`（`#` 行・空行を無視、相対パスは base 基準で絶対化、CRLF 対応）と `pub fn write_m3u(paths: &[PathBuf]) -> String`（先頭 `#EXTM3U`、改行 `\n`）。テスト: 往復・コメント/空行・相対パス・日本語パス・CRLF
- **8.2** `core/src/playlist/store.rs`: `pub struct PlaylistStore { dir: PathBuf }`、`new(dir) -> io::Result<Self>`（create_dir_all）、`validate_name(&str) -> Result<(), PlaylistError>`、`list() -> io::Result<Vec<String>>`（名前順）、`load(name) -> Result<Vec<PathBuf>, PlaylistError>`、`save(name, &[PathBuf])`（tmp→rename）。`PlaylistError { InvalidName(String), AlreadyExists(String), NotFound(String), Io(#[from] io::Error) }`。テスト（tempdir）
- **8.3** `store.rs`: `create(name)`（空で作成・重複エラー）、`rename(old, new)`、`delete(name)`、`add(name, &[PathBuf])`（末尾追加）、`remove_at(name, index)`、`move_item(name, from, to)`。テスト
- **8.4** `core/src/library/query.rs`: `pub fn tracks_by_paths(&self, paths: &[PathBuf]) -> Result<Vec<(usize, TrackRow)>, LibraryError>`（戻り値の usize は paths 上の位置。DB に無いものは省く。順序はpaths順）。テスト
- **8.5** `core/src/paths.rs`: `pub fn playlists_dir() -> io::Result<PathBuf>`（data_dir/playlists）。テスト（data_dir_in と同様）
- **8.6** `app/src/state.rs`: `pub playlists: Vec<String>`、`playlist_store: Option<PlaylistStore>`、`Nav::Playlist(String)`、`playlist_positions: Vec<usize>`（表示中 tracks[i] のプレイリスト上の位置）、`reload` で Nav::Playlist の曲を読む、`create_playlist / rename_playlist / delete_playlist / add_to_playlist(name, &[PathBuf]) / remove_from_playlist(track_index) / move_in_playlist(track_index, up: bool)`（失敗は status に）
  - **8.6a** state.rs: フィールド `playlists / playlist_store / playlist_positions` 追加、`open()` で `PlaylistStore::new(paths::playlists_dir()?)`（失敗は status に出して None）、`fn reload_playlists(&mut self)`（store.list() → playlists）
  - **8.6b** state.rs + main.rs + toolbar.rs: `Nav::Playlist(String)`、reload の分岐（store.load → lib.tracks_by_paths → tracks と playlist_positions）、main.rs / toolbar.rs の match に Playlist を曲一覧として追加
  - **8.6c** state.rs: `create_playlist() -> Option<String>`（New Playlist / New Playlist 2… の空き名で作成し選択）、`rename_playlist(old,new)`、`delete_playlist(name)`
  - **8.6d** state.rs: `add_to_playlist(name, &[PathBuf])`、`remove_from_playlist(track_index)`、`move_in_playlist(track_index, up)`
- **8.7** `app/src/ui/sidebar.rs`: PLAYLISTS セクション（LIBRARY と GENRES の間）。`+` で「New Playlist」（重複しない名前 `New Playlist`, `New Playlist 2`…で作成して即選択）、右クリック Rename（インラインの TextEdit）/ Delete
- **8.8** `app/src/ui/tracks.rs`: 行の右クリックメニュー（`Add to Playlist ▸`、Nav::Playlist のときは `Remove from Playlist` / `Move Up` / `Move Down`）

## 段階9: メディアキー / Now Playing（souvlaki 0.8.3）

設計:
- `app/src/media.rs` に OS のメディア操作の橋渡しを置く（core には入れない。UI スレッド側の関心事のため）
- souvlaki の `MediaControls::attach` のコールバック（別スレッド）では `mpsc::Sender` に送って `ctx.request_repaint()` するだけ。処理は UI スレッドの `logic()` で行う
- イベント→操作の変換は純粋関数 `media_action` にしてテストする（Seek は ±10 秒、SeekBy は指定量、SetPosition は絶対位置。0〜duration に丸める）
- Now Playing への反映: 曲（path）が変わったら `set_metadata`、状態（Playing/Paused/Stopped）が変わったか位置が 1 秒単位で変わったら `set_playback`
- `MediaControls::new` が失敗しても落ちない（Option で持ち、何もしない）

タスク:
- **9.1** `app/Cargo.toml` に souvlaki = "0.8.3"（指揮役が追加済み）、`app/src/media.rs`: `pub enum MediaAction { Play, Pause, Toggle, Next, Prev, Stop, SeekTo(u64) }`、`pub fn media_action(ev: &MediaControlEvent, position_ms: u64, duration_ms: u64) -> Option<MediaAction>` + テスト
- **9.2** `app/src/media.rs`: `pub struct MediaKeys { controls, rx }`、`pub fn new(ctx: egui::Context) -> Option<Self>`、`pub fn poll_events(&self) -> Vec<MediaControlEvent>`
- **9.3** `app/src/media.rs`: `MediaKeys::update(&mut self, now: &PlayerStatus, track: Option<&TrackRow>)`（前回値を覚えて変化時だけ OS に送る）
- **9.4** `app/src/main.rs` + `app/src/state.rs`: `MmpApp` に `media: Option<MediaKeys>`、`logic()` で poll_events → `AppState::apply_media_action(MediaAction)` → `update`

## 段階10: アートワークキャッシュ

設計:
- 表示場所（モックアップ）: アルバム詳細のヘッダー（160px）、プレイヤーバー左（56px）。サムネイルは Retina 用に最大 320px
- 取得元: 埋め込み画像（lofty。CoverFront 優先、無ければ最初の画像）→ 無ければ同じフォルダの cover / folder / front / album（.jpg/.jpeg/.png、大文字小文字無視）
- ディスクキャッシュ: `~/Library/Application Support/MacMusicPlay/artwork/<key>.jpg`（JPEG 品質 85）。key = album_key の FNV-1a 64bit 16進（std の Hasher は実行ごとに安定しないため自前）。画像が無いアルバムは `<key>.none` を置いて再探索しない。書き込みは tmp → rename。DB と同じくキャッシュ扱い（消しても再生成）
- 生成・読み込みはバックグラウンドスレッド（mpsc で依頼 / 結果）。UI スレッドはデコード済み RGBA を受け取ってテクスチャにするだけ
- メモリ: app 側で album_key → TextureHandle の LRU（上限 200）。読み込み中は二重依頼しない
- 依存: `image = { version = "0.25.10", default-features = false, features = ["jpeg", "png"] }`（core、指揮役が追加）

タスク:
- **10.1** `core/src/artwork/source.rs`: `embedded_picture(path) -> Option<Vec<u8>>`、`folder_image(dir) -> Option<PathBuf>`、`find_artwork_bytes(track) -> Option<Vec<u8>>` + テスト
- **10.2** `core/src/artwork/thumb.rs`: `THUMB_SIZE = 320`、`make_thumbnail_jpeg(bytes, max) -> Option<Vec<u8>>`、`decode_rgba(bytes) -> Option<Thumb>`（`Thumb { width, height, rgba }`）+ テスト
- **10.3** `core/src/artwork/cache.rs`: `cache_key(album_key) -> String`（FNV-1a）、`ArtworkCache { dir }`、`get_or_create(album_key, track) -> Option<Vec<u8>>` + テスト
- **10.4** `core/src/artwork/loader.rs` + `core/src/paths.rs`: `ArtworkLoader`（スレッド + mpsc、`request` / `try_results`）、`artwork_dir()` + テスト
- **10.5** `app/src/artwork.rs` + `app/src/state.rs`: `ArtTextures`（LRU、`get` / `poll`）、AppState に保持、`poll` から呼ぶ + 退避ロジックのテスト
- **10.6** `app/src/ui/albums.rs` + `app/src/ui/player_bar.rs`: アルバム詳細ヘッダー 160px とプレイヤーバー 56px に表示（無ければ今のプレースホルダー）

## 段階11: テーマ切り替え（System / Light / Dark）

設計:
- 設定は `~/Library/Application Support/MacMusicPlay/settings.txt`（`key=value` の行。依存を増やさないため serde は使わない）。読めない・不明な値は既定値（System）。書き込みは tmp → rename
- core は egui に依存しないので `ThemeMode { System, Light, Dark }` を core に置き、app で `egui::ThemePreference` に変換して `ctx.set_theme` する（System は OS に追従）
- 切り替え UI: ツールバー右端のボタン（◐ System → ☀ Light → ☾ Dark → … の順に巡回、ホバーで「Theme: …」）
- ダーク前提の色（`Color32::from_white_alpha`）をテーマ対応のヘルパーに置き換える: ダークは白の半透明、ライトは黒の半透明
- スクショ用に `MMP_THEME=light|dark` で上書きできるようにする（保存はしない）

タスク:
- **11.1** `core/src/settings.rs` + `core/src/paths.rs`: `ThemeMode`（`next()` で巡回）、`Settings { theme }`、`parse(text)` / `to_text()` / `load(path)` / `save(path)`、`settings_path()` + テスト
- **11.2** `app/src/style.rs` + `ui/sidebar.rs` + `ui/player_bar.rs` + `artwork.rs`: `subtle_fill(ui, alpha)` を作り、`from_white_alpha` の 5 箇所を置き換える
- **11.3** `app/src/state.rs` + `app/src/ui/toolbar.rs` + `app/src/main.rs`: AppState に `settings` / `settings_path`、`set_theme(ctx, mode)`（適用 + 保存）、起動時に適用（`MMP_THEME` 優先）、ツールバーの切り替えボタン

## 段階12: ビットパーフェクトモード

仕様（spec.md「音量モード」）: 曲ごとに出力デバイスのサンプルレートを曲に追従 / ソフトウェア音量 100% 固定（スライダー無効）/ hog mode は設定で選択 / Bluetooth など成立しないデバイスでは選択不可

設計:
- 12.1・12.2 は音声エンジンのコアなので指揮役が直接書く（CLAUDE.md の例外）。12.3・12.4 はワーカー
- device.rs に安全ラッパーを追加: 利用可能なレート一覧（`kAudioDevicePropertyAvailableNominalSampleRates` → `AudioValueRange` の配列）、レートの取得/設定（設定は非同期なので読み戻して最大 1 秒待つ）、hog mode（`kAudioDevicePropertyHogMode` はトグル動作: 値に関係なく「空いていれば取得・自分が持っていれば解放」。取得/解放の前に現在の所有 pid を読んで判断する）
- 成立しないデバイス: Bluetooth / Bluetooth LE / AirPlay
- エンジン: `Player::set_output_mode(bit_perfect, hog)`。ビットパーフェクト時は出力を開く前に対象デバイス（未指定ならデフォルト出力の ID）のレートを曲のレートに合わせる（対応していなければそのまま・ステータスに注記）。ソフト音量は 1.0 固定（ユーザーの音量値は保持し、通常モードに戻したら反映）。gapless の先読みは今どおり「同じレート・ch のときだけ」なので、レートが変わる曲間では出力を作り直してレートが切り替わる
- デバイスのレートは最初に変更したときの値を覚えておき、モード解除・終了時に戻す。hog も同様に解放
- PlayerStatus に `bit_perfect: bool`、`bit_perfect_available: bool`（現在のデバイスで成立するか）、`device_rate: Option<f64>` を追加
- 設定: settings.txt に `bit_perfect=on|off`、`hog_mode=on|off`

タスク:
- **12.1**（指揮役）`core/src/coreaudio/device.rs`: `nominal_sample_rate(id)`、`available_sample_rates(id)`、`supports_rate(ranges, rate)`、`set_nominal_sample_rate(id, rate)`、`hog_owner(id)` / `set_hog(id, on)`、`transport_type(id)` / `bit_perfect_capable(transport)` + テスト
- **12.2**（指揮役）`core/src/player/engine.rs`: 出力モード、レート追従、音量固定、hog、復元、ステータス + テスト
- **12.3** `core/src/settings.rs`: `bit_perfect` / `hog_mode` の保存 + テスト
- **12.4** `app`: 起動時に適用、ツールバーの ⚙ メニュー（Bit-perfect / Exclusive access、成立しないデバイスでは無効）、ビットパーフェクト中は音量スライダー無効 + プレイヤーバーに「Bit-perfect · 96 kHz」表示

## 追加: 出力デバイスの選択 UI（spec の MVP「出力デバイス切り替え」、ユーザー依頼 2026-09-29）

設計:
- エンジン: `Player::set_device(Option<uid>)`。再生中なら同じ位置から新しいデバイスで続ける。切り替え前に旧デバイスの hog 解放・レート復元。見つからない UID はエラー表示してデフォルトへ
- 設定: settings.txt に `output_device=<uid>`（空 = System default）。起動時に適用
- UI: ⚙ メニューに「Output」見出しと、`System default` + `output_devices()` の一覧をラジオボタンで。メニューを開いている間だけ一覧を取得（閉じている間は Core Audio を呼ばない）

タスク:
- **D.1**（指揮役）`core/src/player/engine.rs`: `set_device` + テスト
- **D.2** `core/src/settings.rs`: `output_device: Option<String>`（Copy をやめる）+ テスト
- **D.3** `app/src/state.rs` + `app/src/ui/toolbar.rs`: `set_output_device`、起動時に適用、⚙ メニューに一覧

## 追加: UI の角丸化と選択/再生中の区別（ユーザー依頼 2026-09-29）

- 全体を丸く: ウィジェット・ウィンドウ・メニューの角丸、スライダーのつまみを円に
- 曲一覧: egui_extras は行背景（縞・選択・ホバー）を角丸 0 でセルごとに塗るため使わず、各行の最初のセルで `Shape::Noop` を置いて場所を確保し、行の response の矩形が分かってから角丸の背景に差し替える（文字の下に描かれる）。選択 = 薄いグレー、再生中 = アクセント色を薄く、両方 = 少し濃く。再生中は曲名もアクセント色
- 再生ボタン類: フォントの ⏮▶⏸⏭ は角が尖るので、角を丸めた多角形（角を二次ベジェで丸める）を自前で描く

タスク:
- **U.1** `app/src/style.rs`: 角丸 8（ウィンドウ 12・メニュー 10）、`handle_shape = Circle`
- **U.2** `app/src/ui/tracks.rs`: 角丸の行背景、選択/再生中の色分け
- **U.3** `app/src/ui/icons.rs`（新規）+ `app/src/ui/player_bar.rs`: 角丸の再生・一時停止・前後アイコン + テスト
