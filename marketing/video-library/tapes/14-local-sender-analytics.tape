Output source/sender-analytics.webm

Require mxr

Set Shell "zsh"
Set FontFamily "JetBrains Mono"
Set FontSize 21
Set Width 1280
Set Height 720
Set Padding 22
Set Framerate 30
Set TypingSpeed 24ms
Set Theme "Catppuccin Mocha"

Hide
Type "clear" Enter
Sleep 300ms
Show

Type "mxr sender notes@lists.demo.mxr.local --format json | jq '{sender: .email, messages: .total_inbound, first_seen: .first_seen_at, latest: .last_seen_at, open_threads: .open_thread_count}'"
Sleep 500ms
Enter
Sleep 5s
