use crate::aes128;

use aes::Aes128;
use fat32::traits::BlockDevice;
use shim::io;

///
/// Representation of an EncryptedDevice
/// 
/// An EncryptedDevice is a device that builds upon a previous device, given 
/// as blockDevice.
/// 
/// To use an EncryptedDevice, make another BlockDevice and generate a password 
/// to be used to generate a key and cipher. Create an EncryptedDevice with:
/// 
///     EncryptedDevice::new(password, blockDevice)
/// 
/// This will generate the key and store the block device, while also setting a 
/// flag to use encryption. The encryption flag (e_flag) can be set using the 
/// set_encryption(e_flag) function.
/// 
/// To read and write a sector, just use the read_sector and write_sector 
/// functions as you would for any given block device. If e_flag is set, 
/// then the read or write will automatically use 128bit AES encryption on 
/// the data. If e_flag is not set, then it will read/write without encryption.
/// 
pub struct EncryptedDevice<T> {
    cipher: Aes128,
    blockDevice: T,
    e_flag: bool,
}

impl<T: BlockDevice> EncryptedDevice<T> {
    ///
    /// Create a new EncryptedDevice.
    /// 
    /// Args:
    ///     password: a 16 bytes array of the password used to generate a key
    ///     blockDevice: BlockDevice to add encryption to
    /// 
    pub fn new(password: &[u8; 16], blockDevice: T) -> EncryptedDevice<T> {
        EncryptedDevice {
            cipher: aes128::gen_cipher(password),
            blockDevice: blockDevice,
            e_flag: true,
        }
    }

    ///
    /// Turn on/off encryption
    /// 
    pub fn set_encryption(&mut self, e_flag: bool) {
        self.e_flag = e_flag;
    }
}

impl<T: BlockDevice> BlockDevice for EncryptedDevice<T> {
    fn read_sector(&mut self, n: u64, buf: &mut [u8]) -> io::Result<usize> {
        let bytes_read = self.blockDevice.read_sector(n, buf)?;
        
        if self.e_flag { aes128::decrypt(buf, &self.cipher); }

        Ok(bytes_read)
    }

    fn write_sector(&mut self, n: u64, buf: &[u8]) -> io::Result<usize> {
        let mut buf_cpy = vec![0; buf.len()];
        buf_cpy.clone_from_slice(buf);

        if self.e_flag { aes128::encrypt(&mut buf_cpy, &self.cipher); }

        self.blockDevice.write_sector(n, &buf_cpy)
    }
}