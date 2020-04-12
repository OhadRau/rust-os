use crate::Row;
use std::fs;
use crate::Position;
use std::io::{Error, Write};

#[derive(Default)]
pub struct Document {
    rows: Vec<Row>,
    pub file_name: Option<String>,
}

impl Document {
    pub fn open(filename: &str) -> Result<Self, std::io::Error> {
        let contents = fs::read_to_string(filename)?;
        let mut rows = Vec::new();
        for value in contents.lines() {
            rows.push(Row::from(value));
        }
        Ok(Self{
            rows,
            file_name: Some(filename.to_string()),
        })
    }
    pub fn row(&self, index: usize) -> Option<&Row> {
        self.rows.get(index)
    }
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
    pub fn len(&self) -> usize {
        self.rows.len()
    }
    fn insert_newline(&mut self, location: &Position) {
        if location.y > self.len() {
            return;
        }
        if location.y == self.len() {
            self.rows.push(Row::default());
            return;
        }
        let new_row = self.rows.get_mut(location.y).unwrap().split(location.x);
        self.rows.insert(location.y + 1, new_row);

    }
    pub fn insert(&mut self, location: &Position, c: char) {
        if c == '\n' {
            self.insert_newline(location);
            return;
        }
        if location.y == self.len() {
            let mut row = Row::default();
            row.insert(0, c);
            self.rows.push(row);
        } else if location.y < self.len() {
            let row = self.rows.get_mut(location.y).unwrap();
            row.insert(location.x, c);
        }
    }
    pub fn delete(&mut self, location: &Position) {
        let len = self.len();
        if location.y >= len {
            return;
        }
        if location.x == self.rows.get_mut(location.y).unwrap().len() && location.y < len - 1 {
            let next_row = self.rows.remove(location.y + 1);
            let row = self.rows.get_mut(location.y).unwrap();
            row.append(&next_row);
        } else {
            let row = self.rows.get_mut(location.y).unwrap();
            row.delete(location.x);
        }
    }
    pub fn save(&self) -> Result<(), Error> {
        if let Some(file_name) = &self.file_name {
            let mut file = fs::File::create(file_name)?;
            for row in &self.rows {
                file.write_all(row.as_bytes())?;
                file.write_all(b"\n");
            }
        }
        Ok(())
    }

}