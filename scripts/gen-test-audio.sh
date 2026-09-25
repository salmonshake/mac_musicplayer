#!/bin/bash
set -euo pipefail

export PATH="/opt/homebrew/bin:$PATH"

if ! command -v sox &> /dev/null; then
  echo "Please install sox with 'brew install sox'"
  exit 1
fi

mkdir -p testdata

sox -n -r 44100 -b 16 -c 2 testdata/sine440_44k16_stereo.flac synth 3.0 sine 440 gain -6
sox -n -r 96000 -b 24 -c 2 testdata/sine1k_96k24_stereo.flac synth 2.0 sine 1000 gain -6
sox -n -r 44100 -b 16 -c 2 testdata/_src_44k16.wav synth 3.0 sine 440 gain -6
sox testdata/_src_44k16.wav testdata/sine440_44k16.wav
sox testdata/_src_44k16.wav testdata/sine440_44k16.aiff
sox testdata/_src_44k16.wav testdata/sine440.mp3
sox testdata/_src_44k16.wav testdata/sine440.ogg
afconvert -f m4af -d alac testdata/_src_44k16.wav testdata/sine440_alac.m4a
afconvert -f m4af -d aac -b 192000 testdata/_src_44k16.wav testdata/sine440_aac.m4a
afconvert -f m4af -d aach -b 64000 testdata/_src_44k16.wav testdata/sine440_heaac.m4a

echo "Test audio generation completed successfully."
ls -la testdata
