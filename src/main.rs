use rust_ed::application::Application;
use rust_ed::clipboard::OsClipboard;
use rust_ed::editor::Editor;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    let filepath = args.get(1).map(|x| x.to_string_lossy());
    let sample = include_str!("../resources/sample_text.txt").to_string();

    let text = if let Some(fp) = filepath.as_ref() {
        std::fs::read_to_string(fp.as_ref()).expect("could not open the requested file")
    } else {
        sample
    };

    let filepath = filepath.unwrap_or("editor_content.txt".into());

    let mut app = Application::new(Editor::from(text), OsClipboard::new()?, filepath);

    app.run()?;

    Ok(())
}
