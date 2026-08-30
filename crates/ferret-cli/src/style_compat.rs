//! Drop-in shim for the small slice of `console::style`/`console::Style` this
//! crate used, backed by `form3::compat::Colorize` instead. Kept the same
//! call shape (`style(value).bold().cyan()`, `Style::new().bold()` +
//! `.apply_to(value)`) so call sites did not need to change, only the
//! `use console::{style, Style};` import line.

use form3::ansi::Color;
use form3::compat::Colorize;
use std::fmt;

#[derive(Debug, Clone, Copy, Default)]
struct Effects {
    fg: Option<Color>,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
}

impl Effects {
    fn render(self, text: &str) -> String {
        let mut styled = text.normal();
        if let Some(color) = self.fg {
            styled = styled.color(color);
        }
        if self.bold {
            styled = styled.bold();
        }
        if self.dim {
            styled = styled.dimmed();
        }
        if self.italic {
            styled = styled.italic();
        }
        if self.underline {
            styled = styled.underline();
        }
        styled.to_string()
    }
}

macro_rules! effect_methods {
    ($self_ty:ty, $field:ident) => {
        pub fn bold(mut self) -> $self_ty {
            self.$field.bold = true;
            self
        }
        pub fn dim(mut self) -> $self_ty {
            self.$field.dim = true;
            self
        }
        pub fn italic(mut self) -> $self_ty {
            self.$field.italic = true;
            self
        }
        pub fn underlined(mut self) -> $self_ty {
            self.$field.underline = true;
            self
        }
        pub fn red(mut self) -> $self_ty {
            self.$field.fg = Some(Color::Red);
            self
        }
        pub fn green(mut self) -> $self_ty {
            self.$field.fg = Some(Color::Green);
            self
        }
        pub fn yellow(mut self) -> $self_ty {
            self.$field.fg = Some(Color::Yellow);
            self
        }
        pub fn cyan(mut self) -> $self_ty {
            self.$field.fg = Some(Color::Cyan);
            self
        }
    };
}

/// Mirrors `console::Style`: an effect bundle built once, applied to many values.
#[derive(Debug, Clone, Copy, Default)]
pub struct Style {
    effects: Effects,
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply_to<T: fmt::Display>(&self, value: T) -> StyledValue<T> {
        StyledValue {
            value,
            effects: self.effects,
        }
    }

    effect_methods!(Self, effects);
}

/// Mirrors `console::style(value)`: a value paired with effects, rendered on `Display`.
pub struct StyledValue<T> {
    value: T,
    effects: Effects,
}

pub fn style<T: fmt::Display>(value: T) -> StyledValue<T> {
    StyledValue {
        value,
        effects: Effects::default(),
    }
}

impl<T: fmt::Display> StyledValue<T> {
    effect_methods!(Self, effects);
}

impl<T: fmt::Display> fmt::Display for StyledValue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.effects.render(&self.value.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `form3::term`'s colour override is process-global, so these tests must
    // not run concurrently with each other.
    static OVERRIDE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn style_fn_chains_color_and_attributes() {
        let _guard = OVERRIDE_LOCK.lock().unwrap();
        form3::term::set_override(true);
        let rendered = style("x").red().bold().to_string();
        form3::term::unset_override();
        assert!(rendered.contains('x'));
        assert!(rendered.contains("\x1b["));
    }

    #[test]
    fn style_struct_applies_to_multiple_values() {
        let _guard = OVERRIDE_LOCK.lock().unwrap();
        form3::term::set_override(true);
        let header = Style::new().bold().underlined();
        let styled = header.apply_to("a").to_string();
        let styled_again = header.apply_to("a").to_string();
        let plain = Style::new().apply_to("a").to_string();
        form3::term::unset_override();
        assert_eq!(styled_again, styled);
        assert_ne!(styled, plain);
    }
}
