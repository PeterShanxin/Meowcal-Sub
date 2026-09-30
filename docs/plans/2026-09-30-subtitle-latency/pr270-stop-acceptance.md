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

## Fresh native result

Windows ARM64 isolated CPU validation completed at
2026-09-30T17:41:50.4549069Z using the application built from
`5d7b28d188790365b81b31cadbab46cba2f2c3ad`:

- Binary SHA256:
  `FB30E9D836A7E98254FA4FD50ABC8CF86361885D3C6515AAC8BECC16113E4C30`.
- Harness commit: `554170949433be55a577bc5a820daa801fabb571`.
- Stop: 261.1 ms; immediate Start: 13 ms.
- Stopped session 2; session 3 translated authored “Good morning.” to “早上好。”.
  Native overlay visibility and rendered text passed; capture remained running.
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

## Integration boundary

Current-main integration includes PR271 readiness changes and PR277 Store
automation at main `d9a6cc0953c26b7f99cdaff3e05b26399270983a`. Hosted final-head
checks cover the combined source. The native result above uses the explicit
earlier binary; it is not a fresh merged-native build or an x64 physical-device
result. Store run 36734279190 previously passed both architectures; historical
run 36606250148 lacks sufficient diagnostics to attribute its anomaly.
