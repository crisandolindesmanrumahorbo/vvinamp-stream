#!/bin/bash

# Convert MP3 to HLS
# Usage: ./convert_mp3_to_hls.sh input.mp3 output_directory

if [ $# -ne 2 ]; then
    echo "Usage: $0 <input.mp3> <output_directory>"
    echo "Example: $0 memori-baik.mp3 ./hls/memori-baik"
    exit 1
fi

INPUT_FILE="$1"
OUTPUT_DIR="$2"
BASENAME=$(basename "$INPUT_FILE" .mp3)

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Convert MP3 to HLS
ffmpeg -i "$INPUT_FILE" \
    -c:a aac \
    -b:a 128k \
    -ac 2 \
    -ar 44100 \
    -f hls \
    -hls_time 10 \
    -hls_playlist_type vod \
    -hls_segment_filename "$OUTPUT_DIR/${BASENAME}_%03d.ts" \
    "$OUTPUT_DIR/${BASENAME}.m3u8"

echo "Conversion complete!"
echo "Playlist: $OUTPUT_DIR/${BASENAME}.m3u8"
echo "Segments: $OUTPUT_DIR/${BASENAME}_*.ts"

# Optional: Show the playlist content
echo -e "\nPlaylist content:"
cat "$OUTPUT_DIR/${BASENAME}.m3u8"
