#!/bin/bash
set -euo pipefail
export LC_ALL=en_US.UTF-8

export PATH="/opt/homebrew/bin:$PATH"

if ! command -v sox &> /dev/null; then
  echo "Please install sox with 'brew install sox'"
  exit 1
fi

command -v lame &> /dev/null || { echo "Please install lame with 'brew install lame'"; exit 1; }

mkdir -p testdata

sox -n -r 44100 -b 16 -c 2 testdata/sine440_44k16_stereo.flac synth 3.0 sine 440 gain -6
sox -n -r 96000 -b 24 -c 2 testdata/sine1k_96k24_stereo.flac synth 2.0 sine 1000 gain -6
sox -n -r 44100 -b 16 -c 2 testdata/_src_44k16.wav synth 3.0 sine 440 gain -6
sox testdata/_src_44k16.wav testdata/sine440_44k16.wav
sox testdata/_src_44k16.wav testdata/sine440_44k16.aiff
rm -f testdata/sine440.mp3
lame --quiet -b 192 testdata/_src_44k16.wav testdata/sine440.mp3
sox testdata/_src_44k16.wav testdata/sine440.ogg
afconvert -f m4af -d alac testdata/_src_44k16.wav testdata/sine440_alac.m4a
afconvert -f m4af -d aac -b 192000 testdata/_src_44k16.wav testdata/sine440_aac.m4a
afconvert -f m4af -d aach -b 64000 testdata/_src_44k16.wav testdata/sine440_heaac.m4a

# --- Gapless test pair (Task 3.1) ---
sox -n -r 44100 -b 16 -c 2 testdata/_src6s_44k16.wav synth 6.0 sine 440 gain -6
sox testdata/_src6s_44k16.wav testdata/_gap_a.wav trim 0 132300s
sox testdata/_src6s_44k16.wav testdata/_gap_b.wav trim 132300s
sox testdata/_gap_a.wav testdata/gap_a.flac
sox testdata/_gap_b.wav testdata/gap_b.flac
rm -f testdata/gap_a.mp3 testdata/gap_b.mp3
lame --quiet -b 192 testdata/_gap_a.wav testdata/gap_a.mp3
lame --quiet -b 192 testdata/_gap_b.wav testdata/gap_b.mp3
afconvert -f m4af -d aac -b 192000 testdata/_gap_a.wav testdata/gap_a.m4a
afconvert -f m4af -d aac -b 192000 testdata/_gap_b.wav testdata/gap_b.m4a
sox testdata/_gap_a.wav testdata/gap_a.ogg
sox testdata/_gap_b.wav testdata/gap_b.ogg

echo "Test audio generation completed successfully."
ls -la testdata

# --- Task 5.1: Tagged test audio sources ---
mkdir -p testdata/lib/AlbumA testdata/lib/AlbumB
rm -f testdata/lib/AlbumA/02.mp3
sox testdata/_src_44k16.wav --add-comment "TITLE=一曲目" --add-comment "ARTIST=テスト歌手" --add-comment "ALBUMARTIST=テスト歌手" --add-comment "ALBUM=テストアルバム" --add-comment "TRACKNUMBER=1" --add-comment "DISCNUMBER=1" --add-comment "DATE=2024" --add-comment "GENRE=Rock" testdata/lib/AlbumA/01.flac trim 0 1
sox testdata/_src_44k16.wav testdata/_src_1s.wav trim 0 1
lame --quiet -b 192 --id3v2-only --tt "二曲目" --ta "テスト歌手" --tl "テストアルバム" --tn 2 --ty 2024 --tg Rock testdata/_src_1s.wav testdata/lib/AlbumA/02.mp3
sox testdata/_src_44k16.wav --add-comment "TITLE=Ogg Song" --add-comment "ARTIST=Other" --add-comment "ALBUM=Album B" --add-comment "TRACKNUMBER=3" testdata/lib/AlbumB/track.ogg trim 0 1
sox testdata/_src_44k16.wav testdata/lib/notag.flac trim 0 1
printf 'not audio' > testdata/lib/broken.mp3
printf 'readme' > testdata/lib/readme.txt
sox -n -r 44100 -b 16 -c 2 testdata/long_60s.flac synth 60 sine 440 gain -12
