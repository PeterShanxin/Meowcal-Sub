use std::io::{self, BufRead, BufReader};
use std::process::Child;
use std::thread::JoinHandle;

const MAX_RECORD_BYTES: usize = 4096;

pub(super) fn spawn(child: &mut Child) -> Result<JoinHandle<()>, String> {
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "CORE_STDERR_MISSING".to_string())?;
    let core_pid = child.id();
    let dispatcher = tracing::dispatcher::get_default(Clone::clone);
    std::thread::Builder::new()
        .name("meowcal-core-stderr".to_string())
        .spawn(move || {
            tracing::dispatcher::with_default(&dispatcher, || {
                let result = drain(BufReader::with_capacity(MAX_RECORD_BYTES, stderr), |bytes, truncated| {
                    tracing::info!(core_pid, truncated, message = %String::from_utf8_lossy(bytes), "Core stderr");
                });
                if let Err(error) = result {
                    tracing::warn!(core_pid, %error, "Could not read Core stderr");
                }
            });
        })
        .map_err(|error| format!("CORE_STDERR_READER_START: {error}"))
}

fn drain(mut reader: impl BufRead, mut emit: impl FnMut(&[u8], bool)) -> io::Result<()> {
    let mut record = Vec::with_capacity(MAX_RECORD_BYTES);
    let mut truncated = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if !record.is_empty() {
                emit(&record, truncated);
            }
            return Ok(());
        }
        for byte in available {
            if matches!(*byte, b'\r' | b'\n') {
                if !record.is_empty() {
                    emit(&record, truncated);
                    record.clear();
                    truncated = false;
                }
            } else if record.len() < MAX_RECORD_BYTES {
                record.push(*byte);
            } else {
                // Keep draining an oversized record so diagnostics cannot block Core.
                truncated = true;
            }
        }
        let consumed = available.len();
        reader.consume(consumed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn diagnostics_preserve_failure_lines_and_unterminated_tail() {
        let input = b"GPU failed\r\nCPU fallback\ninvalid byte: \xff";
        let mut messages = Vec::new();
        drain(
            BufReader::with_capacity(3, Cursor::new(input)),
            |line, truncated| {
                messages.push((String::from_utf8_lossy(line).into_owned(), truncated));
            },
        )
        .unwrap();
        assert_eq!(
            messages,
            vec![
                ("GPU failed".to_string(), false),
                ("CPU fallback".to_string(), false),
                ("invalid byte: \u{fffd}".to_string(), false),
            ]
        );
    }

    #[test]
    fn oversized_diagnostics_are_bounded_and_next_line_is_preserved() {
        let mut input = vec![b'x'; MAX_RECORD_BYTES * 3];
        input.extend_from_slice(b"\nfallback reason\n");
        let mut messages = Vec::new();
        drain(
            BufReader::with_capacity(11, Cursor::new(input)),
            |line, truncated| {
                messages.push((line.to_vec(), truncated));
            },
        )
        .unwrap();
        assert_eq!(
            messages,
            vec![
                (vec![b'x'; MAX_RECORD_BYTES], true),
                (b"fallback reason".to_vec(), false),
            ]
        );
    }
}
