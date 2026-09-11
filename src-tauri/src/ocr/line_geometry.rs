use super::LineBox;

/// Keep cleaned lines and their native rectangles index-parallel. Text cleanup
/// and optional capture diagnostics are product policy, outside native OCR.
pub fn clean_lines(lines: Vec<String>, boxes: Vec<LineBox>) -> (Vec<String>, Vec<LineBox>) {
    let mut cleaned_lines = Vec::new();
    let mut cleaned_boxes = Vec::new();
    for (index, line) in lines.into_iter().enumerate() {
        let cleaned = super::text_cleanup::clean_line(&line);
        if cleaned.trim().is_empty() {
            continue;
        }
        cleaned_lines.push(cleaned);
        cleaned_boxes.push(boxes.get(index).copied().unwrap_or(LineBox {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        }));
    }
    super::band_log::record(&cleaned_lines, &cleaned_boxes);
    (cleaned_lines, cleaned_boxes)
}
#[cfg(test)]
mod tests {
    use super::super::LineBox;

    #[test]
    fn cleanup_keeps_geometry_aligned_when_a_line_is_empty() {
        let area = LineBox {
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
        };
        let (lines, boxes) = super::clean_lines(vec![" ".into(), "你 好".into()], vec![area, area]);
        assert_eq!(lines, vec!["你好"]);
        assert_eq!(boxes, vec![area]);
    }

    #[test]
    fn missing_geometry_preserves_text_with_zero_bounds() {
        let (lines, boxes) = super::clean_lines(vec!["hello".into()], vec![]);
        assert_eq!(lines, vec!["hello"]);
        assert_eq!(boxes[0].width, 0.0);
    }

    #[test]
    fn a_line_is_grouped_by_where_its_middle_sits() {
        // Bands are formed from vertical centres, so a tall line and a short
        // one on the same baseline have to land near each other.
        let tall = LineBox {
            x: 0.0,
            y: 100.0,
            width: 50.0,
            height: 40.0,
        };
        let short = LineBox {
            x: 60.0,
            y: 110.0,
            width: 50.0,
            height: 20.0,
        };
        assert_eq!(tall.middle_y(), 120.0);
        assert_eq!(short.middle_y(), 120.0);
    }
}
