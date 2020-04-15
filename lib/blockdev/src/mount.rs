use alloc::string::String;

pub struct EncryptionParams {
    password: String,
}

pub enum MountOptions {
    Encrypted(String),
    Normal
}