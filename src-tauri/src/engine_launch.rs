// =============================================================================
// ENGINE_LAUNCH.RS - how many cores the translation engine is allowed to take
// =============================================================================
// The engine is not the only thing running while a subtitle is on screen.
// Capture, preprocessing, Windows OCR and the WebView overlay all run four
// times a second on the same cores, and on ARM64 the model runs on those cores
// too - see the execution policy in `docs/ARCHITECTURE.md`.
//
// llama.cpp left to itself takes what it wants, and measurement showed that is
// the wrong amount in both directions: its unset default was the slowest
// configuration tested, and every core was slower than most of a core. The
// value belongs here rather than in the manifest because it depends on the host
// and the manifest is one embedded document shipped to every machine.
//
// Kept apart from `hy_mt_runtime` so the arithmetic is testable without
// spawning a process, and so the measurement behind it has somewhere to live.
// =============================================================================

/// Fewest worker threads worth handing the engine.
///
/// Below this the cap costs more than the contention it avoids: llama.cpp's own
/// default beat every setting under four in the sweep, and a host small enough
/// to reach this floor cannot spare cores anyway.
pub(crate) const MIN_ENGINE_THREADS: usize = 4;

/// Cores kept away from the engine for the rest of the pipeline.
///
/// Capture, preprocessing, Windows OCR, the WebView overlay and the OS all run
/// while a translation is in flight, four times a second.
const RESERVED_CORES: usize = 4;

/// Worker threads for the engine, given the host's core count.
///
/// Measured rather than guessed, on a 12-core Snapdragon X Elite - see
/// `docs/evidence/2026-08-05-arm64-engine-thread-sweep.json`. llama.cpp's
/// unset default was the *slowest* configuration tested: 443ms median and
/// 19.3 tok/s against 337ms and 24.8 tok/s at eight threads. Six was faster
/// still on an idle host (298ms, 27.4 tok/s), but its loaded p90 was 3814ms
/// against eight's 377ms, and a subtitle app is judged on the frame where the
/// machine is busy rather than the one where it is not.
///
/// This does not fix the tail. Every setting in that sweep, including this one,
/// still produced calls over fifty seconds under sustained load - which is why
/// the translation slot needs its own deadline rather than a better thread
/// count. See issue #60.
pub(crate) fn worker_threads(available_cores: usize) -> usize {
    available_cores
        .saturating_sub(RESERVED_CORES)
        .max(MIN_ENGINE_THREADS)
}

/// The manifest's launch arguments, plus a thread count matched to this host.
///
/// Thread count cannot live in the manifest the way the other arguments do: the
/// manifest is one embedded document shipped to every machine, and the right
/// value depends on cores the manifest cannot know. A manifest that pins one
/// anyway is honoured - llama.cpp takes the last `--threads` it parses, so
/// appending a second would silently overrule a deliberate choice made for a
/// host we could not measure.
pub(crate) fn launch_args(extra_args: &[String], available_cores: usize) -> Vec<String> {
    let mut args = extra_args.to_vec();
    if extra_args.iter().any(|argument| pins_threads(argument)) {
        return args;
    }
    if !MEASURED_ON_THIS_ARCHITECTURE {
        // llama.cpp's own default counts physical cores. That is a worse choice
        // than the measured one here, and a better choice than applying it
        // somewhere it was never measured - see `MEASURED_ON_THIS_ARCHITECTURE`.
        return args;
    }
    args.push("--threads".to_string());
    args.push(worker_threads(available_cores).to_string());
    args
}

/// Whether the sweep behind `worker_threads` applies to this host.
///
/// The measurement was taken on a Snapdragon X Elite: twelve physical cores and
/// no SMT, so `available_parallelism` counted physical cores there. On an SMT
/// host it counts logical ones, and "logical minus four" can exceed the physical
/// count outright - an 8-core/16-thread x64 laptop would be handed twelve
/// threads, over-subscribing the very cores this module exists to protect. The
/// evidence file says as much in its own limitations; this is the code agreeing
/// with it rather than extrapolating past it.
const MEASURED_ON_THIS_ARCHITECTURE: bool = cfg!(target_arch = "aarch64");

/// Whether an argument sets the thread count in any of the forms llama.cpp takes.
///
/// `--threads=8` matters as much as `--threads 8`: missing it would append a
/// second thread count, and llama.cpp honours the last one it parses - silently
/// overruling exactly the deliberate choice this check exists to protect.
fn pins_threads(argument: &str) -> bool {
    let name = argument.split_once('=').map_or(argument, |(name, _)| name);
    name == "--threads" || name == "-t"
}

/// Cores this host reports, or zero when it will not say.
///
/// Zero falls through to `MIN_ENGINE_THREADS`, which is the safe direction: an
/// unknown host gets the small cap rather than every core it might have.
pub(crate) fn available_cores() -> usize {
    std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_manifest::EngineManifest;

    // The engine is one of several things competing for this machine. Measured
    // on a 12-core Snapdragon X Elite (see
    // docs/evidence/2026-08-05-arm64-engine-thread-sweep.json): llama.cpp's
    // default was the slowest setting tested, and 8 was the only one that also
    // held its loaded p90 near its median.
    #[test]
    fn worker_threads_leave_headroom_for_the_rest_of_the_pipeline() {
        assert_eq!(worker_threads(12), 8);
        assert_eq!(worker_threads(16), 12);
    }

    // Capture, OCR and the overlay still need somewhere to run on a small host,
    // and a thread count of zero or one would be worse than the default.
    #[test]
    fn worker_threads_stay_within_bounds_on_small_and_absurd_hosts() {
        assert_eq!(worker_threads(1), MIN_ENGINE_THREADS);
        assert_eq!(worker_threads(4), MIN_ENGINE_THREADS);
        assert_eq!(worker_threads(0), MIN_ENGINE_THREADS);
        assert!(worker_threads(256) <= 256);
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn launch_args_gain_a_measured_thread_count() {
        let extra = vec![
            "--jinja".to_string(),
            "--parallel".to_string(),
            "1".to_string(),
        ];
        let args = launch_args(&extra, 12);
        assert_eq!(
            args,
            vec!["--jinja", "--parallel", "1", "--threads", "8"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
    }

    // The sweep was run on a host with no SMT, where `available_parallelism`
    // counts physical cores. Elsewhere it counts logical ones, and the rule
    // would over-subscribe rather than protect - so llama.cpp's own default,
    // which counts physical cores, is left in place.
    #[cfg(not(target_arch = "aarch64"))]
    #[test]
    fn launch_args_are_untouched_where_the_rule_was_not_measured() {
        let extra = vec!["--jinja".to_string()];
        assert_eq!(launch_args(&extra, 16), extra);
    }

    // The manifest is the authority. A pinned thread count is a deliberate
    // choice for a machine we could not measure, and silently appending a
    // second `--threads` would let llama.cpp pick whichever it parsed last.
    #[test]
    fn a_manifest_that_pins_threads_is_left_alone() {
        for pinned in [
            vec!["--threads".to_string(), "3".to_string()],
            vec!["-t".to_string(), "3".to_string()],
            // llama.cpp accepts the joined form too, and missing it would append
            // a second count that silently wins.
            vec!["--threads=3".to_string()],
            vec!["-t=3".to_string()],
        ] {
            let args = launch_args(&pinned, 12);
            assert_eq!(args, pinned, "{pinned:?} should pass through unchanged");
            assert_eq!(
                args.iter().filter(|a| pins_threads(a)).count(),
                1,
                "{pinned:?} should still carry exactly one thread count"
            );
        }
    }

    // Asserts what the name says, which is also the part that is true on every
    // architecture: the manifest leaves the thread count to this module. Phrased
    // as "the shipped manifest gains --threads 8" it passed on the aarch64 dev
    // host and failed on CI's x64 runner, where the measured rule deliberately
    // does not apply.
    #[test]
    fn the_shipped_manifest_does_not_pin_threads() {
        let manifest = EngineManifest::shipped().expect("manifest should be valid");
        assert!(!manifest
            .launch
            .extra_args
            .iter()
            .any(|argument| pins_threads(argument)));
    }

    // The host must report something usable; a zero here would silently pin
    // every machine to the floor.
    #[test]
    fn this_host_reports_its_cores() {
        assert!(available_cores() >= 1);
    }
}
