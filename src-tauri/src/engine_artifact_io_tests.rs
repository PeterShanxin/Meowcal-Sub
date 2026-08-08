use super::*;
use crate::engine_manifest::EngineManifest;
use std::time::{SystemTime, UNIX_EPOCH};

// Issue #66: the reason has to reach the wizard, not a console nobody sees.
#[test]
fn an_extraction_failure_keeps_the_reason() {
    let stderr = b"Expand-Archive : The process cannot access the file \
                   'llama.dll' because it is being used by another process.\n\
                   At line:1 char:1\n";
    let reason = extraction_reason(stderr);

    assert!(reason.contains("being used by another process"));
}

// A silent failure must still say that it was silent, rather than trailing
// off into an empty string that reads like a truncated message.
#[test]
fn an_extraction_failure_without_output_says_so() {
    assert_eq!(extraction_reason(b""), "no error output");
}

// PowerShell can emit a long trace; the wizard shows this text, so the
// output stays bounded rather than pasting a whole trace into a dialog.
#[test]
fn a_long_trace_is_bounded() {
    let stderr = (1..=20)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let reason = extraction_reason(stderr.as_bytes());

    assert!(
        reason.split("; ").count() <= 4,
        "unbounded reason: {reason}"
    );
    assert!(reason.contains("line 20"), "the tail is kept: {reason}");
}

// The real shape of a Windows PowerShell 5.1 failure. The cause is at the
// top and the last four lines are a fixed trailer, so keeping "the last few
// lines" reports the trailer and throws the cause away - which is how a real
// failure still arrived as no explanation at all.
#[test]
fn the_cause_survives_powershells_trailer() {
    let stderr = b"Expand-Archive : Cannot validate argument on parameter 'LiteralPath'. \
         The argument is null or empty.\n\
         At line:1 char:169\n\
         + ... -DestinationPath $destination -Force } catch { Write-Error $_; exit 1 ...\n\
         +                                                    ~~~~~~~~~~~~~~\n\
         + CategoryInfo          : NotSpecified: (:) [Write-Error], WriteErrorException\n\
         + FullyQualifiedErrorId : Microsoft.PowerShell.Commands.WriteErrorException\n";

    let reason = extraction_reason(stderr);

    assert!(
        reason.contains("argument is null or empty"),
        "the cause was dropped: {reason}"
    );
    assert!(
        !reason.contains("FullyQualifiedErrorId"),
        "the trailer was kept instead of the cause: {reason}"
    );
}

// `-Command` makes PowerShell echo the entire script as the error's source.
// Reporting our own script back to the operator is not a diagnosis.
#[test]
fn the_echoed_script_is_not_reported_as_the_reason() {
    let stderr = b"$ErrorActionPreference = 'Stop'; try { Expand-Archive -LiteralPath \
         $env:MEOWCAL_EXTRACT_ARCHIVE -DestinationPath\n\
         $env:MEOWCAL_EXTRACT_DESTINATION -Force } catch { Write-Error $_; exit 1 } : The path\n\
         'C:\\missing\\runtime.zip' either does not exist or is not a valid file system path.\n\
         At line:1 char:164\n\
         + CategoryInfo          : NotSpecified: (:) [Write-Error], WriteErrorException\n";

    let reason = extraction_reason(stderr);

    assert!(
        reason.starts_with("The path"),
        "the script echo was kept: {reason}"
    );
    assert!(
        reason.contains("does not exist or is not a valid file system path"),
        "the cause was lost: {reason}"
    );
    assert!(
        !reason.contains("ErrorActionPreference"),
        "our own script is not a diagnosis: {reason}"
    );
}

// A locked destination is the case #66 was filed for, and its cause is also
// on the first line with the same trailer beneath it.
#[test]
fn a_locked_destination_reports_the_sharing_violation() {
    let stderr = b"Expand-Archive : The process cannot access the file 'llama.dll' \
         because it is being used by another process.\n\
         At line:1 char:1\n\
         + CategoryInfo          : NotSpecified: (:) [Write-Error], WriteErrorException\n\
         + FullyQualifiedErrorId : Microsoft.PowerShell.Commands.WriteErrorException\n";

    let reason = extraction_reason(stderr);

    assert!(
        reason.contains("being used by another process"),
        "the cause was dropped: {reason}"
    );
}

// The diagnostics above only matter if extraction itself works. `-Command`
// does not bind a `param()` block to trailing arguments, so this is what
// proves the archive and destination actually reach Expand-Archive instead
// of arriving empty.
#[cfg(target_os = "windows")]
#[tokio::test]
async fn a_real_archive_is_extracted() {
    let root = std::env::temp_dir().join(format!(
        "meowcal-extract-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let source = root.join("source");
    let destination = root.join("destination");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&destination).unwrap();
    std::fs::write(source.join("llama-server.exe"), b"payload").unwrap();

    let archive = root.join("runtime.zip");
    let compress = crate::windowless_command::tokio_command("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Compress-Archive -Path $env:MEOWCAL_TEST_SOURCE\\* \
             -DestinationPath $env:MEOWCAL_TEST_ARCHIVE -Force",
        ])
        .env("MEOWCAL_TEST_SOURCE", &source)
        .env("MEOWCAL_TEST_ARCHIVE", &archive)
        .output()
        .await
        .unwrap();
    assert!(
        compress.status.success(),
        "could not build the fixture archive: {}",
        String::from_utf8_lossy(&compress.stderr)
    );

    let result = extract_zip(&archive, &destination).await;

    assert!(result.is_ok(), "extraction failed: {result:?}");
    assert!(
        destination.join("llama-server.exe").is_file(),
        "the archive contents did not reach the destination"
    );

    std::fs::remove_dir_all(&root).ok();
}

// stderr is where PowerShell reports, but a native tool beneath it can exit
// non-zero having written only to stdout. Reporting "no error output" beside
// a real failure is the same information loss #66 is about.
#[test]
fn a_reason_on_stdout_is_used_when_stderr_is_silent() {
    let reason = failure_reason(b"", b"disk full while writing llama.dll\n");

    assert_eq!(reason, "disk full while writing llama.dll");
}

#[test]
fn stderr_wins_when_both_streams_spoke() {
    let reason = failure_reason(b"access denied\n", b"progress chatter\n");

    assert_eq!(reason, "access denied");
}

#[test]
fn silence_on_both_streams_still_says_so() {
    assert_eq!(failure_reason(b"", b""), "no error output");
}

// End-to-end against real PowerShell: a genuine failure must exit non-zero
// and carry a readable cause, not just a code. This is the shape of the
// original #66 report.
#[cfg(target_os = "windows")]
#[tokio::test]
async fn a_failing_extraction_reports_a_readable_cause() {
    let missing = std::env::temp_dir().join(format!(
        "meowcal-absent-{}-{}.zip",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let destination = std::env::temp_dir();

    let error = extract_zip(&missing, &destination)
        .await
        .expect_err("extracting an archive that does not exist must fail");

    assert!(
        error.starts_with("ENGINE_EXTRACT_FAILED:"),
        "the stable prefix changed: {error}"
    );
    assert!(
        error.contains(&missing.display().to_string()),
        "the archive is not named: {error}"
    );
    assert!(
        !error.contains("FullyQualifiedErrorId"),
        "the trailer was reported instead of the cause: {error}"
    );
    // Whatever PowerShell's wording, it has to say something beyond the code.
    let reason = error
        .split_once(" into ")
        .and_then(|(_, tail)| tail.split_once(": "))
        .map(|(_, reason)| reason)
        .unwrap_or_default();
    assert!(
        reason.trim().len() > 10,
        "no usable reason survived: {error}"
    );
}

#[tokio::test]
async fn same_sized_corrupt_artifact_is_not_treated_as_installed() {
    let path = fixture_path("integrity", "bin");
    std::fs::write(&path, b"trusted").unwrap();
    let expected_hash = sha256_file(&path).unwrap();
    assert!(file_matches(&path, 7, &expected_hash).await.unwrap());
    std::fs::write(&path, b"corrupt").unwrap();
    assert!(!file_matches(&path, 7, &expected_hash).await.unwrap());
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn corrupt_runtime_executable_requires_repair() {
    let manifest = EngineManifest::shipped().unwrap();
    let runtime = manifest.runtime_for_current_arch().unwrap();
    let path = fixture_path("runtime-corrupt", "exe");
    std::fs::write(&path, vec![0; runtime.executable.size_bytes as usize]).unwrap();
    assert!(!file_matches(
        &path,
        runtime.executable.size_bytes,
        &runtime.executable.sha256
    )
    .await
    .unwrap());
    std::fs::remove_file(path).unwrap();
}

fn fixture_path(label: &str, extension: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "meowcal-{label}-{}-{unique}.{extension}",
        std::process::id()
    ))
}
