#!/bin/bash
set -euo pipefail

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
