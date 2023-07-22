// SPDX-FileCopyrightText: Copyright 2023 tSVoI
// SPDX-License-Identifier: GPL-3.0-only

use log::{debug, error, info, warn};
use serde::de::Error;

use crate::aes::AES;
use crate::audio::Audio;
use crate::audio::playback::AudioPlayback;
use crate::audio_peer::AudioPeer;
use crate::{signaling};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;

pub struct SignalingServer {
    username: String,
    listener: TcpListener,
    cipher: Arc<AES>,
    streams: Arc<Mutex<HashMap<u8, TcpStream>>>,
    audio_peers: Arc<Mutex<HashMap<u8, (String, String, AudioPeer)>>>,
}
impl SignalingServer {
    pub fn new(username: String) -> Self {
        let bind = signaling::get_address_ipv6();
        let listener = TcpListener::bind(bind).unwrap();

        let cipher = Arc::new(AES::new(None));
        SignalingServer {
            username,
            listener,
            cipher,
            streams: Arc::new(Mutex::new(HashMap::new())),
            audio_peers: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub fn get_listen_address(&self) -> String {
        self.listener.local_addr().unwrap().to_string()
    }
    pub fn get_cipher_key(&self) -> String {
        self.cipher.get_key().clone()
    }
    pub fn run(&self, backend:String ,playback_name: String) {
        let listener_tryclone = self.listener.try_clone();
        if listener_tryclone.is_err() {
            panic!("Failed to clone listener");
        }
        //let listener_clone = listener_tryclone.unwrap();
        //let cipher_mainclone = self.cipher.clone();
        //let peers_mainclone = self.streams.clone();
        thread::scope(|scope| {
            info!("Listening for connections");
            let bkc = backend.clone();
            loop {
                let bck = bkc.clone();
                let (mut stream, _) = self.listener.accept().unwrap();
                let mut stream_clone = stream.try_clone().unwrap();
                info!("New connection");

                let peers = self.streams.clone();

                let id = peers.lock().unwrap().len() as u8 + 1;
                let welcome_msg = format!("id¬{}", id);
                let encrypted_welcome = self.cipher.encrypt(welcome_msg);

                //The id also lets the client know how many adress candidates has to create
                stream.write_all(encrypted_welcome.as_bytes()).unwrap();

                self.streams
                    .lock()
                    .unwrap()
                    .insert(id, stream.try_clone().unwrap());
                let peers = self.streams.clone();
                let playback_clone = playback_name.clone();
                scope.spawn(move || {
                    let audio_peers = self.audio_peers.clone();
                    let bkk = bck.clone();
                    loop {
                        let backend = bkk.clone();
                        let buf = &mut [0; 2048];
                        let size = stream_clone.read(buf).unwrap();
                        if size == 0 {
                            info!("Connection closed");
                            peers.lock().unwrap().remove(&id);
                            break;
                        }
                        let encrypted = String::from_utf8_lossy(&buf[0..size]).to_string();
                        debug!("Encrypted: {}", encrypted);
                        let decrypted = self.cipher.decrypt(encrypted);
                        debug!("Decrypted: {}", decrypted);
                        //<target_id>¬<from_id>¬<event>¬<args>
                        let split: Vec<&str> = decrypted.split("¬").collect();
                        let target_id = split[0].parse::<u8>().unwrap();
                        if target_id == 0 {
                            let event = split[2];
                            match event {
                                "ann" => {
                                    let peer_id = split[1].parse::<u8>().unwrap();
                                    let peer_username = split[3].to_string();
                                    let peer_address_candidate = split[4].to_string();
            
                                    let adress_candidate = signaling::get_address_ipv6();
                                    let username = self.username.clone();
            
                                    let mut peers = audio_peers.lock().unwrap();
                                    let audio_peer = AudioPeer::new(adress_candidate.clone());
                                    let item = (peer_username, peer_address_candidate, audio_peer);
                                    peers.insert(peer_id, item);
            
                                    let ack = format!(
                                        "{}¬{}¬ack¬{}¬{}",
                                        peer_id, 0, username, adress_candidate
                                    );
                                    let encrypted = self.cipher.encrypt(ack);
                                    debug!("Sending encrypted {}", encrypted);
                                    stream.write(encrypted.as_bytes()).unwrap();
                                }
                                "ack" => {
                                    let peer_id = split[1].parse::<u8>().unwrap();
                                    let username = split[3];
                                    let address_candidate = split[4];
            
                                    let mut peers = audio_peers.lock().unwrap();
                                    peers.get_mut(&peer_id).unwrap().0 = username.to_string();
                                    peers.get_mut(&peer_id).unwrap().1 = address_candidate.to_string();
            
                                    let ok = format!("{}¬{}¬ok", peer_id, 0);
                                    let encrypted = self.cipher.encrypt(ok);
                                    debug!("Sending encrypted {}", encrypted);
                                    stream.write(encrypted.as_bytes()).unwrap();
                                }
                                "ok" => {
                                    let peer_id = split[1].parse::<u8>().unwrap();
            
                                    let ok = format!("{}¬{}¬ko", peer_id, 0);
                                    let encrypted = self.cipher.encrypt(ok);
                                    debug!("Sending encrypted {}", encrypted);
                                    stream.write(encrypted.as_bytes()).unwrap();
            
                                    let audio_peers_clone = audio_peers.clone();
                                    let playback_clone = playback_clone.clone();
                                    scope.spawn(move ||{
                                        let peers = audio_peers_clone.lock().unwrap();
                                        let item = peers.get(&peer_id);
                                        let (_, address, peer) = item.unwrap();
            
                                        let playback_id = Audio::get_device_id(backend.clone(), &playback_clone, crate::audio::DeviceKind::Playback).unwrap();
                                        let playback_config = AudioPlayback::create_config(playback_id, 2, 48_000);
                                        peer.connect(address.clone(), backend.clone() ,playback_config);
                                        loop{
                                            thread::sleep(std::time::Duration::from_millis(1));
                                        }
                                        
                                    }); 
                                }
                                "ko" => {
                                    let peer_id = split[1].parse::<u8>().unwrap();
            
                                    let audio_peers_clone = audio_peers.clone();
                                    let playback_clone = playback_clone.clone();
                                    scope.spawn(move ||{
                                        let peers = audio_peers_clone.lock().unwrap();
                                        let item = peers.get(&peer_id);
                                        let (_, address, peer) = item.unwrap();
            
                                        let playback_id = Audio::get_device_id(backend.clone(), &playback_clone, crate::audio::DeviceKind::Playback).unwrap();
                                        let playback_config = AudioPlayback::create_config(playback_id, 2, 48_000);
                                        peer.connect(address.clone(), backend.clone(), playback_config);
                                        loop{
                                            thread::sleep(std::time::Duration::from_millis(1));
                                        }
                                        
                                    }); 
                                }
                                _ => {
                                    error!("Unknown event {}", event);
                                }
                            }
                        } else {
                            let mut list = peers.lock().unwrap();
                            let try_stream = list.get(&target_id);
                            if try_stream.is_none() {
                                debug!("Peer not found");
                                continue;
                            }
                            let mut target = try_stream.unwrap();
                            //let mut target = stream.try_clone().unwrap();
                            let try_send = target.write(&buf[0..size]);
                            if try_send.is_err() {
                                error!(
                                    "Failed to send message to peer, connection is probably closed"
                                );
                                list.remove(&target_id);
                                break;
                            }
                        }
                    }
                });
            }
        });
    }
    pub fn send_opus(&self, opus_packet: Vec<u8>) {
        let peers = self.audio_peers.lock().unwrap();
        for (_, _, peer) in peers.values() {
            peer.send(opus_packet.clone());
        }
    }
    pub fn get_peers(&self) -> Vec<(u8, String)> {
        let peers = self.audio_peers.lock().unwrap();
        let mut result: Vec<(u8,String)> = Vec::new();
        peers.iter().for_each(|(id, (username, _, _))| {
            result.push((*id, username.clone()));
        });
        result
    }
    pub fn change_peer_volume(&self, peer_id: u8, volume: u8){
        let peers = self.audio_peers.lock().unwrap();
        let peer = peers.get(&peer_id);
        if peer.is_none(){
            error!("Peer {} not found", peer_id);
            return;
        }
        let (_, _, peer) = peer.unwrap();
        peer.change_volume(volume);
    }
}
