Output source/attachment-search.webm

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

Type "mxr search 'from:weekly@lists.demo.mxr.local has:attachment' --format table --limit 8"
Sleep 500ms
Enter
Sleep 5s
