param($what)
$cdb = "C:\Program Files (x86)\Windows Kits\10\Debuggers\x64\cdb.exe"
"cdb: $(Test-Path $cdb)" | Out-File diag-info.log
if ($what -eq "engine") {
  $cmd = "cd engine && for /L %i in (1,1,40) do @(cargo test -p axiom-host --locked --test trouble -- --nocapture --test-threads 8 || exit /b 1)"
  $names = "trouble*"
} else {
  $cmd = "cd app && flutter test --reporter expanded test/host/fake_host_test.dart"
  $names = "flutter_tester*"
}
$log = Join-Path $PWD "diag-out.log"
$p = Start-Process -FilePath cmd.exe -ArgumentList "/c", "`"($cmd) > `"$log`" 2>&1`"" -PassThru -WindowStyle Hidden
$quiet = 0; $size = -1
for ($i = 0; $i -lt 900; $i += 5) {
  if ($p.HasExited) { break }
  Start-Sleep 5
  $n = (Get-Item $log -ErrorAction SilentlyContinue).Length
  if ($n -eq $size) { $quiet += 5 } else { $quiet = 0; $size = $n }
  if ($quiet -ge 100) { break }
}
"exited: $($p.HasExited) code: $($p.ExitCode) quiet: $quiet" | Out-File diag-info.log -Append
Get-Process | Where-Object { $_.ProcessName -like $names -or $_.ProcessName -like "dart*" -or $_.ProcessName -like "axiom*" } | Format-Table Id, ProcessName, CPU, HandleCount | Out-String | Out-File diag-info.log -Append
if (-not $p.HasExited) {
  foreach ($q in (Get-Process | Where-Object { $_.ProcessName -like $names })) {
    $c = Start-Process -FilePath $cdb -ArgumentList "-pv", "-p", $q.Id, "-c", "`".lines; ~*kn 40; !handle 0 0; qd`"" -RedirectStandardOutput "diag-stack-$($q.Id).log" -PassThru -WindowStyle Hidden
    if (-not $c.WaitForExit(120000)) { "cdb hung on $($q.Id)" | Out-File diag-info.log -Append }
  }
}
Get-Content diag-info.log
"--- tail"
Get-Content $log -Tail 60
Get-ChildItem diag-stack-*.log | ForEach-Object { "--- $_"; Get-Content $_ -Tail 260 }
