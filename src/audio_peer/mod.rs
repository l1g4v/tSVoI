// SPDX-FileCopyrightText: Copyright 2023 tSVoI
// SPDX-License-Identifier: GPL-3.0-only 

use std::{sync::{Arc, Mutex, atomic::{AtomicBool, Ordering, AtomicU64, AtomicI8}}, thread};
use flume::{Sender, Receiver};
use bytes::{BytesMut, BufMut, Bytes};

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use crate::audio::{playback::AudioPlayback, Audio};

use crate::aes::AES;

pub struct AudioPeer {
    ready: Arc<AtomicBool>,
    packet_count: Arc<AtomicU64>,
    volume: Arc<AtomicI8>,
    udpsocket: Arc<Mutex<std::net::UdpSocket>>,
    aes: AES
}
impl AudioPeer {
    /// Creates a new AudioPeer
    /// # Arguments
    /// * `bind` - The address to bind to
    pub fn new(bind: String, key: String) -> AudioPeer {
        AudioPeer {
            packet_count: Arc::new(AtomicU64::new(0)),
            ready: Arc::new(AtomicBool::new(false)),
            volume: Arc::new(AtomicI8::new(100)),
            //tk_socketqueue: Arc::new(Mutex::new(BinaryHeap::new())),
            udpsocket: Arc::new(Mutex::new(
                std::net::UdpSocket::bind(bind).expect("couldn't bind to address"),
            )),
            aes: AES::new(Some(&key)).unwrap()
        }
    }

    // TODO: change playback_config to an AudioPlayback object
    /// Connects to a peer
    /// # Arguments
    /// * `addr` - The address to connect to
    /// * `playback_name` - The name of the playback device
    pub fn connect(&self, addr: String, playback_name: String) {
        self.udpsocket.lock().unwrap().connect(&addr).expect("couldn't connect to address");
        let udp_socket = self.udpsocket.lock().unwrap().try_clone().unwrap();
        let aes = self.aes.clone();
        let volume = self.volume.clone();
        let playback_config = AudioPlayback::create_config(playback_name, 2, 48_000);
        let audio_playback = AudioPlayback::new(playback_config);
        let ready = self.ready.clone();

        //Avoids a weird bug where the cpu usage grows when one of the two peers never receives a packet
        udp_socket.send(&[1]).unwrap();
        thread::spawn(move ||{
            audio_playback.start();
            let mut recv_buffer = BytesMut::zeroed(1024);
            let mut audio_buffer: BinaryHeap<Reverse<(u64, Bytes)>> = BinaryHeap::new();
            let playback_tx = audio_playback.get_playback_tx();
            loop{
                match udp_socket.recv(recv_buffer.as_mut()) {
                    Ok(n) => {
                        if n == 1 && !ready.load(Ordering::Relaxed){
                            debug!("Ready");
                            ready.store(true, Ordering::Relaxed);
                            udp_socket.send(&[1]).unwrap();
                            continue;
                        }
                        if n < 8 { continue; }      

                        //Decrypt packet
                        let decrypted = aes.decrypt(bytes::Bytes::copy_from_slice(&recv_buffer[..n])).unwrap();
                        let dec_len = decrypted.len();

                        //Get packet count
                        let mut packet_count_bytes = [0u8; 8];
                        packet_count_bytes.copy_from_slice(&decrypted[dec_len - 8..]);
                        let recv_packet_count: u64 = u64::from_be_bytes(packet_count_bytes);
    
                        //Push voice packet to buffer
                        let mut opus = BytesMut::with_capacity(dec_len - 7);
                        opus.put(&decrypted[..dec_len - 8]);
                        opus.put_u8(volume.load(Ordering::Relaxed) as u8);

                        let voice = (recv_packet_count, opus.freeze());
                        audio_buffer.push(Reverse(voice));
                    }
                    Err(e) => {
                        error!("Error: {}", e);
                    }
                }
                // "jitter buffer¿¿¿¿¿ (Ñ)"
                if audio_buffer.len() > 1 {
                    while !audio_buffer.is_empty() {
                        let payload = audio_buffer.pop().unwrap().0.1;
                        playback_tx.send(payload).unwrap();
                    }
                }
            }
        });
    }

    /// Sends a voice packet through the socket.
    /// The packet is serialized as follows:
    /// <opus packet variable size><packet number 8 bytes>
    /// # Arguments
    /// * `data` - An opus packet
    /// # Returns
    /// * `usize` - The number of bytes sent
    /// # Errors
    /// * `std::io::Error` - If the peer is not ready
    pub fn send(&self, data: Bytes) -> Result<usize, std::io::Error> {
        if !self.is_ready(){
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Peer not ready"));
        }
        let packet_count = self.packet_count.fetch_add(1, Ordering::Relaxed);
        let mut payload = BytesMut::with_capacity(data.len() + 8);
        payload.put(data);
        payload.put_u64(packet_count);
        let encrypted = self.aes.encrypt(payload.freeze()).unwrap();

        self.udpsocket
            .lock()
            .unwrap()
            .send(&encrypted)
    }

    pub fn change_volume(&self, volume: u8) {
        self.volume.store(volume as i8, Ordering::Relaxed);
    }
    
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Relaxed)
    }
}