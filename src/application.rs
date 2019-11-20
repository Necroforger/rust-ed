use crate::clipboard::Clipboard;
use crate::editor::{Editor, Position};
use crate::renderer::{RenderOpts, Renderer, StringRenderer};
use crate::vector::Vector2;

use crossterm::{
    cursor::MoveTo,
    input::{EnableMouseCapture, InputEvent, KeyEvent, MouseEvent, SyncReader},
    screen::{self},
    terminal::{self, ClearType},
    ExecutableCommand,
};

use crossterm::style::Colorize;
use std::fmt::{Error, Formatter};
use std::io::{stdout, Write};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Action {
    SaveFileAs,

    // if the provided boolean is true, search backwards through the text
    Search(bool),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum EditMode {
    Command,
    Insert,

    // prompt user input. store the edit mode to return to when done, and an optional action to
    // execute
    Prompt(Box<EditMode>, Option<Action>),
}

impl std::fmt::Display for EditMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        use EditMode::*;
        write!(
            f,
            "{}",
            match self {
                Command => "command",
                Insert => "insert",
                Prompt(_, _) => "prompt",
            }
        )
    }
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

    // the value of the last search text
    last_search: String,

    // buffer to store the results of a prompt in
    prompt_buffer: Editor,

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

            prompt_buffer: Editor::new(),
            edit_mode: EditMode::Command,

            cursor_hidden: false,
            filepath: filepath.into(),

            last_search: String::new(),
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

    pub fn process_prompt_mode(
        &mut self,
        event: KeyEvent,
        action: Option<Action>,
        edit_mode: EditMode,
    ) {
        use KeyEvent::*;

        macro_rules! update {
            () => {
                self.render_status_bar();
                self.update_cursor_pos();
            };
        }

        macro_rules! reset {
            () => {
                self.prompt_buffer = Editor::new();
                self.edit_mode = edit_mode;
            };
        }

        match event {
            Char(x) => {
                self.prompt_buffer.write(x);
                update!();
            }
            Backspace => {
                self.prompt_buffer.delete();
                update!();
            }
            // when escape is pressed, cancel
            Esc => {
                reset!();
                update!();
            }
            Left => {
                self.prompt_buffer.move_cursor((-1, 0));
                update!();
            }
            Right => {
                self.prompt_buffer.move_cursor((1, 0));
                update!();
            }
            Up => {
                if let Some(a) = action {
                    match a {
                        Action::Search(_) => {
                            self.prompt_buffer = Editor::from(self.last_search.clone());
                            self.prompt_buffer.move_cursor_to(Position::LineEnd);
                            self.render();
                        }
                        _ => {}
                    }
                }
            }
            Enter => {
                match action {
                    Some(action) => match action {
                        Action::SaveFileAs => {
                            let text = self.prompt_buffer.to_string();
                            self.filepath = text.clone();
                            self.log = format!("saved to file: {}", text);
                            self.save_to_file(text);
                        }
                        Action::Search(reverse) => {
                            let text = self.prompt_buffer.to_string();
                            self.search_next(text, reverse);
                        }
                    },
                    None => {}
                }
                // when done, clear the prompt
                reset!();
                update!();
            }
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

        if self.editor.selecting {
            self.render_line_hint = Some(self.editor.cursor_pos().y());
            self.render();

            self.render_line_hint = Some(self.editor.cursor_pos().y() - y);
            self.render();
        }
    }

    pub fn move_cursor(&mut self, x: i32, y: i32) {
        self.editor.move_cursor((x, y));
        self.update_cursor_pos();

        if self.editor.selecting {
            self.render_line_hint = Some(self.editor.cursor_pos().y());
            self.render();

            self.render_line_hint = Some(self.editor.cursor_pos().y() - y);
            self.render();
        }
    }

    pub fn move_view(&mut self, x: i32, y: i32) {
        self.render_opts.view.location = self
            .render_opts
            .view
            .location
            .add(Vector2(x as f64, y as f64));
        self.render();
    }

    pub fn center_renderer(&mut self, loc: impl Into<Vector2<i32>>) {
        let loc = loc.into();
        self.render_opts.view.location.1 = (loc.y() - (self.render_opts.view.height / 2)) as f64;
    }

    pub fn process_key_event(&mut self, event: KeyEvent) {
        use KeyEvent::*;

        // global key bindings, apply regardless of edit mode
        match event {
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
                self.center_renderer(self.editor.cursor_pos());
                self.render();
            }
            Ctrl('s') => {
                self.save_to_file(&self.filepath);
                self.log = format!("saved to {}", self.filepath);
                self.render();
            }
            Ctrl('x') => {
                // save file as
                self.edit_mode =
                    EditMode::Prompt(Box::new(self.edit_mode.clone()), Some(Action::SaveFileAs));
                self.log = "save as: ".into();

                // pre-fill the prompt with the current file name
                self.prompt_buffer = Editor::from(&self.filepath);
                self.prompt_buffer.move_cursor_to(Position::LineEnd);
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
            _ => match self.edit_mode.clone() {
                EditMode::Insert => self.process_insert_mode(event),
                EditMode::Command => self.process_command_mode(event),
                EditMode::Prompt(mode, action) => {
                    self.process_prompt_mode(event, action.clone(), *mode.clone());
                }
            },
        }
    }

    /// process keys for insert mode
    pub fn process_insert_mode(&mut self, event: KeyEvent) {
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

            Char('v') => self.editor.begin_select(),
            Char('d') => {
                self.editor.delete();
                self.render();
            }
            Char('c') => {
                if let Some(x) = self.editor.copy() {
                    let text: String = x.iter().map(|x| x.char).collect();
                    match self.clipboard.copy(text) {
                        Ok(_) => self.log = "copied to clipboard".to_string(),
                        Err(e) => self.log = format!("error copying: {}", e),
                    }
                }
                self.render();
            }
            Esc => {
                self.editor.clear_selection();
                self.render();
            }

            Char('/') => {
                self.log = "search: ".into();
                self.edit_mode = EditMode::Prompt(
                    Box::new(self.edit_mode.clone()),
                    Some(Action::Search(false)),
                );
                self.render();
            }

            Char('?') => {
                self.log = "search: ".into();
                self.edit_mode =
                    EditMode::Prompt(Box::new(self.edit_mode.clone()), Some(Action::Search(true)));
                self.render();
            }

            // repeat last search
            Char('n') => {
                self.log = "repeating last search".into();
                self.search_next(self.last_search.clone(), false);
                self.render();
            }

            // repeat last search backwards
            Char('N') => {
                self.log = "repeating last search".into();
                self.search_next(self.last_search.clone(), true);
                self.render();
            }

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
                let original_scale = self.render_opts.scale;
                self.render_opts.scale += 0.1;

                // find the new center point
                let c1 = self.render_opts.view.center_point();
                let c2 = transform_view_coordinates(c1, original_scale, self.render_opts.scale);
                let origin = c2.sub(self.render_opts.view.center());

                self.render_opts.view.location = origin;

                self.render();
            }
            Char('+') => {
                let original_scale = self.render_opts.scale;
                self.render_opts.scale -= 0.1;

                if self.render_opts.scale <= 0. {
                    self.render_opts.scale = 0.;
                } else {
                    // shift the renderer window to keep it's position
                    let c1 = self.render_opts.view.center_point();
                    let c2 = transform_view_coordinates(c1, original_scale, self.render_opts.scale);
                    let origin = c2.sub(self.render_opts.view.center());
                    self.render_opts.view.location = origin;
                }
                self.render();
            }
            Char('=') => {
                let original_scale = self.render_opts.scale;

                self.render_opts.scale = 1.;

                let c1 = self.render_opts.view.center_point();
                let c2 = transform_view_coordinates(c1, original_scale, self.render_opts.scale);
                let origin = c2.sub(self.render_opts.view.center());

                self.render_opts.view.location = origin;

                self.log = "reset render scale to 1".to_string();
                self.render();
            }
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
            _ => {}
        }
    }

    /// search for the next occurrence of a string
    pub fn search_next(&mut self, text: impl Into<String>, reverse: bool) {
        let text = text.into();
        self.last_search = text.clone();

        let start = self.editor.cursor_pos();

        if let Some(x) = self.editor.search(&text, start, reverse) {
            self.set_cursor(x.x(), x.y());
            if !self.render_opts.view.contains(Vector2::from(x)) {
                self.center_renderer(x);
                self.render();
            }
        }
    }

    /// move the cursor to the next word
    pub fn next_word(&mut self, forward: bool) {
        self.editor.move_cursor_to(if forward {
            Position::NextWord
        } else {
            Position::PreviousWord
        });
        self.update_cursor_pos();
    }

    /// save the editor contents to a file
    pub fn save_to_file(&self, filename: impl AsRef<std::path::Path>) {
        std::fs::write(filename, self.editor.to_string()).unwrap();
    }

    pub fn render_status_bar(&mut self) {
        stdout()
            .execute(MoveTo(0, (self.render_opts.view.height + 1) as u16))
            .unwrap();

        let l = self.render_opts.view.location;
        let r = self.render_opts.view;

        let prompt_text: Vec<String> = format!("{} ", self.prompt_buffer)
            .chars()
            .enumerate()
            .map(|(i, x)| {
                if i as i32 == self.prompt_buffer.cursor_pos().x()
                    && (if let EditMode::Prompt(_, _) = self.edit_mode {
                        true
                    } else {
                        false
                    })
                {
                    crossterm::style::style(x.to_string()).on_red().to_string()
                } else {
                    x.to_string()
                }
            })
            .collect();

        let text = format!(
            "help[F1] {x:.2}:{y:.2}:{w}:{h}/{scale:.2}//[{mode}][{log}]:",
            x = l.x(),
            y = l.y(),
            w = r.width,
            h = r.height,
            scale = self.render_opts.scale,
            log = self.log,
            mode = self.edit_mode.to_string().to_uppercase(),
        );

        let prompt_offset = text.len();
        for i in 0..self.render_opts.view.width {
            let c = if (i as usize) < prompt_offset {
                text.chars().nth(i as usize).map(|x| x.to_string())
            } else if (i as usize) < prompt_offset + prompt_text.len() {
                prompt_text
                    .get(i as usize - prompt_offset)
                    .map(|x| x.clone())
            } else {
                Some(" ".to_string())
            };

            if let Some(c) = c {
                print!("{}", c);
            }
        }
    }

    /// move to the beginning of the line
    pub fn go_to_line_home(&mut self) {
        self.editor.move_cursor_to(Position::LineBeginning);
    }

    /// move cursor to the end of the line
    pub fn go_to_line_end(&mut self) {
        self.editor.move_cursor_to(Position::LineEnd);
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

/// find the new location of an editor coordinate when applied to a different scale
pub fn transform_view_coordinates(p: Vector2<f64>, scale: f64, scale2: f64) -> Vector2<f64> {
    let real = p.scalar(scale);
    real.scalar_div(scale2)
}
