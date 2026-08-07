Output tapes/output/register.gif

Set FontSize 20
Set Width 800
Set Height 400
Set Theme {"background":"#FDF8F3","foreground":"#1A1512","cursor":"#C47E5A","black":"#1A1512","red":"#E8505A","green":"#7EBA5A","yellow":"#FFB020","blue":"#58D5FF","magenta":"#FF6B8A","cyan":"#58D5FF","white":"#F5E6D3","brightBlack":"#7A6B63","brightRed":"#FF4D6D","brightGreen":"#7EBA5A","brightYellow":"#FFB020","brightBlue":"#58D5FF","brightMagenta":"#FF6B8A","brightCyan":"#58D5FF","brightWhite":"#FDF8F3"}
Set Padding 24
Set MarginFill "#FDF8F3"
Set Margin 24
Set BorderRadius 12
Set WindowBar Colorful
Set TypingSpeed 50ms

Type "./target/debug/ferret --database ferret.register.db init"
Enter
Sleep 1.5s
Type "./target/debug/ferret --database ferret.register.db register --slug elci-group/ferret"
Enter
Sleep 1.5s
