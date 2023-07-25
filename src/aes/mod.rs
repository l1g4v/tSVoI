// SPDX-FileCopyrightText: Copyright 2023 tSVoI
// SPDX-License-Identifier: GPL-3.0-only

use aead::{generic_array::GenericArray, AeadCore, Error};
use aes_gcm_siv::{
    aead::{Aead, KeyInit, OsRng},
    Aes256GcmSiv,
    Nonce, // Or `Aes128GcmSiv`
};
use base64::{engine::general_purpose, DecodeError, Engine as _};
use bytes::{BufMut, Bytes, BytesMut};
use general_purpose::STANDARD_NO_PAD as BASE64;

#[derive(Clone)]
pub struct AES {
    key: String,
    cipher: Aes256GcmSiv,
}

impl AES {
    /// This function will create a new AES instance with a random key if no key is provided.
    pub fn new(key: Option<&str>) -> Result<Self, DecodeError> {
        if key.is_some() {
            let key_vec = BASE64.decode(key.unwrap())?;
            return Ok(AES {
                key: key.unwrap().to_string(),
                cipher: Aes256GcmSiv::new(GenericArray::from_slice(key_vec.as_slice())),
            });
        }

        let key = Aes256GcmSiv::generate_key(&mut OsRng);
        Ok(AES {
            key: BASE64.encode(key.as_slice()),
            cipher: Aes256GcmSiv::new(&key),
        })
    }

    pub fn get_key(&self) -> String {
        self.key.clone()
    }

    pub fn encrypt<T: AsRef<[u8]>>(&self, data: T) -> Result<Bytes, Error> {
        let nonce = &Aes256GcmSiv::generate_nonce(&mut OsRng);
        let ciphertext = self.cipher.encrypt(nonce, data.as_ref())?;
        let len = ciphertext.len();
        let mut noncecipher = BytesMut::with_capacity(12 + len);

        noncecipher.put(nonce.as_ref());
        noncecipher.put(ciphertext.as_slice());

        Ok(noncecipher.into())
    }

    pub fn encrypt_b64<T: AsRef<[u8]>>(&self, data: T) -> Result<String, Error> {
        let nonceciphertext = self.encrypt(data)?;
        Ok(BASE64.encode(nonceciphertext))
    }

    pub fn decrypt(&self, bytes: Bytes) -> Result<Bytes, Error> {
        let nonce = &bytes[0..12];
        let ciphertext = &bytes[12..];
        let decrypted = self.cipher.decrypt(&Nonce::from_slice(nonce), ciphertext)?;

        Ok(decrypted.into())
    }

    pub fn decrypt_vec(&self, bytes: Vec<u8>) -> Result<Vec<u8>, Error> {
        let nonce = &bytes[0..12];
        let ciphertext = &bytes[12..];

        self.cipher.decrypt(&Nonce::from_slice(nonce), ciphertext)
    }

    pub fn decrypt_b64(&self, b64_cipher: String) -> Result<Bytes, Error> {
        let b64_decode = Bytes::from(BASE64.decode(b64_cipher).unwrap());

        self.decrypt(b64_decode)
    }
}
