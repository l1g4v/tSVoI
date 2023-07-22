// SPDX-FileCopyrightText: Copyright 2023 tSVoI
// SPDX-License-Identifier: GPL-3.0-only 

use aead::{AeadCore, generic_array::GenericArray, Error};
use aes_gcm_siv::{
    aead::{Aead, KeyInit, OsRng},
    Aes256GcmSiv, Nonce // Or `Aes128GcmSiv`
};
use base64::{Engine as _, engine::general_purpose, DecodeError};
use general_purpose::STANDARD_NO_PAD as BASE64;
use bytes::{BytesMut, BufMut, Bytes};

#[derive(Clone)]
pub struct AES{
    key: String,
    cipher : Aes256GcmSiv
}

impl AES{
    /// This function will create a new AES instance with a random key if no key is provided.
    pub fn new(key: Option<&String>) -> Result<Self, DecodeError>{
        if key.is_some(){
            let key_try = BASE64.decode(key.unwrap());
            if key_try.is_err(){
                return Err(key_try.unwrap_err());
            }

            return Ok(AES{
                key: key.unwrap().clone(),
                cipher: Aes256GcmSiv::new(GenericArray::from_slice(key_try.unwrap().as_slice()))
            });
        }

        let key = Aes256GcmSiv::generate_key(&mut OsRng);
        Ok(AES{
            key: BASE64.encode(key.as_slice()),
            cipher: Aes256GcmSiv::new(&key)
        })
    }

    pub fn get_key(&self) -> String{
        self.key.clone()
    }

    pub fn encrypt<T: AsRef<[u8]>>(&self, data: T) -> Result<Bytes, Error>{    
        let nonce = &Aes256GcmSiv::generate_nonce(&mut OsRng);
        let ciphertext = self.cipher.encrypt(nonce, data.as_ref());
        if ciphertext.is_err(){
            return Err(*ciphertext.as_ref().err().unwrap());
        }
        let len = ciphertext.as_ref().unwrap().len();
        let mut noncecipher = BytesMut::with_capacity(12 + len);
        noncecipher.put(nonce.as_ref());
        noncecipher.put(ciphertext.as_ref().unwrap().as_slice());
    
        Ok(noncecipher.into())
    }

    pub fn encrypt_b64<T: AsRef<[u8]>>(&self, data: T) -> Result<String, Error>{    
        let nonceciphertext = self.encrypt(data);
        if nonceciphertext.is_err(){
            return Err(nonceciphertext.unwrap_err());
        }
    
        Ok(BASE64.encode(nonceciphertext.unwrap()))
    }

    pub fn decrypt(&self, bytes: Bytes) -> Result<Bytes, Error>{    
        let nonce = &bytes[0..12];
        let ciphertext = &bytes[12..];
    
        let decrypted = self.cipher.decrypt(&Nonce::from_slice(nonce), ciphertext);
        if decrypted.is_err(){
            return Err(decrypted.unwrap_err());
        }

        Ok(decrypted.unwrap().into())
    }

    pub fn decrypt_b64(&self, b64_cipher: String) -> Result<Bytes, Error>{    
        let b64_decode = Bytes::from(BASE64.decode(b64_cipher).unwrap());

        self.decrypt(b64_decode)
    }

    

}