//! 📄 인쇄: a word list (단어 / 뜻) or a sentence list (문장 / 뜻) as an A4 page, written to
//! `print/` in the data folder and opened in the browser, which shows its print dialog.

use std::path::{Path, PathBuf};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// What a sheet lists.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sheet {
    /// Short words: one A4 page, in two columns when the list is long.
    Words,
    /// Sentences: one per row, over as many pages as they need.
    Sentences,
}

const HEAD: &str = r#"@page { size: A4 portrait; margin: 14mm 12mm; }
* { box-sizing: border-box; }
body { margin: 0; font-family: -apple-system, "Hiragino Sans", "Yu Gothic", "Apple SD Gothic Neo", "Malgun Gothic", "Noto Sans JP", "Noto Sans KR", sans-serif; color: #000; }
h1 { font-size: 15pt; margin: 0 0 4mm; }
table { border-collapse: collapse; table-layout: fixed; width: 100%; }
th, td { border-bottom: 0.3mm solid #999; padding: 0 1.5mm; text-align: left; vertical-align: middle; line-height: 1.2; }
th { border-bottom: 0.5mm solid #000; font-size: 9pt; height: 7mm; }
.n { width: 7mm; color: #666; font-size: 8pt; }
.k { color: #222; }
tr { break-inside: avoid; }
@media screen { body { width: 186mm; margin: 10mm auto; } }"#;

fn page(title: &str, style: &str, body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="ko"><head><meta charset="utf-8"><title>{title}</title>
<style>
{HEAD}
{style}
</style></head>
<body><h1>{title}</h1>{body}
<script>window.onload = () => window.print();</script>
</body></html>
"#,
        title = esc(title),
    )
}

fn table(head: &str, rows: &[(String, String)], first: usize) -> String {
    let body: String = rows
        .iter()
        .enumerate()
        .map(|(i, (w, ko))| format!("<tr><td class=n>{}</td><td class=w>{}</td><td class=k>{}</td></tr>", first + i + 1, esc(w), esc(ko)))
        .collect();
    format!("<table><thead><tr><th class=n>#</th><th class=w>{head}</th><th class=k>뜻</th></tr></thead><tbody>{body}</tbody></table>")
}

pub fn sheet_html(kind: Sheet, title: &str, rows: &[(String, String)]) -> String {
    match kind {
        Sheet::Words => word_sheet_html(title, rows),
        Sheet::Sentences => sentence_sheet_html(title, rows),
    }
}

/// Words on one A4 page: numbered 단어 / 뜻 rows, in two columns when the list is long and
/// smaller type the more rows there are. Past 100 words (too small to read on one page) it
/// goes on over more pages of 60.
pub fn word_sheet_html(title: &str, rows: &[(String, String)]) -> String {
    let per_page = if rows.len() <= 100 { rows.len().max(1) } else { 60 };
    let cols = if per_page > 24 { 2 } else { 1 };
    let per_col = per_page.div_ceil(cols).max(1);
    // 297mm − margins − title ≈ 250mm for the rows.
    let row_mm = (250.0 / per_col as f32).min(10.0);
    let font_pt = (row_mm * 1.2).clamp(8.0, 12.0);
    let pages: String = rows
        .chunks(per_page)
        .enumerate()
        .map(|(pi, page)| {
            let tables: String = page.chunks(per_col).enumerate().map(|(ci, part)| table("단어", part, pi * per_page + ci * per_col)).collect();
            format!("<div class=cols>{tables}</div>")
        })
        .collect();
    let style = format!(
        "body {{ font-size: {font_pt:.1}pt; }}\n.cols {{ display: flex; gap: 6mm; align-items: flex-start; }}\n.cols + .cols {{ break-before: page; }}\ntable {{ flex: 0 0 {table_w}; }}\nth, td {{ height: {row_mm:.2}mm; }}\n.w {{ width: {}%; font-weight: 600; }}",
        if cols == 2 { 42 } else { 38 },
        // A last page's lone column stays as wide as the columns before it.
        table_w = if cols == 2 { "calc(50% - 3mm)" } else { "100%" },
    );
    page(title, &style, &pages)
}

/// Sentences: numbered 문장 / 뜻 rows, the sentence large enough to read aloud from paper.
pub fn sentence_sheet_html(title: &str, rows: &[(String, String)]) -> String {
    let style = "body { font-size: 11pt; }\ntd { padding: 2mm 1.5mm; }\n.w { width: 55%; font-weight: 600; font-size: 12pt; }";
    page(title, style, &table("문장", rows, 0))
}

/// Writes the page to `<data>/print/<name>.html` and opens it in the browser (not in tests).
pub fn print_sheet(data_dir: &Path, name: &str, title: &str, kind: Sheet, rows: &[(String, String)]) -> anyhow::Result<PathBuf> {
    let dir = data_dir.join("print");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{name}.html"));
    std::fs::write(&path, sheet_html(kind, title, rows))?;
    if !cfg!(test) {
        open::that(&path)?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_days_words_fit_one_a4_page() {
        let rows: Vec<(String, String)> = (0..40).map(|i| (format!("word{i}"), format!("뜻 <{i}>"))).collect();
        let html = word_sheet_html("1일차 · 10월 18일 — 40개", &rows);
        assert!(html.contains("size: A4") && html.contains("window.print()"));
        assert_eq!(html.matches("<table>").count(), 2, "two columns of 20");
        assert!(html.contains("word39") && html.contains("뜻 &lt;39&gt;"), "escaped");
        // 20 rows of at most 10mm + header fit in the 269mm the margins leave.
        let row_mm = (250.0f32 / 20.0).min(10.0);
        assert!(20.0 * row_mm + 7.0 + 15.0 < 269.0);
    }

    #[test]
    fn a_long_word_list_goes_on_over_more_pages() {
        let rows: Vec<(String, String)> = (0..150).map(|i| (format!("word{i}"), "뜻".into())).collect();
        let html = word_sheet_html("지난 날 못 끝낸 새 단어", &rows);
        assert_eq!(html.matches("<div class=cols>").count(), 3, "pages of 60");
        assert_eq!(html.matches("<table>").count(), 5, "2 + 2 + the last 30 in one");
        assert!(html.contains(">150</td><td class=w>word149<"));
    }

    #[test]
    fn sentences_one_per_row() {
        let rows = vec![("I'd like to <go>.".to_string(), "가고 싶어요.".to_string()), ("See you.".into(), "또 봐요.".into())];
        let html = sheet_html(Sheet::Sentences, "문장 읽기", &rows);
        assert!(html.contains("<th class=w>문장</th>") && html.contains("I'd like to &lt;go&gt;."));
        assert_eq!(html.matches("<tr>").count(), 3);
        assert!(html.contains("window.print()"));
    }
}
