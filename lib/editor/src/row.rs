use std::cmp;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Default)]
pub struct Row {
    string: String,
    len: usize,
}

impl From<&str> for Row {
    fn from(slice: &str) -> Self {
        let mut row = Self {
            string: String::from(slice),
            len: 0,
        };
        row.update_len();
        row
    }
}

impl Row {
    pub fn render(&self, start: usize, end: usize) -> String {
        let end = cmp::min(end, self.string.len());
        let start = cmp::min(start, end);
        let mut result = String::new();
        for string in self.string[..].graphemes(true).skip(start).take(end - start) {
            if string == "\t" {
                result.push_str(" ");
            } else {
                result.push_str(string);
            }
        }
        result
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    fn update_len(&mut self) {
        self.len = self.string[..].graphemes(true).count();
    }
    pub fn insert(&mut self, location: usize, c: char) {
        if location >= self.len() {
            self.string.push(c);
        } else {
            let mut result: String = self.string[..].graphemes(true).take(location).collect();
            let remainder: String = self.string[..].graphemes(true).take(location).collect();
            result.push(c);
            result.push_str(&remainder);
            self.string = result;
        }
        self.update_len();
    }
    pub fn delete(&mut self, location: usize) {
        if location >= self.len() {
            return;
        } else {
            let mut result: String = self.string[..].graphemes(true).take(location).collect();
            let remainder: String = self.string[..].graphemes(true).skip(location + 1).collect();
            result.push_str(&remainder);
            self.string = result;
        }
        self.update_len();
    }
    pub fn append(&mut self, new: &Self) {
        self.string = format!("{}{}", self.string, new.string);
        self.update_len();
    }
    pub fn split(&mut self, location: usize) -> Self {
        let start: String = self.string[..].graphemes(true).take(location).collect();
        let remainder: String = self.string[..].graphemes(true).skip(location).collect();
        self.string = start;
        self.update_len();
        Self::from(&remainder[..])
    }
    pub fn as_bytes(&self) -> &[u8] {
        self.string.as_bytes()
    }
}