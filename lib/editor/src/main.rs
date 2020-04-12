mod editor;
mod terminal;
mod document;
mod row;
pub use editor::Position;
pub use document::Document;
pub use row::Row;
pub use terminal::Terminal;
use editor::Editor;

fn main() {
    Editor::default().run();
}