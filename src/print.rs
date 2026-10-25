//! 📄 인쇄: a day's word list as an A4 page (단어 / 뜻), written to `print/` in the data folder
//! and opened in the browser, which shows its print dialog.

use std::path::{Path, PathBuf};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// One A4 page: the title, then numbered 단어 / 뜻 rows, in two columns when the list is long
/// and smaller type the more rows there are, so the whole list stays on the one page.
pub fn word_sheet_html(title: &str, rows: &[(String, String)]) -> String {
    let cols = if rows.len() > 24 { 2 } else { 1 };
    let per_col = rows.len().div_ceil(cols).max(1);
    // 297mm − margins − title ≈ 250mm for the rows.
    let row_mm = (250.0 / per_col as f32).min(10.0);
    let font_pt = (row_mm * 1.2).clamp(8.0, 12.0);
    let table = |part: &[(String, String)], first: usize| {
        let body: String = part
            .iter()
            .enumerate()
            .map(|(i, (w, ko))| format!("<tr><td class=n>{}</td><td class=w>{}</td><td class=k>{}</td></tr>", first + i + 1, esc(w), esc(ko)))
            .collect();
        format!("<table><thead><tr><th class=n>#</th><th class=w>단어</th><th class=k>뜻</th></tr></thead><tbody>{body}</tbody></table>")
    };
    let tables: String = rows.chunks(per_col).enumerate().map(|(i, part)| table(part, i * per_col)).collect();
    format!(
        r#"<!DOCTYPE html>
<html lang="ko"><head><meta charset="utf-8"><title>{title}</title>
<style>
@page {{ size: A4 portrait; margin: 14mm 12mm; }}
* {{ box-sizing: border-box; }}
body {{ margin: 0; font-family: -apple-system, "Hiragino Sans", "Yu Gothic", "Apple SD Gothic Neo", "Malgun Gothic", "Noto Sans JP", "Noto Sans KR", sans-serif; font-size: {font_pt:.1}pt; color: #000; }}
h1 {{ font-size: 15pt; margin: 0 0 4mm; }}
.cols {{ display: flex; gap: 6mm; align-items: flex-start; }}
table {{ flex: 1; border-collapse: collapse; table-layout: fixed; }}
th, td {{ border-bottom: 0.3mm solid #999; padding: 0 1.5mm; height: {row_mm:.2}mm; text-align: left; vertical-align: middle; line-height: 1.2; }}
th {{ border-bottom: 0.5mm solid #000; font-size: 9pt; height: 7mm; }}
.n {{ width: 7mm; color: #666; font-size: 8pt; }}
.w {{ width: {w_pct}%; font-weight: 600; }}
.k {{ color: #222; }}
tr {{ break-inside: avoid; }}
@media screen {{ body {{ width: 186mm; margin: 10mm auto; }} }}
</style></head>
<body><h1>{title}</h1><div class=cols>{tables}</div>
<script>window.onload = () => window.print();</script>
</body></html>
"#,
        title = esc(title),
        w_pct = if cols == 2 { 42 } else { 38 },
    )
}

/// Writes the page to `<data>/print/<name>.html` and opens it in the browser (not in tests).
pub fn print_words(data_dir: &Path, name: &str, title: &str, rows: &[(String, String)]) -> anyhow::Result<PathBuf> {
    let dir = data_dir.join("print");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{name}.html"));
    std::fs::write(&path, word_sheet_html(title, rows))?;
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
}
