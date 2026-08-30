//! 3form-backed animated result trails for interactive Ferret commands.

use form3::term::TermInfo;
use form3::{Aura, GrowthTrack, Mode};
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const FRAME_DELAY: Duration = Duration::from_millis(18);
const ANIMATION_STEPS: usize = 6;
static ENABLED: AtomicBool = AtomicBool::new(false);
static MACHINE: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ChaseFont {
    round: usize,
    ferret: [&'static str; 3],
    rabbit: [&'static str; 3],
}

/// Six successive font mutations. Each round retains a recognisable species
/// silhouette while increasing expression and motion differentiation.
const FONT_ROUNDS: [ChaseFont; 6] = [
    ChaseFont {
        round: 1,
        ferret: ["~o>", "~~o>", "~o>>"],
        rabbit: ["(o)", "(O)", "(!)"],
    },
    ChaseFont {
        round: 2,
        ferret: ["~(o)>", "~~(^)>", "~(>)>>"],
        rabbit: ["/(o)\\", "/(^)\\", "/(!)\\"],
    },
    ChaseFont {
        round: 3,
        ferret: ["~(o.o)>", "~~(^.o)>", "~(>.<)>>"],
        rabbit: ["/(o.o)\\", "/(^.^)\\", "/(!.!)\\"],
    },
    ChaseFont {
        round: 4,
        ferret: ["~(•.•)>", "~~(ᵔ.•)>", "~(>.<)>>"],
        rabbit: ["/(•.•)\\", "/(ᵔ.ᵔ)\\", "/(!.!)\\"],
    },
    ChaseFont {
        round: 5,
        ferret: ["~(•ᴥ•)>", "~~(ᵔᴥᵔ)>", "~(•̀ᴥ•́)>>"],
        rabbit: ["/(•×•)\\", "/(O×o)\\", "/(˃×˂)\\"],
    },
    ChaseFont {
        round: 6,
        ferret: ["≈~(•ᴥ•)>", "≈~~(ᵔᴥᵔ)>", "≈~(•̀ᴥ•́)>>"],
        rabbit: ["/(•×•)\\", "/(O×O)\\", "/(˃×˂)\\"],
    },
];

#[derive(Debug, Clone, Copy)]
struct Evaluation {
    font: ChaseFont,
    score: usize,
    rounds: usize,
}

/// Enable animated UI output after CLI mode negotiation. Machine/JSON output
/// remains byte-stable, as required by the 3form environment contract.
pub fn configure(machine_output: bool) {
    let explicitly_disabled = std::env::var("FERRET_ANIMATION")
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "0" | "false" | "FALSE"));
    let mode = Mode::auto();
    MACHINE.store(machine_output || mode == Mode::Machine, Ordering::Relaxed);
    ENABLED.store(
        !machine_output && !explicitly_disabled && mode == Mode::Ui && TermInfo::detect().is_tty,
        Ordering::Relaxed,
    );
}

/// Shared sink used by every human-facing `println!` in the Ferret CLI.
pub fn print_line(text: &str) {
    if !ENABLED.load(Ordering::Relaxed) {
        if MACHINE.load(Ordering::Relaxed) {
            std::println!("{}", strip_ansi(text));
        } else {
            std::println!("{text}");
        }
        return;
    }

    if text.is_empty() {
        std::println!();
        return;
    }

    for line in text.split('\n') {
        if line.is_empty() {
            std::println!();
        } else if animate_line(line).is_err() {
            // Output failures must never turn decorative animation into a
            // command failure. Fall back to the original result line.
            std::println!("{line}");
        }
    }
}

fn animate_line(line: &str) -> io::Result<()> {
    let term = TermInfo::detect();
    let evaluation = evaluate_rounds(0, None);
    debug_assert_eq!(evaluation.rounds, FONT_ROUNDS.len());
    let font = evaluation.font;
    let visible_total = visible_len(line).max(1);
    let aura = Aura::new((92, 48, 25), (196, 126, 90), (255, 230, 211));
    let track = GrowthTrack::new(3).with_glyphs('·', '·', '✦', '›');
    let mut stdout = io::stdout().lock();

    for step in 0..ANIMATION_STEPS {
        let progress = (step + 1) as f32 / ANIMATION_STEPS as f32;
        let pose = step % font.ferret.len();
        let revealed = visible_prefix(line, ((visible_total as f32) * progress).ceil() as usize);
        let remaining = visible_total.saturating_sub(visible_len(&revealed));
        let ferret = aura.paint(font.ferret[pose], progress, term.color_support);
        let rabbit = aura.paint(font.rabbit[pose], 1.0 - progress, term.color_support);
        write!(
            stdout,
            "\r\x1b[2K{revealed}{} {ferret}{}{rabbit}",
            track.render(progress),
            " ".repeat(remaining + 1)
        )?;
        stdout.flush()?;
        std::thread::sleep(FRAME_DELAY);
    }

    writeln!(
        stdout,
        "\r\x1b[2K{line} ··· {}  {}",
        aura.paint(font.ferret[2], 1.0, term.color_support),
        aura.paint(font.rabbit[2], 0.0, term.color_support)
    )?;
    stdout.flush()
}

fn evaluate_rounds(index: usize, best: Option<Evaluation>) -> Evaluation {
    if index == FONT_ROUNDS.len() {
        return best.unwrap_or(Evaluation {
            font: FONT_ROUNDS[0],
            score: 0,
            rounds: 0,
        });
    }
    let font = FONT_ROUNDS[index];
    let candidate = Evaluation {
        font,
        score: score_font(&font),
        rounds: index + 1,
    };
    let best = match best {
        Some(current) if current.score >= candidate.score => current,
        _ => candidate,
    };
    evaluate_rounds(index + 1, Some(best))
}

fn score_font(font: &ChaseFont) -> usize {
    let unique_ferrets = font
        .ferret
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let unique_rabbits = font
        .rabbit
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let species = usize::from(font.ferret.iter().all(|pose| pose.contains('~')))
        + usize::from(
            font.rabbit
                .iter()
                .all(|pose| pose.starts_with('/') && pose.ends_with('\\')),
        );
    let expression = font
        .ferret
        .iter()
        .chain(font.rabbit.iter())
        .flat_map(|pose| pose.chars())
        .filter(|glyph| matches!(glyph, '•' | 'ᴥ' | 'ᵔ' | '˃' | '˂' | '×' | '̀' | '́'))
        .count();
    unique_ferrets * 3 + unique_rabbits * 3 + species * 5 + expression + font.round
}

fn visible_len(text: &str) -> usize {
    let mut count = 0;
    let mut bytes = text.as_bytes().iter().copied().peekable();
    while let Some(byte) = bytes.next() {
        if byte == 0x1b && bytes.peek() == Some(&b'[') {
            bytes.next();
            for code in bytes.by_ref() {
                if code.is_ascii_alphabetic() {
                    break;
                }
            }
        } else if byte & 0b1100_0000 != 0b1000_0000 {
            count += 1;
        }
    }
    count
}

fn visible_prefix(text: &str, limit: usize) -> String {
    let mut output = String::new();
    let mut visible = 0;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            output.push(ch);
            while let Some(code) = chars.next() {
                output.push(code);
                if code.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        if visible >= limit {
            break;
        }
        output.push(ch);
        visible += 1;
    }
    if output.contains("\x1b[") {
        output.push_str("\x1b[0m");
    }
    output
}

fn strip_ansi(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            for code in chars.by_ref() {
                if code.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            output.push(ch);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use form3::term::ColorSupport;

    #[test]
    fn six_recursive_rounds_select_the_most_expressive_font() {
        let evaluation = evaluate_rounds(0, None);
        assert_eq!(evaluation.rounds, 6);
        assert_eq!(evaluation.font.round, 6);
        assert!(evaluation.font.ferret[2].contains('ᴥ'));
        assert!(evaluation.font.rabbit[2].contains('˃'));
    }

    #[test]
    fn selected_font_has_three_mutated_poses_per_animal() {
        let font = evaluate_rounds(0, None).font;
        assert_eq!(
            font.ferret
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            3
        );
        assert_eq!(
            font.rabbit
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            3
        );
    }

    #[test]
    fn ansi_aware_reveal_never_splits_style_sequences() {
        let styled = "\x1b[32mFERRET\x1b[0m result";
        assert_eq!(visible_len(styled), 13);
        let prefix = visible_prefix(styled, 3);
        assert!(prefix.contains("\x1b[32mFER"));
        assert!(prefix.ends_with("\x1b[0m"));
    }

    #[test]
    fn machine_cleanup_removes_ansi_without_losing_text() {
        assert_eq!(strip_ansi("\x1b[32mFERRET\x1b[0m result"), "FERRET result");
    }

    #[test]
    fn threeform_track_never_preprints_future_cells() {
        let track = GrowthTrack::new(8).with_glyphs('·', '·', '✦', '›');
        assert!(track.render(0.25).chars().count() < track.render(0.75).chars().count());
    }

    #[test]
    fn no_color_fonts_are_still_legible() {
        let font = evaluate_rounds(0, None).font;
        let painted = Aura::mystic().paint(font.ferret[0], 0.5, ColorSupport::NoColor);
        assert_eq!(painted, font.ferret[0]);
        assert!(!painted.contains("\x1b["));
    }
}
