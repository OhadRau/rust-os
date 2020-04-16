use alloc::string::String;

pub struct EncryptionParams {
    password: String,
}

#[derive(Clone)]
pub enum MountOptions {
    Encrypted(String),
    Normal
}