# Vynglish full test runner.
# Save this file in the project folder (next to Cargo.toml), then run:
#   powershell -ExecutionPolicy Bypass -File .\run-tests.ps1

Set-Location $PSScriptRoot

Write-Host "Building..." -ForegroundColor Cyan
cargo build -q 2>$null
if ($LASTEXITCODE -ne 0) { Write-Host "Build failed. Run 'cargo build' to see why." -ForegroundColor Red; exit 1 }

$exe = Join-Path $PSScriptRoot "target\debug\vyn.exe"
if (-not (Test-Path $exe)) { Write-Host "Cannot find $exe" -ForegroundColor Red; exit 1 }

$tmp = Join-Path $env:TEMP "vyn-tests"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
Get-ChildItem $tmp -Filter *.vyn -ErrorAction SilentlyContinue | Remove-Item -Force

$script:pass = 0
$script:fail = 0
$script:failures = New-Object System.Collections.ArrayList

function L { return ($args -join "`n") }

function Invoke-Vyn {
    param([string]$Arguments = "", [string]$Stdin = "")
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $exe
    $psi.Arguments = $Arguments
    $psi.WorkingDirectory = $tmp
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $p = [System.Diagnostics.Process]::Start($psi)
    $outTask = $p.StandardOutput.ReadToEndAsync()
    $errTask = $p.StandardError.ReadToEndAsync()
    if ($Stdin) { $p.StandardInput.Write($Stdin) }
    $p.StandardInput.Close()
    if (-not $p.WaitForExit(20000)) {
        try { $p.Kill() } catch {}
        return "TIMEOUT"
    }
    return ($outTask.Result + $errTask.Result)
}

function Norm([string]$s) { return ($s -replace "`r", "").Trim() }

function Report($name, $ok, $expected, $got, $show) {
    if ($ok) {
        $script:pass++
        Write-Host ("PASS  " + $name) -ForegroundColor Green
    } else {
        $script:fail++
        Write-Host ("FAIL  " + $name) -ForegroundColor Red
        [void]$script:failures.Add([pscustomobject]@{ Name = $name; Expected = $expected; Got = $got })
    }
    if ($show) {
        Write-Host "      output (check the wording yourself):" -ForegroundColor DarkGray
        foreach ($line in ($got -split "`n")) { Write-Host ("      " + $line) -ForegroundColor DarkGray }
    }
}

function Check-Output($name, $out, $expect, $mode, $forbid, $show) {
    $out = Norm $out
    $expect = @($expect)
    $ok = $true
    if ($mode -eq "exact") { $ok = ($out -eq (Norm ($expect -join "`n"))) }
    elseif ($mode -eq "regex") { $ok = ($out -match $expect[0]) }
    else { foreach ($e in $expect) { if (-not $out.Contains($e)) { $ok = $false } } }
    foreach ($f in @($forbid)) { if ($out.Contains($f)) { $ok = $false } }
    if ($out -eq "TIMEOUT") { $ok = $false }
    Report $name $ok ($expect -join " | ") $out $show
}

function T-Script {
    param($Name, $Source, $Expect, $Mode = "exact", $Forbid = @(), $Stdin = "", [switch]$Crlf, [switch]$Show)
    $file = Join-Path $tmp ($Name + ".vyn")
    $text = ($Source -replace "`r`n", "`n") + "`n"
    if ($Crlf) { $text = $text -replace "`n", "`r`n" }
    [System.IO.File]::WriteAllText($file, $text)
    $out = Invoke-Vyn ('"' + $file + '"') $Stdin
    Check-Output $Name $out $Expect $Mode $Forbid $Show
}

function T-Repl {
    param($Name, $Lines, $Expect, $Forbid = @(), [switch]$Show)
    $stdin = ((@($Lines) + "exit") -join "`n") + "`n"
    $out = Invoke-Vyn "" $stdin
    Check-Output ("repl: " + $Name) $out $Expect "contains" $Forbid $Show
}

Write-Host ""
Write-Host "=== CORE ===" -ForegroundColor Cyan
T-Script "print-quoted-and-bare"   (L 'print "hello"' 'print hi') @("hello", "hi")
T-Script "variable-print"          (L 'set n to 5' 'print n') "5"
T-Script "divide-by-zero"          (L 'set x to 1 divided by 0' 'print x') "undefined"
T-Script "zero-divided-by-one"     (L 'set x to 0 divided by 1' 'print x') "0"
T-Script "repeating-decimal"       (L 'set x to 23 divided by 6' 'print x') "3.83333..."
T-Script "precedence"              (L 'set z to 2 plus 3 times 4' 'print z') "14"
T-Script "parentheses"             (L 'set z to (2 plus 3) times 4' 'print z') "20"
T-Script "wait"                    (L 'wait 1 seconds' 'print "ok"') "ok"
T-Script "comment-line"            (L '# just a comment' 'print "ok"') "ok"
T-Script "shebang-line"            (L '#!/usr/bin/env vyn' 'print "ok"') "ok"
T-Script "crlf-file"               (L 'print "first"' 'print "second"') @("first", "second") -Crlf
T-Script "ask-input"               (L 'ask "Name?" and remember it as who' 'print who') @("Bob") -Mode contains -Stdin "Bob`n"

Write-Host ""
Write-Host "=== MATH MODULE ===" -ForegroundColor Cyan
T-Script "sqrt-irrational"         (L 'use math' 'set y to square root of 2' 'print y') "about 1.4142135624"
T-Script "sqrt-perfect-square"     (L 'use math' 'set y to square root of 16' 'print y') "4"
T-Script "sqrt-negative"           (L 'use math' 'set n to 0 minus 4' 'set y to square root of n' 'print y') "undefined"
T-Script "factorial-5"             (L 'use math' 'set f to factorial of 5' 'print f') "120"
T-Script "factorial-negative"      (L 'use math' 'set n to 0 minus 3' 'set f to factorial of n' 'print f') "undefined"
T-Script "power"                   (L 'use math' 'set p to 2 to the power of 10' 'print p') "1024"
T-Script "percent"                 (L 'use math' 'set p to 10 percent of 50' 'print p') "5"
T-Script "power-too-big"           (L 'use math' 'set p to 2 to the power of 99999999' 'print p') @("too big") -Mode contains
T-Script "absolute-value"          (L 'use math' 'set n to 0 minus 5' 'set a to absolute value of n' 'print a') "5"
T-Script "smallest-of"             (L 'use math' 'set s to smallest of 3 and 7' 'print s') "3"
T-Script "biggest-of"              (L 'use math' 'set b to biggest of 3 and 7' 'print b') "7"

Write-Host ""
Write-Host "=== LOOPS AND DECISIONS ===" -ForegroundColor Cyan
T-Script "repeat-times"            (L 'repeat 3 times' 'print "x"' 'end') @("x", "x", "x")
T-Script "repeat-while"            (L 'set i to 1' 'repeat while i is less than 4' 'print i' 'set i to i plus 1' 'end') @("1", "2", "3")
T-Script "repeat-for-each"         (L 'use list' 'make a list called foo with 1, 2 and 3' 'repeat for each n in foo' 'print n' 'end') @("1", "2", "3")
T-Script "stop-leaves-loop"        (L 'repeat 5 times' 'print "x"' 'stop' 'end') "x"
T-Script "if-branch"               (L 'set n to 5' 'if n is greater than 3' 'print "big"' 'otherwise' 'print "small"' 'end') "big"
T-Script "otherwise-branch"        (L 'set n to 1' 'if n is greater than 3' 'print "big"' 'otherwise' 'print "small"' 'end') "small"
T-Script "comparisons-and-logic"   (L 'if 3 is at least 3' 'print "a"' 'end' 'if 2 is at most 2' 'print "b"' 'end' 'if 1 is not equal to 2' 'print "c"' 'end' 'if 4 is equal to 4' 'print "d"' 'end' 'if 1 is less than 2 and 5 is greater than 3' 'print "e"' 'end' 'if not 1 is greater than 2' 'print "f"' 'end' 'if 1 is greater than 2 or 2 is greater than 1' 'print "g"' 'end') @("a", "b", "c", "d", "e", "f", "g")

Write-Host ""
Write-Host "=== EXECUTE REVERSE ===" -ForegroundColor Cyan
T-Script "execute-reverse"         (L 'print "first"' 'print "last"' 'execute reverse') @("last", "first")
T-Script "execute-reverse-blocks"  (L 'print "a"' 'repeat 2 times' 'print "b"' 'print "c"' 'end' 'print "d"' 'execute reverse') @("d", "b", "c", "b", "c", "a")

Write-Host ""
Write-Host "=== TEACH ===" -ForegroundColor Cyan
T-Script "teach-with-colon"        (L 'teach "greet <name>":' '    print "Hello, <name>"' 'end' 'greet "Barkha"' 'greet Barkha') @("Hello, Barkha", "Hello, Barkha")
T-Script "teach-without-colon"     (L 'teach "greet <name>"' '    print "Hello, <name>"' 'end' 'greet "Barkha"' 'greet Barkha') @("Hello, Barkha", "Hello, Barkha")
T-Script "teach-used-before-taught" (L 'greet "Ann"' 'teach "greet <name>":' '    print "Hello, <name>"' 'end') "Hello, Ann"
T-Script "teach-override-warning"  (L 'teach "hi":' 'print "a"' 'end' 'teach "hi":' 'print "b"' 'end' 'hi') @("b") -Mode contains -Show
T-Script "teach-empty-warning"     (L 'teach "nothing here":' 'end' 'print "ok"') @("ok") -Mode contains -Show
T-Script "recursion-limit"         (L 'teach "again":' 'again' 'end' 'again') @("called itself too many times") -Mode contains

Write-Host ""
Write-Host "=== MODULES ===" -ForegroundColor Cyan
[System.IO.File]::WriteAllText((Join-Path $tmp "circ_a.vyn"), "use circ_b`n")
[System.IO.File]::WriteAllText((Join-Path $tmp "circ_b.vyn"), "use circ_a`n")
T-Script "use-twice-is-fine"       (L 'use math' 'use math' 'print "ok"') "ok"
T-Script "circular-use-is-error"   (L 'use circ_a' 'print "ok"') '(?i)circular|circle|cycle|itself' -Mode regex
T-Script "text-uppercase"          (L 'use text' 'set t to uppercase "hello"' 'print t') "HELLO"
T-Script "text-length"             (L 'use text' 'set t to length of "hello"' 'print t') "5"
T-Script "text-join"               (L 'use text' 'set t to join "a" and "b"' 'print t') "ab"
T-Script "list-item-1-indexed"     (L 'use list' 'make a list called foo with 10, 20 and 30' 'set a to item 2 of foo' 'print a') "20"
T-Script "list-add-and-length"     (L 'use list' 'make a list called foo with 10, 20 and 30' 'add 40 to foo' 'set l to length of foo' 'print l') "4"
T-Script "list-sort-first-last"    (L 'use list' 'make a list called foo with 30, 10 and 20' 'sort foo' 'set f to first of foo' 'set z to last of foo' 'print f' 'print z') @("10", "30")
T-Script "random-between"          (L 'use random' 'set r to random number between 1 and 1' 'print r') "1"
T-Script "time-current-date"       (L 'use time' 'set d to current date' 'print d') '^\d{4}-\d{2}-\d{2}$' -Mode regex

Write-Host ""
Write-Host "=== SCRIPT-MODE ERRORS ===" -ForegroundColor Cyan
T-Script "unknown-command"         (L 'shout "hi"') @("I don't know the command", "Nothing was run.") -Mode contains
T-Script "error-stops-everything"  (L 'print "before"' 'shout "hi"') @("Nothing was run.") -Mode contains -Forbid @("before")
T-Script "module-not-used"         (L 'set y to square root of 16') @("use math", "Nothing was run.") -Mode contains
T-Script "misused-command"         (L 'set bad to') @("I don't understand this use of") -Mode contains

Write-Host ""
Write-Host "=== REPL ===" -ForegroundColor Cyan
T-Repl "banner"                    @() @("Vynglish", 'Type "help", "copyright", "credits" or "license" for more information.')
T-Repl "quit-leaves"               @("quit") @("Vynglish") -Forbid @("TIMEOUT")
T-Repl "print-quoted-and-bare"     @('print "kiwi"', 'print zebra') @("kiwi", "zebra")
T-Repl "variable"                  @('set n to 5', 'print n') @("5")
T-Repl "help-opens-help-prompt"    @("help", "q") @("Welcome to Vynglish")
T-Repl "help-one-shot"             @("help print") @("print") -Show
T-Repl "copyright"                 @("copyright") @("Copyright")
T-Repl "credits"                   @("credits") @("written by") -Show
T-Repl "license"                   @("license") @("MIT")
T-Repl "unknown-command"           @("hekp") @("I don't know the command", "Did you mean: help?") -Forbid @("Line 1:1", "Nothing was run")
T-Repl "misused-command"           @("set bad to") @("I don't understand this use of")
T-Repl "module-not-used"           @("set y to square root of 16") @("use math")
T-Repl "use-then-command-next-line" @("use text", 'set t to uppercase "kiwi"', "print t") @("KIWI")
T-Repl "execute-reverse-message"   @("execute reverse") @("execute reverse only works in a script file.")
T-Repl "multi-line-teach"          @('teach "greet <name>":', '    print "Hello, <name>"', 'end', 'greet "Ann"') @("Hello, Ann")

Write-Host ""
Write-Host "==============================" -ForegroundColor Cyan
Write-Host ("Passed: {0}    Failed: {1}" -f $script:pass, $script:fail)
if ($script:failures.Count -gt 0) {
    Write-Host ""
    Write-Host "FAILED TESTS (copy everything below and send it to Claude):" -ForegroundColor Yellow
    foreach ($f in $script:failures) {
        Write-Host ("--- " + $f.Name)
        Write-Host ("expected: " + $f.Expected)
        Write-Host ("got:      " + $f.Got)
    }
}

Write-Host ""
Write-Host "Running cargo test..." -ForegroundColor Cyan
cargo test -q 2>&1 | ForEach-Object { "$_" } | Select-Object -Last 12

Write-Host ""
Write-Host "MANUAL CHECKS (a script cannot test these, do them yourself with 'cargo run'):" -ForegroundColor Yellow
Write-Host "  1. Ctrl-C at an empty prompt leaves NO blank line between prompts."
Write-Host "  2. Type: repeat 1000000000 times / wait 0 seconds / end, then Ctrl-C. It must stop and show >>>."
Write-Host "  3. F1 opens help>. In help>, type 'modules', 'commands', 'topics', 'modules text', then q."
Write-Host "  4. F3 enters paste mode: paste a teach block, press F3 again, it runs."
Write-Host "  5. F2 shows history browsing. Up and down arrows recall earlier lines."
Write-Host "  6. 'license' shows a pager: Space = next page, q = quit, Ctrl-C = quit cleanly."
Write-Host "  7. Ctrl-Z then Enter (Windows) exits the REPL."

if ($script:fail -gt 0) { exit 1 } else { exit 0 }
