pub(super) fn sanitize_subtitle_translation_output(output: &str) -> String {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let trimmed = trimmed
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('\u{201C}')
        .trim_matches('\u{201D}')
        .trim_matches('`')
        .trim();

    let mut collected: Vec<String> = Vec::with_capacity(4);
    let mut started = false;
    for raw_line in trimmed.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            if started {
                break;
            }
            continue;
        }

        let lower = line.to_ascii_lowercase();
        let is_header = lower.starts_with("translation")
            || lower.starts_with("translated")
            || lower.starts_with("output")
            || line.starts_with("翻译")
            || line.starts_with("译文");
        let is_explanation = lower.starts_with("explanation")
            || lower.starts_with("note")
            || line.starts_with("解释")
            || line.starts_with("说明");

        if !started {
            if is_header {
                if let Some((_, rest)) = line.split_once(':') {
                    let rest = rest.trim();
                    if !rest.is_empty() {
                        collected.push(rest.to_string());
                        started = true;
                    }
                }
                continue;
            }
            if is_explanation {
                continue;
            }
            started = true;
        }

        if started && is_explanation {
            break;
        }
        collected.push(line.to_string());
    }

    if collected.is_empty() {
        trimmed.to_string()
    } else {
        collected.join("\n")
    }
}
