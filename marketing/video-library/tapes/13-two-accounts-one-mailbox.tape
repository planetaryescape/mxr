Output source/two-accounts.webm

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

Type "mxr accounts"
Sleep 400ms
Enter
Sleep 3s

Type "mxr search 'from:notes@lists.demo.mxr.local' --format table --limit 6"
Sleep 400ms
Enter
Sleep 4s
