use core::fmt;

use alloc::string::String;

use crate::traits;

/// A date as represented in FAT32 on-disk structures.
#[repr(C, packed)]
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct Date(u16);

/// Time as represented in FAT32 on-disk structures.
#[repr(C, packed)]
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct Time(u16);

/// File attributes as represented in FAT32 on-disk structures.
#[repr(C, packed)]
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct Attributes(u8);

impl Attributes {
    pub fn is_lfn(&self) -> bool {
        (self.0 & 0x0F) == 0x0F
    }

    pub fn is_dir(&self) -> bool {
        (self.0 & 0x10) == 0x10
    }

    pub fn lfn(&self) -> Self {
        Attributes(self.0 | 0x0F)
    }

    pub fn dir(&self) -> Self {
        Attributes(self.0 | 0x10)
    }

    pub fn default_dir() -> Self {
        Attributes(0x10)
    }
}

/// A structure containing a date and time.
#[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]
pub struct Timestamp {
    pub time: Time,
    pub date: Date,
}

/// Metadata for a directory entry.
#[derive(Default, Debug, Clone)]
pub struct Metadata {
    pub name: String,
    pub created: Timestamp,
    pub accessed: Timestamp,
    pub modified: Timestamp,
    pub attributes: Attributes,
    pub size: usize,
}

// Implement `traits::Timestamp` for `Timestamp`.
impl traits::Timestamp for Timestamp {
    /// The calendar year.
    ///
    /// The year is not offset. 2009 is 2009.
    fn year(&self) -> usize {
      ((self.date.0 >> 9) & 0x7F) as usize + 1980
    }

    /// The calendar month, starting at 1 for January. Always in range [1, 12].
    ///
    /// January is 1, Feburary is 2, ..., December is 12.
    fn month(&self) -> u8 {
      ((self.date.0 >> 5) & 0xF) as u8
    }

    /// The calendar day, starting at 1. Always in range [1, 31].
    fn day(&self) -> u8 {
      (self.date.0 & 0x1F) as u8
    }

    /// The 24-hour hour. Always in range [0, 24).
    fn hour(&self) -> u8 {
       ((self.time.0 >> 11) & 0x1F) as u8
    }

    /// The minute. Always in range [0, 60).
    fn minute(&self) -> u8 {
        ((self.time.0 >> 5) & 0x3F) as u8
    }

    /// The second. Always in range [0, 60).
    fn second(&self) -> u8 {
        (2 * (self.time.0 & 0x1F)) as u8
    }
}

// Implement `traits::Metadata` for `Metadata`.
impl traits::Metadata for Metadata {
    /// Type corresponding to a point in time.
    type Timestamp = Timestamp;

    /// Whether the associated entry is read only.
    fn read_only(&self) -> bool {
       (self.attributes.0 & 0x01) == 0x01
    }

    /// Whether the entry should be "hidden" from directory traversals.
    fn hidden(&self) -> bool {
      (self.attributes.0 & 0x02) == 0x02
    }

    /// The timestamp when the entry was created.
    fn created(&self) -> Self::Timestamp {
      self.created
    }

    /// The timestamp for the entry's last access.
    fn accessed(&self) -> Self::Timestamp {
      self.accessed
    }

    /// The timestamp for the entry's last modification.
    fn modified(&self) -> Self::Timestamp {
      self.modified
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use traits::Timestamp;
        write!(f, "{:02}/{:02}/{:04} {:02}:{:02}:{:02}",
               self.month(), self.day(), self.year(),
               self.hour(), self.minute(), self.second())
    }
}

// Implement `fmt::Display` (to your liking) for `Metadata`.
impl fmt::Display for Metadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use traits::Metadata;
        
        fn flag(f: bool, c: char) -> char {
            if f {
                c
            } else {
                '-'
            }
        }
        
        let dir = flag(self.attributes.is_dir(), 'd');
        let writeable = flag(!self.read_only(), 'w');
        let hidden = flag(self.hidden(), 'h');

        let name = if self.attributes.is_dir() {
            format!("{}/", self.name)
        } else {
            String::from(&self.name)
        };

        // dwh created modified size name
        write!(f, "{}{}{}\t{}\t{}\t{}\t{}",
               dir, writeable, hidden,
               self.created, self.modified,
               self.size, name)
    }
}
