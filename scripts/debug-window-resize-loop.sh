#!/usr/bin/env bash
set -euo pipefail

exe="${1:-target/resize-debug/release/MoeFrame.exe}"
if [[ ! -f "$exe" ]]; then
  printf 'Missing diagnostic executable: %s\n' "$exe" >&2
  exit 1
fi

running="$(cmd.exe /c 'tasklist /FI "IMAGENAME eq MoeFrame.exe" /NH' | tr -d '\r')"
if printf '%s\n' "$running" | grep -qi 'MoeFrame.exe'; then
  printf 'Close every MoeFrame.exe instance first; no process was stopped.\n%s\n' "$running" >&2
  exit 1
fi

appdata="$(cmd.exe /c 'echo %APPDATA%' | tr -d '\r')"
log="$(cygpath -u "$appdata")/MoeFrame/moe_frame.log"
exe_win="$(cygpath -aw "$exe")"
printf 'Launching: %s\nLog: %s\n' "$exe_win" "$log"
read -r -p 'Press Enter to launch the diagnostic build. ' _
powershell.exe -NoProfile -Command "Start-Process -FilePath '$exe_win'"
sleep 2

read -r -p 'In the app, do 10 resize cycles: drag bottom-center down/up, then top-center down/up. Wait briefly after each release. Press Enter when finished. ' _
read -r -p 'How many of the 10 cycles failed to change height? Enter a number: ' failures
printf '\nReported failures: %s\n' "$failures"
printf '\nRecent resize diagnostic records:\n'
if [[ -f "$log" ]]; then
  rg -F '[DEBUG-resize-f317]' "$log" | tail -n 120 || true
else
  printf 'Diagnostic log not found.\n'
fi
