// SPDX-FileCopyrightText: Copyright 2023 tSVoI
// SPDX-License-Identifier: GPL-3.0-only

use bytes::{BufMut, Bytes, BytesMut};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::net::TcpStream;
use std::sync::atomic::AtomicU8;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::aes::AES;
use crate::audio_peer::AudioPeer;
use crate::signaling;

pub struct SignalingServer {
    username: String,
    listener: TcpListener,
    cipher: Arc<AES>,
    streams: Arc<Mutex<HashMap<u8, TcpStream>>>,
    audio_peers: Arc<Mutex<HashMap<u8, (Bytes, AudioPeer)>>>,
    index_counter: Arc<AtomicU8>,
}
impl SignalingServer {
    pub fn new(username: String) -> Self {
        let bind = signaling::get_address_ipv6();
        let listener = TcpListener::bind(bind).unwrap();

        let cipher = Arc::new(AES::new(None).unwrap());
        SignalingServer {
            username,
            listener,
            cipher,
            streams: Arc::new(Mutex::new(HashMap::new())),
            audio_peers: Arc::new(Mutex::new(HashMap::new())),
            index_counter: Arc::new(AtomicU8::new(1)),
        }
    }
    pub fn get_listen_address(&self) -> String {
        self.listener.local_addr().unwrap().to_string()
    }
    pub fn get_cipher_key(&self) -> String {
        self.cipher.get_key().clone()
    }
    pub fn run(&self, playback_name: String) {
        let listener_tryclone = self.listener.try_clone();
        if listener_tryclone.is_err() {
            panic!("Failed to clone listener");
        }
        let listener = listener_tryclone.unwrap();
        let audio_peers = self.audio_peers.clone();
        let streams = self.streams.clone();
        let aes = self.cipher.clone();
        let my_username = self.username.clone();
        let index_counter = self.index_counter.clone();
        thread::spawn(move || {
            let audio_peers = audio_peers.clone();
            let streams = streams.clone();
            let my_username = my_username.clone();
            println!("{{ \"event_code\": 1 }}");
            loop {
                let audio_peers = audio_peers.clone();
                let streams = streams.clone();
                let my_username = my_username.clone();

                let try_accept = listener.accept();
                if try_accept.is_err() {
                    error!("{:?}", try_accept.err());
                    continue;
                }
                let (mut stream, addr) = try_accept.unwrap();
                debug!("New connection from {}", addr);

                let id = index_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let welcome_prim = vec![0, id];
                let welcome_msg = Bytes::from(welcome_prim);
                let encrypted_msg = aes.encrypt(welcome_msg).unwrap();
                let _ = stream.write_all(&encrypted_msg);
                streams
                    .lock()
                    .unwrap()
                    .insert(id, stream.try_clone().unwrap());

                let aes_clone = aes.clone();

                let playback_name = playback_name.clone();
                thread::spawn(move || {
                    let recv_buffer = &mut [0u8; 1024];
                    let playback_name = playback_name.clone();
                    let streams = streams.clone();
                    loop {
                        let audio_peers = audio_peers.clone();
                        let playback_name = playback_name.clone();
                        match stream.read(recv_buffer.as_mut()) {
                            Ok(recv_len) => {
                                if recv_len == 0 {
                                    debug!("Connection closed");
                                    streams.lock().unwrap().remove(&id);
                                    audio_peers.lock().unwrap().remove(&id);
                                    println!("{{ \"event_code\": 3, \"id\": {} }}", id);
                                    streams.lock().unwrap().iter().for_each(|(sid, stream)| {
                                        let mut reply = BytesMut::with_capacity(1024);
                                        reply.put_u8(4);
                                        reply.put_u8(0);
                                        reply.put_u8(*sid);
                                        reply.put_u8(id);
                                        let encrypted_reply = aes_clone.encrypt(reply.freeze()).unwrap();
                                        let _ = stream.try_clone().as_mut().unwrap().write_all(&encrypted_reply);
                                    });

                                    return;
                                }
                                let try_decrypt =
                                    aes_clone.decrypt_vec(recv_buffer[..recv_len].to_vec());
                                if try_decrypt.is_err() {
                                    error!(
                                        "Failed to decrypt message: {}",
                                        try_decrypt.err().unwrap()
                                    );
                                    continue;
                                }

                                let decrypted = try_decrypt.unwrap();
                                debug!("Received message: {:?}", decrypted);
                                //let mut payload = Vec::new();
                                //decrypted[2..].clone_into(&mut payload);
                                let to_id = decrypted[2];
                                if to_id == 0 {
                                    let opcode = decrypted[0];
                                    let from_id = decrypted[1];

                                    match opcode {
                                        1 => {
                                            let payload = decrypted[3..].to_vec();
                                            let ip_len = 1 + payload[0] as usize;
                                            let ip_candidate =
                                                String::from_utf8(payload[1..ip_len].to_vec())
                                                    .unwrap();
                                            debug!("Received ip candidate: {}", ip_candidate);
                                            let username = Bytes::from(payload[ip_len..].to_vec());
                                            let my_addr_candidate = signaling::get_address_ipv6();
                                            let audio_peer = AudioPeer::new(
                                                my_addr_candidate.clone(),
                                                aes_clone.get_key(),
                                            );
                                            audio_peers
                                                .lock()
                                                .unwrap()
                                                .insert(from_id, (username, audio_peer));

                                            thread::spawn(move || {
                                                let unlocked_peers = audio_peers.lock().unwrap();
                                                let (usr, audio_peer) =
                                                    unlocked_peers.get(&from_id).unwrap();
                                                let username = String::from_utf8(usr.to_vec()).unwrap();
                                                println!("{{ \"event_code\": 2, \"id\": {}, \"username\": \"{}\" }}", from_id, username);
                                                audio_peer.connect(ip_candidate, playback_name.clone());
                                            });

                                            let mut reply = BytesMut::with_capacity(1024);
                                            reply.put_u8(2);
                                            reply.put_u8(0);
                                            reply.put_u8(from_id);
                                            reply.put_u8(my_addr_candidate.len() as u8);
                                            reply.put(my_addr_candidate.as_bytes());
                                            reply.put(my_username.as_bytes());

                                            let encrypted =
                                                aes_clone.encrypt(reply.freeze()).unwrap();
                                            let _ = stream.write_all(&encrypted);
                                        }
                                        2 => {
                                            error!("Received unexpected opcode 2");
                                            continue;
                                        }
                                        3 => {
                                            todo!("Change bitrate or let AudioPeer handle it");
                                        }
                                        _ => {
                                            error!("Unknown opcode {}", opcode);
                                            continue;
                                        }
                                    }
                                } else {
                                    let mut streams = streams.lock().unwrap();
                                    let stream = streams.get_mut(&to_id);
                                    if stream.is_none() {
                                        debug!("Stream {} not found", to_id);
                                        continue;
                                    }
                                    let stream = stream.unwrap();
                                    let _ = stream.write_all(&recv_buffer[..recv_len]);
                                }
                            }
                            Err(e) => {
                                debug!("Connection closed");
                                streams.lock().unwrap().remove(&id);
                                audio_peers.lock().unwrap().remove(&id);
                                println!("{{ \"event_code\": 3, \"id\": {} }}", id);
                                streams.lock().unwrap().iter().for_each(|(sid, stream)| {
                                    let mut reply = BytesMut::with_capacity(1024);
                                    reply.put_u8(4);
                                    reply.put_u8(0);
                                    reply.put_u8(*sid);
                                    reply.put_u8(id);
                                    let encrypted_reply = aes_clone.encrypt(reply.freeze()).unwrap();
                                    let _ = stream.try_clone().as_mut().unwrap().write_all(&encrypted_reply);
                                });
                                return;
                            }
                        }
                    }
                });
            }
        });
    }
    pub fn send_opus(&self, opus_packet: Bytes) {
        let peers = self.audio_peers.lock().unwrap();
        for (_, peer) in peers.values() {
            let _ = peer.send(opus_packet.clone());
        }
    }
    pub fn get_peers(&self) -> Vec<(u8, String)> {
        let peers = self.audio_peers.lock().unwrap();
        let mut result: Vec<(u8, String)> = Vec::new();
        peers.iter().for_each(|(id, (username, _))| {
            let username = String::from_utf8(username.to_vec()).unwrap();
            result.push((*id, username));
        });
        result
    }
    pub fn change_playback(&self, device_name: String, channels: u32, sample_rate: u32) {
        let peers = self.audio_peers.lock().unwrap();
        for (_, peer) in peers.values() {
            peer.change_device(device_name.clone(), channels, sample_rate);
        }
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
