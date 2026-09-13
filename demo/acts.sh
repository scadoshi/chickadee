# Demo acts and tmux layout for chickadee, sourced by run.sh and record.sh.
#
# Steller's demo lives on the wire, so it is all clients and protocol. Chickadee's
# lives on disk, so these acts spend most of their time in a hexdump: the whole point
# of an LSM store is the shape of what it writes down.

CAP=0.0; MAIN=0.1; A=0.2; B=0.3

# A short prompt for the demo panes only, set before capture starts so the command
# that sets it never appears. The default prompt is 31 characters of username and
# host, which is a quarter of a pane once the font is big enough to read in a GIF.
# %1~ tracks the directory, so it stays right when an act cds elsewhere.
set_prompt() {
  # PROMPT_EOL_MARK is the reverse-video % zsh prints when output ends mid-line.
  # Harmless in daily use, distracting at the top of every pane in a recording.
  tmux send-keys -t "$1" "export PS1='%1~ \$ ' PROMPT_EOL_MARK=''" Enter
  tmux send-keys -t "$1" 'clear' Enter
  sleep 0.15
  tmux clear-history -t "$1"   # drop the scrollback too, or the old prompt shows through
}

build_layout() {
  tmux kill-session -t "$SESSION" 2>/dev/null || true
  rm -f "$CAPTION_FILE"; : > "$CAPTION_FILE"
  rm -rf data
  cargo build --quiet

  tmux new-session -d -s "$SESSION" -x "$(tput cols)" -y "$(tput lines)"
  tmux set-option -t "$SESSION" -g status off
  tmux set-option -t "$SESSION" -g pane-border-status top
  tmux set-option -t "$SESSION" -g pane-border-format ' #{pane_title} '

  tmux split-window -v -b -l 3 -t "$SESSION":0.0
  tmux split-window -h -l 55% -t "$SESSION":0.1
  tmux split-window -v -l 50% -t "$SESSION":0.2

  CAP="$SESSION":0.0; MAIN="$SESSION":0.1; A="$SESSION":0.2; B="$SESSION":0.3
  tmux select-pane -t "$CAP"  -T 'chickadee'
  tmux select-pane -t "$MAIN" -T 'store'
  tmux select-pane -t "$A"    -T 'disk'
  tmux select-pane -t "$B"    -T 'second client'

  for p in "$MAIN" "$A" "$B"; do set_prompt "$p"; done
  tmux send-keys -t "$CAP" "source demo/lib.sh; caption_loop" Enter
  sleep 0.6
}

# Only the last act needs a second client. The others left an empty pane in frame
# for the whole clip, so drop it and let the disk pane take the column.
want_second_client() {
  local have
  have=$(tmux list-panes -t "$SESSION" | wc -l | tr -d ' ')
  if [ "$1" = "yes" ] && [ "$have" -lt 4 ]; then
    tmux split-window -v -l 50% -t "$SESSION":0.2
    tmux select-pane -t "$SESSION":0.3 -T 'second client'
    set_prompt "$SESSION":0.3
    B="$SESSION":0.3
  elif [ "$1" = "no" ] && [ "$have" -ge 4 ]; then
    tmux kill-pane -t "$SESSION":0.3 2>/dev/null || true
  fi
  sleep 0.3
}

pin_layout() {
  # Only balance the stacked panes when there are two. With one pane in the right
  # column, resizing it to 50% gives the other half back to the caption row.
  local panes
  panes=$(tmux list-panes -t "$SESSION" | wc -l | tr -d ' ')
  [ "$panes" -ge 4 ] && tmux resize-pane -t "$A" -y 50% 2>/dev/null
  tmux resize-pane -t "$CAP" -y 2 2>/dev/null || true
  sleep 0.3
}

reset_state() {
  # Address only the panes that exist; an act may have dropped one.
  local idx
  for idx in $(tmux list-panes -t "$SESSION" -F '#{pane_index}' | grep -v '^0$'); do
    tmux send-keys -t "$SESSION":0."$idx" C-c; sleep 0.2
    tmux send-keys -t "$SESSION":0."$idx" 'quit' Enter; sleep 0.2
    tmux send-keys -t "$SESSION":0."$idx" 'clear' Enter
  done
  pkill -f 'target/debug/server' 2>/dev/null || true
  rm -rf data
  : > "$CAPTION_FILE"
  sleep 0.5
}

cli()    { tmux send-keys -t "$MAIN" './target/debug/cli' Enter; sleep 1.0; }
seed()   { run_in "$MAIN" "set $1 $2" 0.5; }

act1_cli_basics() {
  say "This is chickadee. A log-structured key-value store, built from the Bitcask paper up."
  cli
  say "Let's set some keys."
  run_in "$MAIN" 'set city seattle'
  run_in "$MAIN" 'set bird chickadee'
  say "Read one back."
  run_in "$MAIN" 'get city'
  say "Now let's delete one. Though delete is the wrong word for what happens."
  run_in "$MAIN" 'delete city'
  say "Gone from reads."
  run_in "$MAIN" 'get city'
  say "But nothing was erased. It wrote a tombstone, which is how a flushed key stays dead."
  run_in "$MAIN" 'get bird'
  pause
}

act2_wal_on_disk() {
  # xxd emits 67-character lines. Give the disk pane most of the width so they
  # land whole, rather than narrowing the dump and splitting keys across rows.
  tmux resize-pane -t "$MAIN" -x 35% 2>/dev/null || true
  say "Every write hits the log before anything else. Let's go look at it."
  cli
  seed city seattle
  seed bird chickadee
  say "Two keys in. Now quit, so nothing is left sitting in memory."
  run_in "$MAIN" 'quit' 1.0
  say "Let's read the raw bytes off disk."
  run_in "$A" 'xxd data/wal | head -8' 3.0
  say "Ten-byte header per entry: magic, CRC32, length. Then the key and value in the clear."
  pause 2
  say "And CD at every entry boundary. That is what the reader looks for to find its footing."
  pause 3
}

act3_durability() {
  say "Let's prove nothing lives only in memory."
  cli
  seed city seattle
  seed bird chickadee
  say "Two keys. Now kill the process outright, so the memtable goes with it."
  run_in "$MAIN" 'quit' 1.2
  say "Here is what is actually on disk."
  run_in "$A" 'ls -la data/' 2.0
  say "Start it back up."
  cli
  say "Did they come back?"
  run_in "$MAIN" 'get city'
  run_in "$MAIN" 'get bird'
  say "Rebuilt from the log, not from memory."
  pause
}

act4_corruption_recovery() {
  say "Now let's break something on purpose."
  cli
  seed city seattle
  seed bird chickadee
  run_in "$MAIN" 'quit' 1.0
  say "We flip one byte inside the first value, so its checksum no longer matches."
  run_in "$A" 'python3 demo/corrupt_wal.py' 2.0
  say "One byte. Let's restart and see what happens."
  cli
  say "The reader hits the bad checksum and scans forward to the next CD marker."
  run_in "$MAIN" 'get city'
  say "The corrupted entry is gone, as it should be."
  run_in "$MAIN" 'get bird'
  say "But everything after it survived. Corruption costs one entry, not the file."
  pause 3
}

act5_concurrent_clients() {
  say "Last one. Let's put two clients on the TCP server at once."
  tmux send-keys -t "$MAIN" './target/debug/server' Enter; sleep 1.5
  tmux send-keys -t "$A" 'nc 127.0.0.1 3000' Enter; sleep 0.8
  tmux send-keys -t "$B" 'nc 127.0.0.1 3000' Enter; sleep 0.8
  say "Two connections, two threads, one store. A writes."
  run_in "$A" 'set shared from-client-a'
  say "B reads it, on a different connection entirely."
  run_in "$B" 'get shared'
  say "Now B writes back."
  run_in "$B" 'set shared from-client-b'
  say "And A sees that too."
  run_in "$A" 'get shared'
  say "2,254 lines of Rust, 99 tests. github.com/scadoshi/chickadee"
  pause 4
}

run_act() {
  case "$1" in
    1) want_second_client no;  pin_layout; act1_cli_basics ;;
    2) want_second_client no;  pin_layout; act2_wal_on_disk ;;
    3) want_second_client no;  pin_layout; act3_durability ;;
    4) want_second_client no;  pin_layout; act4_corruption_recovery ;;
    5) want_second_client yes; pin_layout; act5_concurrent_clients ;;
    *) echo "unknown act: $1" >&2; return 1 ;;
  esac
}
