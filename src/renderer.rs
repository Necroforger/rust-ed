//! handles rendering an editor state

use crate::editor::Editor;
use crate::vector::Vector2;

/// contains parameters for rendering
#[derive(Clone, Copy, Debug)]
pub struct RenderOpts {
    pub view: Rect<f64>,

    // scale at which to display editor content
    pub scale: f64,
}

impl Default for RenderOpts {
    fn default() -> Self {
        Self {
            view: Rect {
                location: Vector2(0., 0.),
                width: 0,
                height: 0,
            },
            scale: 1.,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Rect<T> {
    pub location: Vector2<T>,
    pub width: i32,
    pub height: i32,
}

impl<T> Rect<T>
where
    T: std::ops::Add<Output = T> + num::cast::FromPrimitive + std::cmp::PartialOrd + Copy + 'static,
    T: std::ops::Add<Output = T>
        + std::ops::Mul<Output = T>
        + std::ops::Div<Output = T>
        + std::ops::Sub<Output = T>
        + Copy
        + 'static,
{
    /// return the area of a rectangle
    pub fn area(&self) -> i32 {
        self.width * self.height
    }

    pub fn x(&self) -> T {
        self.location.x()
    }
    pub fn y(&self) -> T {
        self.location.y()
    }

    pub fn contains(&self, p: impl Into<Vector2<T>>) -> bool {
        let p = p.into();
        let width: T = T::from_i32(self.width).unwrap();
        let height: T = T::from_i32(self.height).unwrap();

        return (p.x() >= self.location.x() && p.x() < self.location.x() + width)
            && (p.y() >= self.location.y() && p.y() < self.location.y() + height);
    }

    /// return the center of the view relative to the width and height
    pub fn center(&self) -> Vector2<T> {
        Vector2(
            T::from_i32(self.width).unwrap() / T::from_i32(2).unwrap(),
            T::from_i32(self.height).unwrap() / T::from_i32(2).unwrap(),
        )
    }

    /// center point relative to the view location
    pub fn center_point(&self) -> Vector2<T> {
        self.location.add(self.center())
    }
}

pub trait Renderer {
    type Output;
    fn render(&self, editor: &Editor, opts: RenderOpts) -> Self::Output;
}

/// renders an editor state to a string
pub struct StringRenderer {
    // only render a particular line in the editor
    pub line_hint: Option<i32>,
    pub break_on_line_end: bool,
}

impl StringRenderer {
    pub fn new() -> Self {
        Self {
            line_hint: None,
            break_on_line_end: false,
        }
    }

    pub fn with_line_hint(line: i32) -> Self {
        Self {
            line_hint: Some(line),
            break_on_line_end: false,
        }
    }
}

impl Renderer for StringRenderer {
    type Output = String;

    fn render(&self, editor: &Editor, opts: RenderOpts) -> Self::Output {
        // draw the rectangle
        let mut screen: String = String::with_capacity(opts.view.area() as usize);

        let width = opts.view.width;

        let height = if let Some(_) = self.line_hint {
            1
        } else {
            opts.view.height as i32
        };

        let y2 = if let Some(line) = self.line_hint {
            line
        } else {
            opts.view.location.y().round() as i32
        };

        let x2 = opts.view.location.x().round() as i32;

        for y in y2..y2 + height {
            for x in x2..x2 + width {
                let x = (x as f64 * opts.scale) as i32;
                let y = (y as f64 * opts.scale) as i32;
                if let Some(cell) = editor.get_cell((x, y)) {
                    screen.push(cell.char);
                } else if self.break_on_line_end && x > 0 {
                    break;
                } else {
                    screen.push(' ');
                }
            }
            screen.push('\n')
        }

        screen
    }
}

#[cfg(test)]
mod tests {
    //    use super::*;
    //    const SAMPLE_TEXT: &'static str = include_str!("../resources/sample_text.txt");

    //    #[test]
    //    fn test_string_renderer() {
    //        let editor = Editor::from(SAMPLE_TEXT);
    //        let renderer = StringRenderer();
    //
    //        let renderOpts = RenderOpts {
    //            view: Rect {
    //                location: Vector2(0, 0),
    //                width: 180,
    //                height: 25,
    //            }
    //        };
    //
    //        let text = renderer.render(&editor, renderOpts);
    //        panic!("\n{}", text);
    //    }
}
