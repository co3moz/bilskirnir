//! Pins down how Tesseract's TSV becomes words in desktop pixels, how those words make lines when
//! sparse text mode scatters them over blocks, and where a text is found among them.
use super::*;

/// A word row of Tesseract's TSV, read from an image enlarged twice.
fn row(
    block: u32,
    left: u32,
    top: u32,
    width: u32,
    height: u32,
    confidence: f32,
    text: &str,
) -> String {
    format!(
        "5\t1\t{block}\t1\t1\t1\t{}\t{}\t{}\t{}\t{confidence}\t{text}\n",
        left * 2,
        top * 2,
        width * 2,
        height * 2
    )
}

/// What a terminal and a panel of an XFCE desktop read as, each word in a block of its own or
/// with a few others, as sparse text mode reads them.
fn desktop() -> String {
    let mut tsv = String::from(
        "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n",
    );
    tsv.push_str("1\t1\t0\t0\t0\t0\t0\t0\t2560\t1440\t-1\t\n");
    tsv.push_str("2\t1\t1\t0\t0\t0\t40\t14\t160\t26\t-1\t\n");
    for (block, left, top, width, height, confidence, text) in [
        (1, 20, 7, 80, 13, 96.0, "Applications"),
        (2, 139, 7, 54, 10, 78.7, "Terminal"),
        (3, 340, 10, 60, 10, 91.2, "xmessage"),
        (11, 238, 213, 10, 9, 26.9, "e]"),
        (29, 460, 457, 47, 12, 96.3, "Build"),
        (29, 521, 457, 76, 12, 96.4, "finished"),
        (29, 610, 457, 17, 12, 95.8, "in"),
        (30, 641, 457, 37, 12, 95.9, "12.4"),
        (30, 691, 457, 66, 12, 97.0, "seconds"),
        (33, 592, 496, 35, 11, 96.5, "root"),
        (34, 642, 496, 35, 11, 96.4, "root"),
        (35, 720, 495, 37, 12, 96.8, "4096"),
        (36, 770, 495, 27, 12, 95.9, "Oct"),
        (36, 810, 495, 18, 12, 96.6, "29"),
    ] {
        tsv.push_str(&row(block, left, top, width, height, confidence, text));
    }
    tsv
}

fn texts(lines: &[Line]) -> Vec<String> {
    lines.iter().map(Line::text).collect()
}

#[test]
fn words_are_kept_by_confidence_and_their_boxes_scaled_back_to_desktop_pixels() {
    let words = parse(&desktop(), 2.0, (0, 0), (1280, 720), 50.0);
    assert_eq!(words.len(), 13, "{words:?}");
    assert!(words.iter().all(|word| word.text != "e]"));
    assert_eq!(
        words[0],
        Word {
            text: "Applications".into(),
            x: 20,
            y: 7,
            width: 80,
            height: 13,
            confidence: 96.0
        }
    );
    // A region's reading is placed where the region is on the desktop.
    let placed = parse(&desktop(), 2.0, (100, 50), (1280, 720), 0.0);
    assert_eq!(placed.len(), 14);
    assert_eq!((placed[0].x, placed[0].y), (120, 57));
    // A box is kept within what was read, and an odd edge rounds outward.
    let edge = row(1, 0, 0, 0, 0, 90.0, "x").replace("\t0\t0\t0\t0\t90", "\t2555\t3\t9\t9\t90");
    let word = &parse(&edge, 2.0, (0, 0), (1280, 720), 0.0)[0];
    assert_eq!((word.x, word.y, word.width, word.height), (1277, 1, 3, 5));
}

#[test]
fn rows_that_are_not_words_or_not_tsv_are_left_out() {
    let tsv = "level\tpage_num\n4\t1\t1\t1\t1\t0\t0\t0\t10\t10\t-1\t\n5\t1\t1\t1\t1\t1\t0\t0\t10\t10\t90\t \nnot a row\n5\t1\t1\t1\t1\t1\tx\t0\t10\t10\t90\tword\n";
    assert!(parse(tsv, 1.0, (0, 0), (100, 100), 0.0).is_empty());
}

#[test]
fn words_on_a_row_make_a_line_unless_a_column_lies_between_them() {
    let words = parse(&desktop(), 2.0, (0, 0), (1280, 720), 50.0);
    assert_eq!(
        texts(&lines(&words)),
        [
            "Applications",
            "Terminal",
            "xmessage",
            "Build finished in 12.4 seconds",
            "root root",
            "4096 Oct 29",
        ]
    );
    let line = &lines(&words)[3];
    assert_eq!(
        line.json(),
        serde_json::json!({"text":"Build finished in 12.4 seconds","x":460,"y":457,"width":297,"height":12,"confidence":96})
    );
}

#[test]
fn a_text_is_found_within_a_line_whatever_its_case_and_spacing() {
    let lines = lines(&parse(&desktop(), 2.0, (0, 0), (1280, 720), 50.0));
    let found = find(&lines, "FINISHED   in 12.4", false);
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].json(),
        serde_json::json!({"text":"finished in 12.4","x":521,"y":457,"width":157,"height":12,"center_x":599,"center_y":463,"confidence":96})
    );
    // Part of a word finds the whole word, and every place it is in.
    let roots = find(&lines, "oo", false);
    assert_eq!(roots.len(), 2);
    assert!(roots.iter().all(|found| found.words.len() == 1));
    assert_eq!(roots[1].json()["x"], 642);
    assert_eq!(find(&lines, "erm", false)[0].json()["text"], "Terminal");
    assert_eq!(find(&lines, "Terminal", false).len(), 1);
    assert!(find(&lines, "terminal", true).is_empty());
    // A text that spans two lines, or only spaces, is not found.
    assert!(find(&lines, "seconds root", false).is_empty());
    assert!(find(&lines, "  ", false).is_empty());
}

fn word(text: &str, x: u32, y: u32, width: u32, confidence: f32) -> Word {
    Word {
        text: text.into(),
        x,
        y,
        width,
        height: 12,
        confidence,
    }
}

#[test]
fn two_readings_merge_into_each_word_once_as_the_surer_one_read_it() {
    // Sparse text mode merged the last word into the window's edge; page mode read it.
    let sparse = vec![
        word("Thoric", 52, 80, 100, 91.0),
        word("‘4217", 220, 80, 70, 37.0),
    ];
    let page = vec![
        word("4217", 222, 80, 66, 96.0),
        word("reads", 160, 80, 52, 97.0),
        word("Thoric", 53, 81, 98, 92.0),
        word("Cancel", 52, 120, 48, 96.0),
    ];
    let merged = merge([sparse, page]);
    assert_eq!(texts(&lines(&merged)), ["Thoric reads 4217", "Cancel"]);
    let thoric = merged.iter().find(|w| w.text == "Thoric").unwrap();
    assert_eq!((thoric.x, thoric.confidence), (53, 92.0));
    // Words side by side that touch are two words.
    let touching = merge([
        vec![word("a", 0, 0, 10, 90.0), word("b", 9, 0, 10, 90.0)],
        vec![],
    ]);
    assert_eq!(touching.len(), 2);
}
