use aes::block_cipher_trait::generic_array::GenericArray;
use aes::block_cipher_trait::BlockCipher;
use aes::Aes128;

/// Size of an AES block in bytes
const BLOCK_SIZE: usize = 16;

///
/// Generate and return a cipher block. The key consists of 16 bytes, for a 
/// total of 128 bits.
/// 
/// The caller must hold onto this to encrypt/decrypt
/// 
pub fn gen_cipher(password: &[u8; 16]) -> Aes128 {
    let key = GenericArray::from_slice(password);

    Aes128::new(&key)
}

///
/// Encrypt the input buffer in-place.
/// 
/// buf.len() must be a multiple of 16
/// 
pub fn encrypt<'a, 'b>(buf: &'a mut [u8], cipher: &'b Aes128) -> Result<&'a mut [u8], ()> {
    if buf.len() % BLOCK_SIZE != 0 { return Err(()) }

    // amount of blocks to store data
    let block_num: usize = buf.len() / BLOCK_SIZE;

    // for each block, replace the bytes with the encrypted bytes in-place
    for i in 0..block_num {
        let block_start = i * BLOCK_SIZE;
        
        // slice bytes for slice
        let mut block_slice = [0u8; BLOCK_SIZE];
        block_slice.clone_from_slice(&buf[block_start..block_start + BLOCK_SIZE]);

        // create block from block slice
        let mut block_orig: &mut GenericArray<u8, <Aes128 as BlockCipher>::BlockSize> = GenericArray::from_mut_slice(&mut block_slice);

        // encrypt the block
        cipher.encrypt_block(&mut block_orig);

        // replace bytes in buffer in-place
        buf[block_start..block_start + BLOCK_SIZE].copy_from_slice(block_orig);
    }

    Ok(buf)
}

///
/// Decrypt the input encrypted buffer in-place.
/// 
/// bef.len() must be a multiple of 16
/// 
pub fn decrypt<'a, 'b>(buf: &'a mut [u8], cipher: &'b Aes128) -> Result<&'a mut [u8], ()> {
    if buf.len() % BLOCK_SIZE != 0 { return Err(()) }

    // amount of blocks to store data
    let block_num: usize = buf.len() / BLOCK_SIZE;

    // for each block, replace the bytes with the encrypted bytes in-place
    for i in 0..block_num {
        let block_start = i * BLOCK_SIZE;
        
        // slice bytes for slice
        let mut block_slice = [0u8; BLOCK_SIZE];
        block_slice.clone_from_slice(&buf[block_start..block_start + BLOCK_SIZE]);

        // create block from block slice
        let mut block_orig: &mut GenericArray<u8, <Aes128 as BlockCipher>::BlockSize> = GenericArray::from_mut_slice(&mut block_slice);

        // encrypt the block
        cipher.decrypt_block(&mut block_orig);

        // replace bytes in buffer in-place
        buf[block_start..block_start + BLOCK_SIZE].copy_from_slice(block_orig);
    }

    Ok(buf)
}
