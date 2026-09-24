Output source/safety-dry-run.webm

Require mxr

Set Shell "zsh"
Set FontFamily "JetBrains Mono"
Set FontSize 21
Set Width 1280
Set Height 720
Set Padding 22
Set Framerate 30
Set TypingSpeed 22ms
Set Theme "Catppuccin Mocha"

Hide
Type "clear" Enter
Sleep 300ms
Show

Type "mxr archive --search 'from:notes@lists.demo.mxr.local is:unread' --dry-run"
Sleep 500ms
Enter
Sleep 5s
