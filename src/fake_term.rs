use cursive::theme::Style;
use cursive::theme::*;
use cursive::utils::span::SpannedString;
use std::convert::TryInto as _;
use std::io::Error as ioError;
use std::io::Result as ioResult;
use std::io::{ErrorKind, Write};
use term::color::Color as TermColor;
use term::Attr as TermAttr;
use term::Error as TermError;
use term::Result as TermResult;
use term::Terminal;

#[derive(Debug, Clone)]
pub struct FakeTermString {
    span_string: SpannedString<Style>,
    current_style: Style,
}

impl FakeTermString {

}

impl AsRef<SpannedString<Style>> for FakeTermString {
    fn as_ref(&self) -> &SpannedString<Style> {
        &self.span_string
    }
}

impl AsMut<SpannedString<Style>> for FakeTermString {
    fn as_mut(&mut self) -> &mut SpannedString<Style> {
        &mut self.span_string
    }
}

impl Into<SpannedString<Style>> for FakeTermString {
    fn into(self) -> SpannedString<Style> {
        self.span_string
    }
}

impl Write for FakeTermString {
    fn write(&mut self, buf: &[u8]) -> ioResult<usize> {
        let buf_str = match String::from_utf8(buf.into()) {
            Ok(s) => s,
            Err(e) => return Err(ioError::new(ErrorKind::InvalidData, e)),
        };
        self.span_string.append_styled(buf_str, self.current_style);
        Ok(buf.len())
    }

    fn flush(&mut self) -> ioResult<()> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct FakeTerm {
    inner: FakeTermString,
}

impl FakeTerm {
    pub fn new() -> Self {
        Self {
            inner: FakeTermString {
                span_string: SpannedString::new(),
                current_style: Style::none(),
            }
        }
    }
}

impl Terminal for FakeTerm {
    type Output = FakeTermString;
    fn fg(&mut self, color: TermColor) -> TermResult<()> {
        let color256: u8 = color
            .try_into()
            .map_err(|e| ioError::new(ErrorKind::InvalidData, e))?;
        let mut color_style = self
            .inner
            .current_style
            .color
            .unwrap_or(ColorStyle::primary());
        color_style.front = ColorType::Color(Color::from_256colors(color256));
        self.inner.current_style.color = Some(color_style);
        Ok(())
    }

    fn bg(&mut self, color: TermColor) -> TermResult<()> {
        let color256: u8 = color
            .try_into()
            .map_err(|e| ioError::new(ErrorKind::InvalidData, e))?;
        let mut color_style = self
            .inner
            .current_style
            .color
            .unwrap_or(ColorStyle::terminal_default());
        color_style.back = ColorType::Color(Color::from_256colors(color256));
        self.inner.current_style.color = Some(color_style);
        Ok(())
    }

    fn attr(&mut self, attr: TermAttr) -> TermResult<()> {
        match attr {
            TermAttr::Bold => {
                self.inner.current_style.effects ^= Effect::Bold;
            }
            TermAttr::Reverse => {
                self.inner.current_style.effects ^= Effect::Reverse;
            }
            TermAttr::Italic(true) => {
                self.inner.current_style.effects.insert(Effect::Italic);
            }
            TermAttr::Italic(false) => {
                self.inner.current_style.effects.remove(Effect::Italic);
            }
            TermAttr::Underline(true) => {
                self.inner.current_style.effects.insert(Effect::Underline);
            }
            TermAttr::Underline(false) => {
                self.inner.current_style.effects.remove(Effect::Underline);
            }
            TermAttr::Standout(true) => {
                self.inner.current_style.effects.insert(Effect::Reverse);
            }
            TermAttr::Standout(false) => {
                self.inner.current_style.effects.remove(Effect::Reverse);
            }
            TermAttr::ForegroundColor(c) => {
                self.fg(c)?;
            }
            TermAttr::BackgroundColor(c) => {
                self.bg(c)?;
            }
            _ => {
                return Err(TermError::NotSupported);
            }
        }
        Ok(())
    }

    fn supports_attr(&self, attr: TermAttr) -> bool {
        match attr {
            TermAttr::Bold
            | TermAttr::Reverse
            | TermAttr::Italic(_)
            | TermAttr::Underline(_)
            | TermAttr::Standout(_)
            | TermAttr::ForegroundColor(_)
            | TermAttr::BackgroundColor(_) => true,
            _ => false,
        }
    }

    fn reset(&mut self) -> TermResult<()> {
        self.inner.current_style = Style::none();
        Ok(())
    }

    fn supports_reset(&self) -> bool {
        true
    }

    fn supports_color(&self) -> bool {
        true
    }

    fn cursor_up(&mut self) -> TermResult<()> {
        Err(TermError::NotSupported)
    }

    fn delete_line(&mut self) -> TermResult<()> {
        Err(TermError::NotSupported)
    }

    fn carriage_return(&mut self) -> TermResult<()> {
        self.write("\n".as_bytes())?;
        Ok(())
    }

    fn get_ref(&self) -> &Self::Output {
        &self.inner
    }

    fn get_mut(&mut self) -> &mut Self::Output {
        &mut self.inner
    }

    fn into_inner(self) -> Self::Output {
        self.inner
    }
}

impl Write for FakeTerm {
    fn write(&mut self, buf: &[u8]) -> ioResult<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> ioResult<()> {
        self.inner.flush()
    }
}
