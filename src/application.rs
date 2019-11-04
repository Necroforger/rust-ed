use crate::clipboard::Clipboard;
use crate::editor::{Editor, Vector2};
use crate::renderer::{RenderOpts, Renderer, StringRenderer};

use crossterm::{
    cursor::MoveTo,
    input::{InputEvent, KeyEvent, SyncReader},
    screen::{self},
    terminal::{self},
    ExecutableCommand,
};

use crossterm::input::{EnableMouseCapture, MouseEvent};
use crossterm::terminal::ClearType;
use std::io::Write;

#[derive(Debug)]
pub enum EditMode {
    Command,
    Insert,
}

/// handles the main application logic
pub struct Application<T>
where
    T: Clipboard,
{
    pub editor: Editor,
    pub clipboard: T,

    // stores the current rendering offsets and widths / heights
    pub render_opts: RenderOpts,

    // when true, signals the application to exit
    pub exit: bool,
    pub log: String,

    // current edit mode
    pub edit_mode: EditMode,

    // only render a particular line
    render_line_hint: Option<i32>,

    // render until the end of the line rather
    // than the entire screen width
    render_break_line_hint: bool,
}

impl<T> Application<T>
where
    T: Clipboard,
{
    pub fn new(editor: Editor, clipboard: T) -> Application<T> {
        Application {
            editor,
            clipboard,
            render_opts: RenderOpts::default(),
            exit: false,
            log: String::new(),

            render_line_hint: None,
            render_break_line_hint: false,
            edit_mode: EditMode::Insert,
        }
    }

    /// run the application main loop
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // enter raw mode
        // switch to the alternate screen
        let _alternate = screen::AlternateScreen::to_alternate(true)?;
        // process keyboard events
        let mut reader = SyncReader {};

        // enable mouse capture
        std::io::stdout().execute(EnableMouseCapture).unwrap();

        self.render();

        loop {
            if self.exit {
                break Ok(());
            }

            if let Some(event) = reader.next() {
                self.process_event(event);
            }

            // thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    pub fn process_event(&mut self, event: InputEvent) {
        match event {
            InputEvent::Keyboard(event) => self.process_key_event(event),
            InputEvent::Mouse(event) => self.process_mouse_event(event),
            _ => {}
        }
    }

    pub fn process_mouse_event(&mut self, event: MouseEvent) {
        use MouseEvent::*;

        self.log = "Processing mouse event".to_string();

        // convert screen coordinates into editor coordinates
        macro_rules! to_editor_coords {
            ($x:ident, $y:ident) => {{
                let Vector2(x2, y2) = self.render_opts.view.location;
                ($x + x2, $y + y2)
            }};
        }

        match event {
            Press(_, x, y) => {
                let (x, y) = (x as i32, y as i32);
                let (x, y) = to_editor_coords!(x, y);
                self.log = format!("mouse: set cursor location to {}:{}", x, y);
                self.editor.set_cursor((x, y));
                self.render();
            }
            _ => self.log = "unknown mouse event".to_string(),
        }
    }

    pub fn set_cursor(&mut self, x: i32, y: i32) {
        self.editor.set_cursor((x, y));
        self.update_cursor_pos();
    }

    pub fn move_cursor(&mut self, x: i32, y: i32) {
        self.editor.move_cursor((x, y));
        self.update_cursor_pos();
    }

    pub fn move_view(&mut self, x: i32, y: i32) {
        self.render_opts.view.location = self.render_opts.view.location.add(Vector2(x, y));
        self.render();
    }

    pub fn process_key_event(&mut self, event: KeyEvent) {
        use KeyEvent::*;

        match event {
            Down => {
                self.move_cursor(0, 1);
            }
            Up => {
                self.move_cursor(0, -1);
            }
            Right => {
                self.move_cursor(1, 0);
            }
            Left => {
                self.move_cursor(-1, 0);
            }
            CtrlDown => {
                self.move_view(0, 1);
            }
            CtrlUp => {
                self.move_view(0, -1);
            }
            CtrlRight => {
                self.move_view(1, 0);
            }
            CtrlLeft => {
                self.move_view(-1, 0);
            }
            F(1) => {
                use crossterm::terminal::Clear;
                std::io::stdout().execute(MoveTo(0, 0)).unwrap();
                std::io::stdout().execute(Clear(ClearType::All)).unwrap();
                println!("{}", include_str!("../resources/help_text.txt"));
            }
            F(5) => {
                self.render();
            }
            Ctrl('c') => {
                self.exit = true;
            }
            Ctrl('a') => {
                // bring the cursor to the top of the viewport
                self.set_cursor(
                    0,
                    self.render_opts.view.location.y() + (self.render_opts.view.height / 2),
                );
            }
            Ctrl('l') => {
                // center the screen on the cursor
                self.render_opts.view.location.1 =
                    self.editor.cursor_pos().y() - (self.render_opts.view.height / 2);
                self.render();
            }
            Char(x) => match self.edit_mode {
                EditMode::Command => {
                    self.process_command_mode(event);
                }
                EditMode::Insert => {
                    self.editor.write(x);
                    self.render_break_line_hint = true;
                    self.render_line_hint = Some(self.editor.cursor_pos().y());
                    self.render();
                }
            },
            Esc => {
                // switch to command mode
                self.edit_mode = EditMode::Command;
                self.render();
            }
            Backspace => {
                if let Some(x) = self.editor.delete() {
                    if x.char != '\n' {
                        self.render_line_hint = Some(self.editor.cursor_pos().y());
                    }
                }
                self.render();
            }
            Enter => {
                self.editor.write('\n');
                self.render();
            }
            Home => {
                self.set_cursor(0, self.editor.cursor_pos().y());
            }
            End => {
                self.set_cursor(self.editor.line_len() as i32, self.editor.cursor_pos().y());
            }
            _ => {}
        }
    }

    /// process command mode inputs
    pub fn process_command_mode(&mut self, event: KeyEvent) {
        use KeyEvent::*;
        match event {
            Char('i') => {
                self.edit_mode = EditMode::Insert;
                self.render();
            }
            Char('j') => self.move_cursor(0, 1),
            Char('k') => self.move_cursor(0, -1),
            Char('h') => self.move_cursor(-1, 0),
            Char('l') => self.move_cursor(1, 0),
            _ => {}
        }
    }

    /// render the screen to crossterm.
    /// if self.render_line_hint is not None, only that line will be rendered
    pub fn render(&mut self) {
        self.update_view_size().unwrap();

        // render a single line if the line hint is not None
        if let Some(line) = self.render_line_hint {
            self.render_line(line);
            return;
        }

        let text = StringRenderer::new().render(&self.editor, self.render_opts);

        let mut stdout = std::io::stdout();
        stdout.execute(MoveTo(0, 0)).unwrap();
        write!(
            &mut stdout,
            "{}[F1 to display help ] {:?}{}",
            text, self.render_opts, self.log
        )
        .unwrap();

        self.update_cursor_pos();
    }

    pub fn clear_render_hints(&mut self) {
        self.render_break_line_hint = false;
        self.render_line_hint = None;
    }

    pub fn update_cursor_pos(&self) {
        if self.render_opts.view.contains(self.editor.cursor_pos()) {
            // place the cursor over the current character
            let x = self.render_opts.view.x();
            let y = self.render_opts.view.y();

            // obtain the position of the cursor relative to the screen
            let real_x = self.editor.cursor_pos().x() - x;
            let real_y = self.editor.cursor_pos().y() - y;

            std::io::stdout()
                .execute(MoveTo(real_x as u16, real_y as u16))
                .unwrap();
        }
    }

    /// render only a single line of the editor
    pub fn render_line(&mut self, line: i32) {
        let ycp = line;
        let y = ycp - self.render_opts.view.location.y();
        if self.render_opts.view.contains(Vector2(0, ycp)) {
            std::io::stdout().execute(MoveTo(0, y as u16)).unwrap();
            let text = StringRenderer {
                line_hint: Some(line),
                break_on_line_end: self.render_break_line_hint,
            }
            .render(&self.editor, self.render_opts);
            print!("{}", text);
            self.update_cursor_pos();
            self.clear_render_hints();
        } else {
            self.clear_render_hints();

            if Vector2(0, ycp) < self.render_opts.view.location {
                self.render();
            }
            // the line is not in view, no need to render any changes
        }
    }

    /// update the view size for the renderer
    pub fn update_view_size(&mut self) -> crossterm::Result<()> {
        let (cols, rows) = terminal::size()?;
        self.render_opts.view.width = cols as i32;
        self.render_opts.view.height = rows as i32 - 1;
        Ok(())
    }
}
