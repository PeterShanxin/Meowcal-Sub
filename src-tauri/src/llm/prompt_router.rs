// =============================================================================
// PROMPT_ROUTER.RS - Deterministic subtitle prompt building
// =============================================================================
// Implements the "Prompt Router + Subtitle Context Cache" guide:
// - Choose CN vs EN instruction template deterministically.
// - Apply strict, character-based caps for source/context to stabilize latency.
// - Build a single instruction prompt suitable for MT-style models (e.g. HY-MT1.5).
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptTemplateLanguage {
    Chinese,
    English,
}

#[derive(Debug, Clone, Copy)]
pub struct PromptRouterOptions {
    pub enable_context: bool,
    pub max_context_chars: usize,
    pub max_source_chars: usize,
}

#[derive(Debug, Clone)]
pub struct BuiltPrompt {
    pub prompt: String,
    pub template_language: PromptTemplateLanguage,
    pub used_context: bool,
}

pub fn build_subtitle_translation_prompt(
    source_text: &str,
    src_lang: Option<&str>,
    tgt_lang: &str,
    context_text: Option<&str>,
    options: PromptRouterOptions,
) -> Option<BuiltPrompt> {
    let cleaned_source = clean_source_text(source_text);
    if cleaned_source.is_empty() {
        return None;
    }

    let src_primary = src_lang.and_then(primary_lang);
    let tgt_primary = primary_lang(tgt_lang).unwrap_or_else(|| "und".to_string());
    let template_language = if is_zh_family(&tgt_primary)
        || src_primary.as_deref().map(is_zh_family).unwrap_or(false)
    {
        PromptTemplateLanguage::Chinese
    } else {
        PromptTemplateLanguage::English
    };

    let target_label = target_language_label(&tgt_primary, template_language);

    let clipped_source = truncate_chars(&cleaned_source, options.max_source_chars);

    let context = if options.enable_context {
        context_text
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| clip_context_keep_recent_lines(value, options.max_context_chars))
            .filter(|value| !value.is_empty())
    } else {
        None
    };

    let (used_context, prompt) = match (template_language, context) {
        (PromptTemplateLanguage::Chinese, Some(ctx)) => (
            true,
            format!(
                "{ctx}\n参考上面的信息，把下面的文本翻译成{target_label}，注意不需要翻译上文，也不要额外解释：{clipped_source}"
            ),
        ),
        (PromptTemplateLanguage::English, Some(ctx)) => (
            true,
            format!(
                "{ctx}\nDo NOT translate or repeat the text above. Based on the information above, translate the following segment into {target_label}, without additional explanation. {clipped_source}"
            ),
        ),
        (PromptTemplateLanguage::Chinese, None) => (
            false,
            format!(
                "将以下文本翻译为{target_label}，注意只需要输出翻译后的结果，不要额外解释：{clipped_source}"
            ),
        ),
        (PromptTemplateLanguage::English, None) => (
            false,
            format!(
                "Translate the following segment into {target_label}, without additional explanation. {clipped_source}"
            ),
        ),
    };

    Some(BuiltPrompt {
        prompt,
        template_language,
        used_context,
    })
}

pub fn clean_source_text(text: &str) -> String {
    collapse_whitespace(text).trim().to_string()
}

fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !in_space {
                out.push(' ');
                in_space = true;
            }
        } else {
            in_space = false;
            out.push(ch);
        }
    }
    out
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    text.chars().take(max_chars).collect::<String>()
}

fn clip_context_keep_recent_lines(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return String::new();
    }

    // Keep most recent lines (tail), but preserve original order.
    let mut kept_rev: Vec<String> = Vec::new();
    let mut chars_used = 0usize;
    for line in lines.iter().rev() {
        let additional = line.chars().count() + if kept_rev.is_empty() { 0 } else { 1 };
        if chars_used + additional > max_chars {
            if kept_rev.is_empty() {
                // Single overlong line: truncate it rather than returning empty.
                return truncate_chars(line, max_chars);
            }
            break;
        }
        kept_rev.push((*line).to_string());
        chars_used += additional;
    }

    kept_rev.reverse();
    let mut joined = kept_rev.join("\n");
    if joined.chars().count() > max_chars {
        joined = truncate_chars(&joined, max_chars);
    }
    joined
}

pub fn primary_lang(tag: &str) -> Option<String> {
    let trimmed = tag.trim();
    if trimmed.is_empty() {
        return None;
    }

    let primary = trimmed.split('-').next().unwrap_or(trimmed).trim();
    if primary.is_empty() {
        return None;
    }

    Some(primary.to_ascii_lowercase())
}

pub fn is_zh_family(primary: &str) -> bool {
    matches!(primary, "zh" | "yue")
}

fn target_language_label(primary: &str, template_language: PromptTemplateLanguage) -> String {
    match template_language {
        PromptTemplateLanguage::Chinese => match primary {
            "zh" => "中文".to_string(),
            "yue" => "粤语".to_string(),
            "en" => "英语".to_string(),
            "ja" => "日语".to_string(),
            "ko" => "韩语".to_string(),
            "fr" => "法语".to_string(),
            "de" => "德语".to_string(),
            "es" => "西班牙语".to_string(),
            _ => primary.to_string(),
        },
        PromptTemplateLanguage::English => match primary {
            "zh" => "Chinese".to_string(),
            "yue" => "Cantonese".to_string(),
            "en" => "English".to_string(),
            "ja" => "Japanese".to_string(),
            "ko" => "Korean".to_string(),
            "fr" => "French".to_string(),
            "de" => "German".to_string(),
            "es" => "Spanish".to_string(),
            _ => primary.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_uses_cn_template_for_en_to_zh() {
        let built = build_subtitle_translation_prompt(
            "Hello",
            Some("en-US"),
            "zh-CN",
            None,
            PromptRouterOptions {
                enable_context: false,
                max_context_chars: 600,
                max_source_chars: 200,
            },
        )
        .unwrap();

        assert_eq!(built.template_language, PromptTemplateLanguage::Chinese);
        assert!(built.prompt.starts_with("将以下文本翻译为"));
    }

    #[test]
    fn router_uses_cn_template_for_zh_to_en() {
        let built = build_subtitle_translation_prompt(
            "你好",
            Some("zh-CN"),
            "en-US",
            None,
            PromptRouterOptions {
                enable_context: false,
                max_context_chars: 600,
                max_source_chars: 200,
            },
        )
        .unwrap();

        assert_eq!(built.template_language, PromptTemplateLanguage::Chinese);
        assert!(built.prompt.starts_with("将以下文本翻译为"));
    }

    #[test]
    fn router_uses_en_template_for_ja_to_en() {
        let built = build_subtitle_translation_prompt(
            "こんにちは",
            Some("ja-JP"),
            "en",
            None,
            PromptRouterOptions {
                enable_context: false,
                max_context_chars: 600,
                max_source_chars: 200,
            },
        )
        .unwrap();

        assert_eq!(built.template_language, PromptTemplateLanguage::English);
        assert!(built.prompt.starts_with("Translate the following segment"));
    }

    #[test]
    fn uses_context_template_when_enabled() {
        let built = build_subtitle_translation_prompt(
            "He left.",
            Some("en"),
            "zh-CN",
            Some("Alice is here.\nBob left."),
            PromptRouterOptions {
                enable_context: true,
                max_context_chars: 100,
                max_source_chars: 200,
            },
        )
        .unwrap();

        assert!(built.used_context);
        assert!(built.prompt.contains("参考上面的信息"));
        assert!(built.prompt.contains("Alice is here."));
    }
}
