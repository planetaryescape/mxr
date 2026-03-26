Output docs/demo.gif

Set Shell "bash"
Set FontSize 18
Set Width 1200
Set Height 900
Set Padding 24
Hide Mouse true

Type "export TMP_ROOT=$(mktemp -d)"
Enter
Sleep 200ms

Type "export MXR_CONFIG_DIR=$TMP_ROOT/config MXR_DATA_DIR=$TMP_ROOT/data MXR_SOCKET_PATH=$TMP_ROOT/mxr.sock"
Enter
Sleep 200ms

Type "./target/debug/mxr version"
Enter
Sleep 800ms

Type "./target/debug/mxr status --format json"
Enter
Sleep 1200ms

Type "./target/debug/mxr search 'is:unread' --format json"
Enter
Sleep 1200ms
