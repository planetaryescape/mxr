#!/usr/bin/env bash
set -euo pipefail

library_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source_dir="$library_dir/source"
clip_dir="$library_dir/clips"
poster_dir="$library_dir/posters"
contact_dir="$library_dir/contact-sheets"
work_dir="$(mktemp -d /tmp/mxr-video-render.XXXXXX)"

font_file="/Users/bhekanik/Library/Fonts/JetBrainsMono-Regular.ttf"
if [[ ! -f "$font_file" ]]; then
  font_file="/System/Library/Fonts/SFNSMono.ttf"
fi

cleanup() {
  rm -rf -- "$work_dir"
}
trap cleanup EXIT

mkdir -p "$clip_dir" "$poster_dir" "$contact_dir"

render_card() {
  local text="$1"
  local output="$2"
  local duration="${3:-2.4}"
  local image="${output%.mp4}.png"

  magick -size 1280x720 xc:"#07151f" \
    -font "$font_file" -fill "#f4f9fc" -pointsize 38 -gravity center \
    -annotate +0-18 "$text" \
    -fill "#8fa8bc" -pointsize 20 -annotate +0+54 "synthetic demo inbox" \
    "$image"

  ffmpeg -hide_banner -loglevel error -y \
    -loop 1 -i "$image" -t "$duration" -r 30 \
    -an -c:v libx264 -preset medium -crf 19 -pix_fmt yuv420p \
    "$output"
}

render_end_card() {
  local output="$1"
  local image="${output%.mp4}.png"

  magick -size 1280x720 xc:"#07151f" \
    -font "$font_file" -fill "#55d6ff" -pointsize 46 -gravity center \
    -annotate +0+0 "mxr.sh" \
    "$image"

  ffmpeg -hide_banner -loglevel error -y \
    -loop 1 -i "$image" -t 2.2 -r 30 \
    -an -c:v libx264 -preset medium -crf 19 -pix_fmt yuv420p \
    "$output"
}

standardize_clip() {
  local input="$1"
  local start="$2"
  local duration="$3"
  local output="$4"

  ffmpeg -hide_banner -loglevel error -y \
    -ss "$start" -t "$duration" -i "$input" \
    -vf "scale=1280:720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2:color=0x07151f,fps=30" \
    -an -c:v libx264 -preset medium -crf 19 -pix_fmt yuv420p \
    "$output"
}

render_video() {
  local id="$1"
  local title="$2"
  local source="$3"
  local start="$4"
  local duration="$5"

  local title_card="$work_dir/${id}-title.mp4"
  local middle="$work_dir/${id}-middle.mp4"
  local end_card="$work_dir/${id}-end.mp4"
  local output="$clip_dir/${id}.mp4"

  render_card "$title" "$title_card"
  standardize_clip "$source" "$start" "$duration" "$middle"
  render_end_card "$end_card"

  ffmpeg -hide_banner -loglevel error -y \
    -i "$title_card" -i "$middle" -i "$end_card" \
    -filter_complex "[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]" \
    -map "[v]" -an -c:v libx264 -preset medium -crf 19 \
    -pix_fmt yuv420p -movflags +faststart "$output"

  ffmpeg -hide_banner -loglevel error -y \
    -ss 2.8 -i "$output" -frames:v 1 -update 1 \
    "$poster_dir/${id}.jpg"
}

render_composite() {
  local id="$1"
  local title="$2"
  shift 2

  local title_card="$work_dir/${id}-title.mp4"
  local end_card="$work_dir/${id}-end.mp4"
  local output="$clip_dir/${id}.mp4"
  local filter_inputs=()
  local concat_streams="[0:v]"
  local input_index=1

  render_card "$title" "$title_card"
  render_end_card "$end_card"
  filter_inputs+=(-i "$title_card")

  while (( "$#" >= 3 )); do
    local source="$1"
    local start="$2"
    local duration="$3"
    local middle="$work_dir/${id}-middle-${input_index}.mp4"
    standardize_clip "$source" "$start" "$duration" "$middle"
    filter_inputs+=(-i "$middle")
    concat_streams+="[${input_index}:v]"
    input_index=$((input_index + 1))
    shift 3
  done

  filter_inputs+=(-i "$end_card")
  concat_streams+="[${input_index}:v]"

  ffmpeg -hide_banner -loglevel error -y \
    "${filter_inputs[@]}" \
    -filter_complex "${concat_streams}concat=n=$((input_index + 1)):v=1:a=0[v]" \
    -map "[v]" -an -c:v libx264 -preset medium -crf 19 \
    -pix_fmt yuv420p -movflags +faststart "$output"

  ffmpeg -hide_banner -loglevel error -y \
    -ss 2.8 -i "$output" -frames:v 1 -update 1 \
    "$poster_dir/${id}.jpg"
}

agent="$source_dir/agent-full.webm"
cli="$source_dir/cli-full.webm"
tui="$source_dir/tui-full.webm"

render_video \
  "01-agent-checks-suspicious-mail" \
  "I asked my agent to check my inbox for scams" \
  "$agent" 0 38.4

render_video \
  "02-agent-searches-local-mail" \
  "My agent searches the local copy of my mail" \
  "$agent" 2.5 15

render_video \
  "03-agent-builds-a-query" \
  "It can build the mxr and jq query it needs" \
  "$agent" 9.5 18

render_video \
  "04-agent-answers-read-only" \
  "It inspected the inbox and left everything alone" \
  "$agent" 22.5 15.9

render_video \
  "05-cli-searches-50000-messages" \
  "Search 50,000 messages from the shell" \
  "$cli" 0 11

render_video \
  "06-cli-pipes-mail-through-jq" \
  "Pipe the local mailbox through jq" \
  "$cli" 7 16

render_video \
  "07-cli-ranks-newsletters" \
  "Find the newsletters I never read" \
  "$cli" 11 12

render_video \
  "08-tui-searches-local-mail" \
  "Search the same local mailbox from the TUI" \
  "$tui" 0 8

render_video \
  "09-tui-mail-tools" \
  "The mail tools I use every day live in the TUI" \
  "$tui" 2 9.6

render_composite \
  "10-one-mailbox-three-interfaces" \
  "The TUI, CLI and my agent share one mailbox" \
  "$tui" 0 8 \
  "$cli" 0 10 \
  "$agent" 2.5 12

render_composite \
  "11-mxr-is-written-in-rust" \
  "I wrote mxr in Rust" \
  "$tui" 0 8 \
  "$cli" 0 9 \
  "$agent" 9.5 10

render_video \
  "12-agent-previews-a-change" \
  "mxr shows what will change before it changes anything" \
  "$source_dir/safety-dry-run.webm" 0 8

render_video \
  "13-two-accounts-one-mailbox" \
  "Two accounts in the same local mailbox" \
  "$source_dir/two-accounts.webm" 0 12

render_video \
  "14-local-sender-analytics" \
  "Look at the history with one sender" \
  "$source_dir/sender-analytics.webm" 0 8

render_video \
  "15-searches-attachments" \
  "Search the local mailbox for attachments" \
  "$source_dir/attachment-search.webm" 0 9

for clip in "$clip_dir"/*.mp4; do
  name="$(basename "$clip" .mp4)"
  ffmpeg -hide_banner -loglevel error -y \
    -i "$clip" \
    -vf "fps=1/5,scale=480:-1,tile=4x2:padding=4:margin=4:color=0x07151f" \
    -frames:v 1 "$contact_dir/${name}.jpg"
done

printf 'Rendered %s clips in %s\n' "$(find "$clip_dir" -name '*.mp4' | wc -l | tr -d ' ')" "$clip_dir"
