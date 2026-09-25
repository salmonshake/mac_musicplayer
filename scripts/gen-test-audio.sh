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

echo "Test audio generation completed successfully."
ls -la testdata
