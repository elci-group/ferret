Output tapes/output/predict.gif

Set FontSize 18
Set Width 960
Set Height 520
Set Theme {"background":"#FDF8F3","foreground":"#1A1512","cursor":"#C47E5A","black":"#1A1512","red":"#E8505A","green":"#7EBA5A","yellow":"#FFB020","blue":"#58D5FF","magenta":"#FF6B8A","cyan":"#58D5FF","white":"#F5E6D3","brightBlack":"#7A6B63","brightRed":"#FF4D6D","brightGreen":"#7EBA5A","brightYellow":"#FFB020","brightBlue":"#58D5FF","brightMagenta":"#FF6B8A","brightCyan":"#58D5FF","brightWhite":"#FDF8F3"}
Set Padding 24
Set MarginFill "#FDF8F3"
Set Margin 24
Set BorderRadius 12
Set WindowBar Colorful
Set TypingSpeed 50ms

Type "cat > sample.diff <<'EOF'"
Enter
Type "diff --git a/src/lib.rs b/src/lib.rs"
Enter
Type "--- a/src/lib.rs"
Enter
Type "+++ b/src/lib.rs"
Enter
Type "@@ -1,3 +1,4 @@"
Enter
Type " fn old() {}"
Enter
Type "+fn new() {}"
Enter
Type "EOF"
Enter
Sleep 500ms
Type "bash tapes/scripts/seed.sh ferret.predict.db"
Enter
Sleep 800ms
Type "./target/debug/ferret --database ferret.predict.db predict --repo elci-group/ferret --base abc123 --head def456 --diff sample.diff"
Enter
Sleep 2.5s
