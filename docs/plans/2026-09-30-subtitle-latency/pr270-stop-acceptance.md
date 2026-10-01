# Stop cancellation acceptance

## Contract and diagnosis

Stop drops pending OCR work, invalidates its session and joins bounded capture
cleanup before acknowledging completion. `session_lifecycle.rs` tests require
delayed work to be dropped and stopped-session results to be suppressed.
`CancelOnDrop` signals Core transport cancellation; OCR requests do not drain
active work on drop. Cancellation can terminate the original OCR transport.

The original delayed benchmark required that transport to survive and resume.
Its two native failures remain invalid: the diagnostic rerun recorded exit code
1 and `NtResumeProcess` status `0xC000010A` (process terminating). Those values
support cancellation compatibility, but do not identify the exact termination
caller. No application defect or performance improvement follows from them.

`scripts/acceptance-delayed-ocr.ps1` instead requires the original verified OCR
process to be alive before Stop, then reaped. Stop must complete within 3 seconds
(bounded capture teardown plus overlay fade and IPC margin), become idle and
allow immediate Start. A later session must produce the new authored translation
in the visible native overlay; old-session or old-cue output fails acceptance.
Interrupted trials still check resume/cleanup for the retained original handle.

## Recorded native result and strengthened gate

Windows ARM64 isolated CPU validation completed at
2026-09-30T17:41:50.4549069Z using the application built from
`5d7b28d188790365b81b31cadbab46cba2f2c3ad`:

- Binary SHA256:
  `FB30E9D836A7E98254FA4FD50ABC8CF86361885D3C6515AAC8BECC16113E4C30`.
- Harness commit: `554170949433be55a577bc5a820daa801fabb571`.
- Stop: 261.1 ms; immediate Start: 13 ms.
- Stopped session 2; session 3 translated authored “Good morning.” to “早上好。”.
  Native window visibility and DOM text matched; capture remained running.
  This trial did not independently check subtitle-container visibility/bounds.
- Original suspended OCR exited with code 1. No stale result appeared during
  the additional observation interval.
- Retained inference and model process handles/creation times remained identical.
  Loaded free RAM was 9.3 GiB.
- All test-owned processes exited, debug port 9241 was released, and config hashes
  remained unchanged after restoration. User processes were preserved.

Local evidence is `reconciled-native-20260930-174132/{verdict,acceptance}.json` in
the task workspace. Two preceding CDP-discovery preflight failures and the
original invalid native trials are retained locally. No private config backups,
screen captures or raw logs are published with this summary.

This is functional cancellation/restart/output acceptance. It does not measure
pixel presentation latency, prove a speedup, or establish general movie
reliability. The prior Windows All, normal restart 10/10 and authored pipeline
10/10 results also bind `5d7b28d`; they were not repeated unnecessarily.

The second review identified four harness defects. Failed reruns now replace the
nominal output with an invalid verdict as well as preserving the failure
companion. Interrupted trials stop only their token-owned session and unregister
their listener; cleanup errors invalidate output. Stale checks inspect events
after the recorded Stop boundary, including old-session non-text events. Native
overlay acceptance now requires a visible subtitle container and text with
positive rendered bounds inside the container and viewport, in addition to
native window visibility and correct text. Regressions cover each failure mode
in PowerShell 7 and 5.1 and execute the actual embedded JavaScript.

Fresh native verification of this strengthened surface assertion passed on
2026-10-01T04:22:17.6646966Z with harness commit
`b2b21ae70af0d89789d6d69f80631d3b9bc13348` (SHA256
`CCB6C14945D2C8A523DA84CB2FC231BFA7A4D27790B32CA581A60EC7E698D98F`)
and the same accepted application binary:

- Stop: 239.2 ms; immediate Start: 14.1 ms; stopped session 2 and new session 3.
- Original verified OCR process was reaped with exit code 1. The new session
  produced “Good morning.” → “早上好。” through the local engine, with no stale
  text or quiet event after the recorded Stop boundary.
- Native overlay and subtitle surface were visible. Container bounds were
  `(100, 630, 1200, 53.2)` and text bounds `(120.8, 638.8, 1158.4, 33.6)`,
  contained within the visible container and viewport.
- Original inference/model handles and creation times stayed unchanged; loaded
  free RAM was 9.55 GiB. All owned processes exited, config hashes were restored
  unchanged, and port 9241 was released.

Selected authored evidence is
`reconciled-native-20261001-042157/{verdict,acceptance}.json` in the task workspace.
This closes the strengthened functional native gate; it adds no pixel latency
or performance claim. Previous failed and limited samples remain preserved.

Two earlier preflights stopped before inference loading or OCR suspension:
`reconciled-native-20260930-181318` and `reconciled-native-20260930-181435`.
The latter recorded only the owned overlay WebView, with no main WebView for
20 seconds; a concurrent read-only inventory found `LogonUI.exe` active. The
desktop was later confirmed locked by WTS inspection. Exact startup causality
is unproven. Both attempts
cleaned all owned processes, restored config hashes and released port 9241.
Do not substitute the earlier DOM-only result for the strengthened native gate.
No desktop unlock or power-state change was attempted.

## Integration boundary

Current-main integration includes PR271 readiness changes and PR277 Store
automation at main `d9a6cc0953c26b7f99cdaff3e05b26399270983a`. Hosted final-head
checks cover the combined source. The native result above uses the explicit
earlier binary; it is not a fresh merged-native build or an x64 physical-device
result. Store run 36734279190 previously passed both architectures; historical
run 36606250148 lacks sufficient diagnostics to attribute its anomaly.
