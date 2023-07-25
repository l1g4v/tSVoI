// SPDX-FileCopyrightText: Copyright 2023 tSVoI
// SPDX-License-Identifier: GPL-3.0-only

use bytes::{BufMut, Bytes, BytesMut};
use log::{debug, error, info, warn};

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::aes::AES;
use crate::audio::playback::AudioPlayback;
use crate::audio::{playback, Audio};
use crate::audio_peer::{self, AudioPeer};
use crate::signaling;

pub struct SignalingClient {
    id: u8,
    username: String,
    stream: TcpStream,
    cipher: Arc<AES>,
    //(username, address_candidate, peer)
    audio_peers: Arc<Mutex<HashMap<u8, (Bytes, AudioPeer)>>>,
}
impl SignalingClient {
    pub fn new(username: String, address: &str, key: &str) -> Self {
        let cipher = Arc::new(AES::new(Some(key)).unwrap());
        let audio_peers = Arc::new(Mutex::new(HashMap::new()));
        let try_stream = TcpStream::connect(address);
        if try_stream.is_err() {
            error!("Err: {:?}", try_stream.err());
            panic!("Failed to connect to server");
        } else {
            debug!("Connected to server");
        }
        let mut stream = try_stream.unwrap();
        //stream.write(&[1]);
        thread::sleep(std::time::Duration::from_millis(10));

        let recv_buffer = &mut [0u8; 1024];
        let try_recv = stream.read(recv_buffer);
        if try_recv.is_err() {
            panic!("Failed to read to buffer");
        }
        let size = try_recv.unwrap();
        let encrypted = recv_buffer[..size].to_vec();
        let try_decrypt = cipher.decrypt(Bytes::from(encrypted));
        if try_decrypt.is_err() {
            panic!("Failed to decrypt id");
        }
        let decrypted = try_decrypt.unwrap();
        let opcode = decrypted[0];
        if opcode != 0 {
            panic!("Invalid opcode");
        }

        let id = decrypted[1];
        debug!("Peer id is {}", id);
        SignalingClient {
            id,
            username,
            stream,
            cipher,
            audio_peers,
        }
    }
    pub fn run(&self, playback_name: String) {
        let mut stream = self.stream.try_clone().unwrap();
        let audio_peers = self.audio_peers.clone();
        //Announce
        let mut unlocked_peers = audio_peers.lock().unwrap();
        for i in 0..self.id {
            let address_candidate = signaling::get_address_ipv6();
            let mut announce_msg = BytesMut::with_capacity(1024);
            announce_msg.put_u8(1);
            announce_msg.put_u8(self.id);
            announce_msg.put_u8(i);
            announce_msg.put_u8(address_candidate.len() as u8);
            announce_msg.put(address_candidate.as_bytes());
            announce_msg.put(self.username.as_bytes());

            let encrypted = self.cipher.encrypt(announce_msg).unwrap();
            stream.write_all(&encrypted).unwrap();

            let audio_peer = AudioPeer::new(address_candidate, self.cipher.get_key());
            unlocked_peers.insert(i, ("tbd".into(), audio_peer));
        }
        drop(unlocked_peers);
        let my_id = self.id;
        let aes_clone = self.cipher.clone();
        let my_username = self.username.clone();
        let playback_name = playback_name.clone();
        let my_id = self.id;
        thread::spawn(move || {
            let recv_buffer = &mut [0u8; 1024];
            let audio_peers = audio_peers.clone();
            let playback_name = playback_name.clone();
            println!("{{ \"notification_code\": 1 }}");
            loop {
                let audio_peers = audio_peers.clone();
                let playback_name = playback_name.clone();
                match stream.read(recv_buffer) {
                    Ok(recv_len) => {
                        if recv_len == 0 {
                            error!("Connection closed");
                            return;
                        }
                        let try_decrypt = aes_clone.decrypt_vec(recv_buffer[..recv_len].to_vec());
                        if try_decrypt.is_err() {
                            error!("Failed to decrypt message: {}", try_decrypt.err().unwrap());
                            continue;
                        }

                        let decrypted = try_decrypt.unwrap();
                        debug!("Received message: {:?}", decrypted);
                        let to_id = decrypted[2];
                        if to_id != my_id {
                            error!("Received message for peer {} {:?}", to_id, decrypted);
                            continue;
                        }

                        let opcode = decrypted[0];
                        let from_id = decrypted[1];
                        match opcode {
                            1 => {
                                let payload = decrypted[3..].to_vec();
                                let ip_len = 1 + payload[0] as usize;
                                let ip_candidate =
                                    String::from_utf8(payload[1..ip_len].to_vec()).unwrap();
                                let username = Bytes::from(payload[ip_len..].to_vec());
                                let my_addr_candidate = signaling::get_address_ipv6();
                                let audio_peer =
                                    AudioPeer::new(my_addr_candidate.clone(), aes_clone.get_key());
                                audio_peers
                                    .lock()
                                    .unwrap()
                                    .insert(from_id, (username, audio_peer));

                                thread::spawn(move || {
                                    let unlocked_peers = audio_peers.lock().unwrap();
                                    let (usr, audio_peer) = unlocked_peers.get(&from_id).unwrap();
                                    audio_peer.connect(ip_candidate, playback_name.clone());
                                    info!("Connected to peer {}", String::from_utf8(usr.to_vec()).unwrap());
                                    drop(unlocked_peers);
                                    loop {
                                        thread::sleep(std::time::Duration::from_millis(100));
                                    }
                                });

                                let mut reply = BytesMut::with_capacity(1024);
                                reply.put_u8(2);
                                reply.put_u8(my_id);
                                reply.put_u8(from_id);
                                reply.put_u8(my_addr_candidate.len() as u8);
                                reply.put(my_addr_candidate.as_bytes());
                                reply.put(my_username.as_bytes());

                                let encrypted = aes_clone.encrypt(reply).unwrap();
                                stream.write_all(&encrypted);
                            }
                            2 => {
                                let payload = decrypted[3..].to_vec();
                                let ip_len = 1 + payload[0] as usize;
                                let ip_candidate =
                                    String::from_utf8(payload[1..ip_len].to_vec()).unwrap();
                                let username = Bytes::from(payload[ip_len..].to_vec());
                                let mut unlocked_peers = audio_peers.lock().unwrap();
                                let (usr, _) = unlocked_peers.get_mut(&from_id).unwrap();
                                *usr = username;
                                drop(unlocked_peers);

                                thread::spawn(move || {
                                    let unlocked_peers = audio_peers.lock().unwrap();
                                    let (usr, audio_peer) = unlocked_peers.get(&from_id).unwrap();
                                    audio_peer.connect(ip_candidate, playback_name.clone());
                                    info!("Connected to peer {}", String::from_utf8(usr.to_vec()).unwrap());
                                    drop(unlocked_peers);
                                    loop {
                                        thread::sleep(std::time::Duration::from_millis(100));
                                    }
                                });
                            }
                            3 => {
                                todo!("Change bitrate or let AudioPeer handle it");
                            }
                            _ => {
                                error!("Unknown opcode {}", opcode);
                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to read from stream: {}", e);
                        return;
                    }
                }
            }
        });
    }
    pub fn send_opus(&self, opus_packet: Bytes) {
        let trylock = self.audio_peers.try_lock();
        if trylock.is_err() {
            error!("Failed to lock audio peers, err:{}", trylock.err().unwrap());
            return;
        }
        let peers = trylock.unwrap();
        for (_, peer) in peers.values() {
            peer.send(opus_packet.clone());
        }
    }

    pub fn get_peers(&self) -> Vec<(u8, Bytes)> {
        let peers = self.audio_peers.lock().unwrap();
        let mut result: Vec<(u8, Bytes)> = Vec::new();
        peers.iter().for_each(|(id, (username, _))| {
            result.push((*id, username.clone()));
        });
        result
    }

    pub fn change_peer_volume(&self, peer_id: u8, volume: u8) {
        let peers = self.audio_peers.lock().unwrap();
        let peer = peers.get(&peer_id);
        if peer.is_none() {
            error!("Peer {} not found", peer_id);
            return;
        }
        let (_, peer) = peer.unwrap();
        peer.change_volume(volume);
    }
}