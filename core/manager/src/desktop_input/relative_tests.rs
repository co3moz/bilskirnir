//! Pins down xdotool's command line for a movement: a move for every step that moves, negative
//! offsets behind `--`, the step's pause after it and the pointer's position at the end, and how
//! that position is read back.
use super::*;

fn motion(dx: i32, dy: i32, steps: u8, pause: u64) -> Relative {
    Relative {
        dx,
        dy,
        steps,
        pause,
        mask: 0,
        from: (0, 0),
    }
}

#[test]
fn a_movement_is_one_chained_xdotool_command() {
    assert_eq!(
        command(&motion(-30, 7, 1, 0)).join(" "),
        "xdotool mousemove_relative -- -30 7 getmouselocation --shell"
    );
    // A step that does not move only pauses; the steps still add up to the offset.
    assert_eq!(
        command(&motion(2, 0, 3, 40)).join(" "),
        "xdotool sleep 0.040 mousemove_relative -- 1 0 sleep 0.040 mousemove_relative -- 1 0 sleep 0.040 getmouselocation --shell"
    );
    assert_eq!(
        command(&motion(0, 0, 1, 0)).join(" "),
        "xdotool getmouselocation --shell"
    );
}

#[test]
fn the_pointer_position_is_read_from_xdotools_shell_output() {
    assert_eq!(
        location("X=1279\nY=300\nSCREEN=0\nWINDOW=10485800\n"),
        Some((1279, 300))
    );
    assert_eq!(location("X=-4\nY=70000\n"), Some((0, u16::MAX)));
    assert_eq!(location("X=12\n"), None);
    assert_eq!(location("Error: no display"), None);
}
