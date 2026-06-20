use windows_sys::Win32::Foundation::RECT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowGeometry {
    pub width: i32,
    pub height: i32,
    pub x: Option<i32>,
    pub y: Option<i32>,
}

impl WindowGeometry {
    pub fn from_rect(rect: RECT) -> Self {
        Self {
            width: rect_dimension(rect.left, rect.right),
            height: rect_dimension(rect.top, rect.bottom),
            x: Some(rect.left),
            y: Some(rect.top),
        }
    }

    pub fn to_config_string(self) -> String {
        match (self.x, self.y) {
            (Some(x), Some(y)) => format!(
                "{}x{}{}{}",
                self.width,
                self.height,
                signed_geometry_coord(x),
                signed_geometry_coord(y)
            ),
            _ => format!("{}x{}", self.width, self.height),
        }
    }
}

fn rect_dimension(start: i32, end: i32) -> i32 {
    end.saturating_sub(start).max(1)
}

fn signed_geometry_coord(value: i32) -> String {
    if value >= 0 {
        format!("+{value}")
    } else {
        value.to_string()
    }
}

pub fn parse_window_geometry(value: &str) -> Option<WindowGeometry> {
    let trimmed = value.trim();
    let separator = trimmed.find(['x', 'X'])?;
    let width = parse_positive_i32(&trimmed[..separator])?;
    let rest = &trimmed[separator + 1..];
    let height_len: usize = rest
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .map(char::len_utf8)
        .sum();
    if height_len == 0 {
        return None;
    }

    let height = parse_positive_i32(&rest[..height_len])?;
    let position = &rest[height_len..];
    if position.is_empty() {
        return Some(WindowGeometry {
            width,
            height,
            x: None,
            y: None,
        });
    }

    let (x, y) = parse_position(position)?;
    Some(WindowGeometry {
        width,
        height,
        x: Some(x),
        y: Some(y),
    })
}

fn parse_positive_i32(value: &str) -> Option<i32> {
    if value.is_empty() || !value.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    value.parse::<i32>().ok().filter(|value| *value > 0)
}

fn parse_position(value: &str) -> Option<(i32, i32)> {
    let mut chars = value.char_indices();
    let (_, x_sign) = chars.next()?;
    if !matches!(x_sign, '+' | '-') {
        return None;
    }
    let y_sign_index =
        chars.find_map(|(index, character)| matches!(character, '+' | '-').then_some(index))?;

    let x = parse_signed_i32(&value[..y_sign_index])?;
    let y = parse_signed_i32(&value[y_sign_index..])?;
    Some((x, y))
}

fn parse_signed_i32(value: &str) -> Option<i32> {
    if value.len() < 2 {
        return None;
    }
    let (sign, digits) = value.split_at(1);
    if digits.is_empty() || !digits.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    let magnitude = digits.parse::<i64>().ok()?;
    let signed = match sign {
        "+" => magnitude,
        "-" => -magnitude,
        _ => return None,
    };
    i32::try_from(signed).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tk_style_geometry() {
        assert_eq!(
            parse_window_geometry("631x324+943+1873"),
            Some(WindowGeometry {
                width: 631,
                height: 324,
                x: Some(943),
                y: Some(1873),
            })
        );
        assert_eq!(
            parse_window_geometry("800x600"),
            Some(WindowGeometry {
                width: 800,
                height: 600,
                x: None,
                y: None,
            })
        );
        assert_eq!(
            parse_window_geometry("800x600-10+20"),
            Some(WindowGeometry {
                width: 800,
                height: 600,
                x: Some(-10),
                y: Some(20),
            })
        );
    }

    #[test]
    fn rejects_invalid_geometry_without_panicking() {
        for value in ["", "x600", "800x", "0x600", "800x600+", "800x600+1"] {
            assert_eq!(parse_window_geometry(value), None);
        }
    }

    #[test]
    fn formats_config_geometry() {
        let rect = RECT {
            left: 10,
            top: 20,
            right: 810,
            bottom: 620,
        };

        assert_eq!(
            WindowGeometry::from_rect(rect).to_config_string(),
            "800x600+10+20"
        );
    }

    #[test]
    fn formats_negative_geometry_coordinates_without_extra_plus() {
        let rect = RECT {
            left: -10,
            top: -20,
            right: 790,
            bottom: 580,
        };

        let formatted = WindowGeometry::from_rect(rect).to_config_string();

        assert_eq!(formatted, "800x600-10-20");
        assert_eq!(
            parse_window_geometry(&formatted),
            Some(WindowGeometry {
                width: 800,
                height: 600,
                x: Some(-10),
                y: Some(-20),
            })
        );
    }

    #[test]
    fn rect_dimensions_do_not_panic_on_extreme_coordinates() {
        let rect = RECT {
            left: i32::MIN,
            top: i32::MAX,
            right: i32::MAX,
            bottom: i32::MIN,
        };

        let geometry = WindowGeometry::from_rect(rect);

        assert_eq!(geometry.width, i32::MAX);
        assert_eq!(geometry.height, 1);
    }
}
