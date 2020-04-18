use alloc::string::String;

pub struct EncryptionParams {
    password: String,
}

#[derive(Clone, Debug)]
pub enum MountOptions {
    Encrypted(Option<String>),
    Normal
}