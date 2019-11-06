use crate::clipboard::Clipboard;
use crate::editor::Editor;
use crate::renderer::{RenderOpts, Renderer, StringRenderer};
use crate::vector::Vector2;

use crossterm::{
    cursor::MoveTo,
    input::{EnableMouseCapture, InputEvent, KeyEvent, MouseEvent, SyncReader},
    screen::{self},
    terminal::{self, ClearType},
    ExecutableCommand,
};

use std::io::{stdout, Write};

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
    // path to save the file to
    pub filepath: String,
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

    cursor_hidden: bool,
}

impl<T> Application<T>
where
    T: Clipboard,
{
    pub fn new(editor: Editor, clipboard: T, filepath: impl Into<String>) -> Application<T> {
        Application {
            editor,
            clipboard,
            render_opts: RenderOpts::default(),
            exit: false,
            log: String::new(),

            render_line_hint: None,
            render_break_line_hint: false,
            edit_mode: EditMode::Insert,

            cursor_hidden: false,
            filepath: filepath.into(),
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
                ($x + x2.round() as i32, $y + y2.round() as i32)
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
        self.render_opts.view.location = self
            .render_opts
            .view
            .location
            .add(Vector2(x as f64, y as f64));
        self.render();
    }

    pub fn process_key_event(&mut self, event: KeyEvent) {
        use KeyEvent::*;

        // global key bindings, apply regardless of edit mode
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
            Ctrl('d') => {
                // delete the line the cursor is on
                self.delete_line();
                self.render();
            }
            Ctrl('a') => {
                // bring the cursor to the top of the viewport
                self.set_cursor(
                    0,
                    self.render_opts.view.location.y() as i32 + (self.render_opts.view.height / 2),
                );
            }
            Ctrl('v') => {
                let text = self.clipboard.paste().unwrap().replace("\r", "");
                for c in text.chars() {
                    self.editor.write(c);
                }
                self.render();
            }
            Ctrl('l') => {
                // center the screen on the cursor
                self.render_opts.view.location.1 =
                    (self.editor.cursor_pos().y() - (self.render_opts.view.height / 2)) as f64;
                self.render();
            }
            Ctrl('s') => {
                self.save_to_file();
                self.log = format!("saved to {}", self.filepath);
                self.render();
            }
            Home => {
                self.go_to_line_home();
                self.update_cursor_pos();
            }
            End => {
                self.go_to_line_end();
                self.update_cursor_pos();
            }
            _ => match self.edit_mode {
                EditMode::Insert => self.process_insert_mode(event),
                EditMode::Command => self.process_command_mode(event),
            },
        }
    }

    /// process keys for insert mode
    pub fn process_insert_mode(&mut self, event: KeyEvent) {
        use KeyEvent::*;
        match event {
            Char(x) => {
                self.log = format!("{}{}{}", "[", x, "]");
                self.editor.write(x);
                self.render_break_line_hint = true;
                self.render_line_hint = Some(self.editor.cursor_pos().y());
                self.render();
            }
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
            _ => {}
        }
    }

    pub fn delete_line(&mut self) {
        let pos = self.editor.cursor_pos();
        // move cursor to the end of the line
        // delete characters until the beginning of the line has been reached
        self.go_to_line_end();
        while let Some(c) = self.editor.delete() {
            if c.char == '\n' {
                break;
            }
        }

        let delete_beginning = if pos.y() == 0 { true } else { false };

        // move the cursor to the beginning of the line and move down one
        self.go_to_line_home();
        self.editor.move_cursor((0, 1));
        if delete_beginning {
            self.editor.delete();
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
            Char('o') => {
                self.go_to_line_end();
                self.editor.write('\n');
                self.edit_mode = EditMode::Insert;
                self.render();
            }
            Char('O') => {
                self.move_cursor(0, -1);
                self.go_to_line_end();
                self.editor.write('\n');
                self.edit_mode = EditMode::Insert;
                self.render();
            }

            // scroll screen faster
            Char('J') => self.move_view(0, 5),
            Char('K') => self.move_view(0, -5),
            Char('H') => self.move_view(-5, 0),
            Char('L') => self.move_view(5, 0),

            Char('j') => self.move_cursor(0, 1),
            Char('k') => self.move_cursor(0, -1),
            Char('h') => self.move_cursor(-1, 0),
            Char('l') => self.move_cursor(1, 0),
            Char('w') => self.next_word(true),
            Char('b') => self.next_word(false),
            Char('$') => {
                self.go_to_line_end();
                self.update_cursor_pos();
            }
            Char('0') => {
                self.go_to_line_home();
                self.update_cursor_pos();
            }
            Char('_') => {
                self.render_opts.scale += 0.1;

                // shift the renderer window to keep it's position
                let l = self.render_opts.view.location;
                let dx = l.x() as f64 * 0.9;
                let dy = l.y() as f64 * 0.9;

                self.render_opts.view.location = Vector2(dx, dy);
                self.log = format!(
                    "set scale to {}: position: {}:{}",
                    self.render_opts.scale, dx, dy
                );

                self.render();
            }
            Char('+') => {
                self.render_opts.scale -= 0.1;
                self.log = format!("set scale to {}: ", self.render_opts.scale);
                if self.render_opts.scale <= 0. {
                    self.render_opts.scale = 0.;
                } else {
                    // shift the renderer window to keep it's position
                    let l = self.render_opts.view.location;
                    let dx = l.x() as f64 * 1.1;
                    let dy = l.y() as f64 * 1.1;

                    self.render_opts.view.location = Vector2(dx, dy);
                    self.log = format!(
                        "set scale to {}: position: {}:{}",
                        self.render_opts.scale, dx, dy
                    );
                }
                self.render();
            }
            Char('=') => {
                self.render_opts.scale = 1.;
                self.log = "reset render scale to 1".to_string();
                self.render();
            }
            _ => {}
        }
    }

    /// move the cursor to the next word
    pub fn next_word(&mut self, forward: bool) {
        let pos = self.editor.next_word(self.editor.cursor_pos(), forward);
        self.set_cursor(pos.x(), pos.y());
    }

    /// save the editor contents to a file
    pub fn save_to_file(&self) {
        std::fs::write(&self.filepath, self.editor.to_string()).unwrap();
    }

    pub fn render_status_bar(&mut self) {
        stdout()
            .execute(MoveTo(0, (self.render_opts.view.height + 1) as u16))
            .unwrap();

        use EditMode::*;

        let l = self.render_opts.view.location;
        let r = self.render_opts.view;
        let mode = match self.edit_mode {
            Insert => "insert",
            Command => "command",
        };

        use std::cmp::max;

        let text = format!(
            "help[F1] {}:{}:{}:{}/{} // [{}] [{} mode]",
            l.x(),
            l.y(),
            r.width,
            r.height,
            self.render_opts.scale,
            self.log,
            mode
        );
        let padding: String = std::iter::repeat(" ")
            .take(max(r.width as usize - text.len(), 0))
            .collect();
        print!("{}{}", text, padding);
    }

    /// move to the beginning of the line
    pub fn go_to_line_home(&mut self) {
        self.editor.set_cursor((0, self.editor.cursor_pos().y()));
    }

    /// move cursor to the end of the line
    pub fn go_to_line_end(&mut self) {
        self.editor
            .set_cursor((self.editor.line_len() as i32, self.editor.cursor_pos().y()));
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
        write!(&mut stdout, "{}", text).unwrap();

        self.render_status_bar();
        self.update_cursor_pos();
    }

    pub fn clear_render_hints(&mut self) {
        self.render_break_line_hint = false;
        self.render_line_hint = None;
    }

    pub fn update_cursor_pos(&mut self) {
        if self
            .render_opts
            .view
            .contains(Vector2::from(self.editor.cursor_pos()))
        {
            // place the cursor over the current character
            let x = self.render_opts.view.x().round();
            let y = self.render_opts.view.y().round();

            // obtain the position of the cursor relative to the screen
            let real_x = self.editor.cursor_pos().x() - x as i32;
            let real_y = self.editor.cursor_pos().y() - y as i32;

            stdout()
                .execute(MoveTo(real_x as u16, real_y as u16))
                .unwrap();

            if self.cursor_hidden {
                stdout().execute(crossterm::cursor::Show).unwrap();
                self.cursor_hidden = false;
            }
        } else {
            stdout().execute(crossterm::cursor::MoveTo(0, 0)).unwrap();
            if !self.cursor_hidden {
                stdout().execute(crossterm::cursor::Hide).unwrap();
                self.cursor_hidden = true;
            }
        }
    }

    /// render only a single line of the editor
    pub fn render_line(&mut self, line: i32) {
        let ycp = line;
        let y = ycp as f64 - self.render_opts.view.location.y().round();
        if self
            .render_opts
            .view
            .contains(Vector2::from(Vector2(0, ycp)))
        {
            std::io::stdout().execute(MoveTo(0, y as u16)).unwrap();
            let text = StringRenderer {
                line_hint: Some(line),
                break_on_line_end: self.render_break_line_hint,
            }
            .render(&self.editor, self.render_opts);
            print!("{}", text);
            self.render_status_bar();
            self.update_cursor_pos();
            self.clear_render_hints();
        } else {
            self.clear_render_hints();

            // only render changes if the current view could be affected by the edits.
            // this should be changed to apply only when a new line has been inserted.
            if Vector2(0, ycp) < Vector2::from(self.render_opts.view.location) {
                self.render();
            }
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
