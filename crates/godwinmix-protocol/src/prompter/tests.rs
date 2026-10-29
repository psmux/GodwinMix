use super::*;

fn texts(rows: &[Row]) -> Vec<&str> {
    rows.iter().map(|r| r.text.as_str()).collect()
}

#[test]
fn a_line_is_broken_at_spaces_and_a_blank_line_is_a_row() {
    let rows = rows("Good evening and welcome\n\nto the news", 12);
    assert_eq!(texts(&rows), ["Good evening", "and welcome", "", "to the news"]);
    assert_eq!(rows.iter().map(|r| r.line).collect::<Vec<_>>(), [0, 0, 1, 2]);
}

#[test]
fn asterisks_do_not_count_and_a_long_word_is_cut() {
    let rows = rows("a **bold** word\nabcdefghijklmn", 12);
    assert_eq!(texts(&rows), ["a **bold** word", "abcdefghijkl", "mn"]);
}

#[test]
fn a_marker_is_a_heading_row_with_its_start() {
    let rows = rows("# Opening\nHello\n#\nBye\n## The weather tonight", 12);
    assert!(rows[0].marker && !rows[1].marker);
    assert_eq!(rows[0].text, "Opening");
    let found = markers(&rows);
    let names: Vec<&str> = found.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["Opening", "marker 2", "The weather tonight"]);
    assert_eq!(found.iter().map(|m| m.row).collect::<Vec<_>>(), [0, 2, 4]);
    assert_eq!(rows.len(), 6, "the weather heading wraps to two rows");
}

#[test]
fn the_position_moves_only_while_playing_and_stays_inside_the_script() {
    let mut state = PrompterState {
        script: Some("news".into()),
        title: None,
        updated_ms: 0,
        rows: 10,
        markers: vec![],
        position: 2.0,
        at_ms: 1_000,
        speed: 0.5,
        playing: true,
        look: Look::default(),
        now_ms: 1_000,
        revision: 1,
    };
    assert_eq!(state.position_at(5_000), 4.0);
    assert_eq!(state.position_at(100_000), 10.0);
    state.speed = -1.0;
    assert_eq!(state.position_at(10_000), 0.0);
    state.playing = false;
    assert_eq!(state.position_at(10_000), 2.0);
}
